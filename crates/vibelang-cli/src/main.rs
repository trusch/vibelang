//! VibeLang CLI - using the vibelang-core async runtime
//!
//! This is the main entry point for the vibelang command-line tool.

#[cfg(feature = "midi")]
mod midi_dispatcher;
mod render;
mod startup_profile;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use vibelang_core::mutation::{
    Atomicity, CandidateOrigin, CliMode, MutationEventSink, MutationKind, MutationReceipt,
    MutationReplySink, MutationSource, ReceiptState, RequestMaterial, Submission,
    SupersessionPolicy, SupersessionReason, TerminalOutcome,
};
use vibelang_core::{
    setup_metering, setup_node_tracking, Message, ReloadMessage, Runtime, ScsynthBackend,
    ScsynthConfig, ScsynthProcess, TransportMessage,
};
use vibelang_rhai::ScriptEngine;

const RECEIPT_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

struct TrackedMutation {
    initial_receipt: MutationReceipt,
    updates: mpsc::UnboundedReceiver<MutationReceipt>,
}

impl TrackedMutation {
    async fn wait_terminal(mut self) -> Result<MutationReceipt> {
        print_receipt(&self.initial_receipt);
        if self.initial_receipt.state.is_terminal() {
            return Ok(self.initial_receipt);
        }
        let mut last_sequence = self.initial_receipt.event_sequence;
        loop {
            match tokio::time::timeout(RECEIPT_WAIT_TIMEOUT, self.updates.recv()).await {
                Ok(Some(receipt)) => {
                    if receipt.event_sequence <= last_sequence {
                        continue;
                    }
                    last_sequence = receipt.event_sequence;
                    print_receipt(&receipt);
                    if receipt.state.is_terminal() {
                        return Ok(receipt);
                    }
                }
                Ok(None) => anyhow::bail!(
                    "receipt stream closed while attempt {} remained pending after event {}",
                    self.initial_receipt.attempt_id,
                    last_sequence
                ),
                Err(_) => {
                    anyhow::bail!(
                        "timed out waiting for terminal receipt: attempt {} remains pending after event {}",
                        self.initial_receipt.attempt_id,
                        last_sequence
                    );
                }
            }
        }
    }
}

fn print_receipt(receipt: &MutationReceipt) {
    for line in receipt_projection(receipt) {
        match &receipt.state {
            ReceiptState::Terminal(TerminalOutcome::Rejected(_))
            | ReceiptState::Terminal(TerminalOutcome::Superseded(_))
            | ReceiptState::Terminal(TerminalOutcome::Partial(_)) => eprintln!("{line}"),
            _ => println!("{line}"),
        }
    }
}

fn receipt_projection(receipt: &MutationReceipt) -> Vec<String> {
    let revision = receipt
        .revision
        .map_or_else(|| "none".into(), |revision| revision.to_string());
    let mut lines = match &receipt.state {
        ReceiptState::Evaluating { phase } => vec![format!(
            "PENDING attempt={} revision={} stage=evaluating/{phase:?}",
            receipt.attempt_id, revision
        )],
        ReceiptState::Accepted { queue_position } => vec![format!(
            "PENDING attempt={} revision={} stage=accepted scope=queue_admitted queue_position={} (terminal truth pending)",
            receipt.attempt_id,
            revision,
            queue_position.map_or_else(|| "unknown".into(), |position| position.to_string())
        )],
        ReceiptState::Planning => vec![format!(
            "PENDING attempt={} revision={} stage=planning",
            receipt.attempt_id, revision
        )],
        ReceiptState::Staging { completed, total } => vec![format!(
            "PENDING attempt={} revision={} stage=staging readiness={completed}/{total}",
            receipt.attempt_id, revision
        )],
        ReceiptState::Committing { phase } => vec![format!(
            "PENDING attempt={} revision={} stage=committing/{phase:?}",
            receipt.attempt_id, revision
        )],
        ReceiptState::Terminal(TerminalOutcome::Applied(applied)) => vec![format!(
            "APPLIED attempt={} revision={} boundary={:?} confirmations={}",
            receipt.attempt_id,
            revision,
            applied.effective_at,
            applied.confirmations.len()
        )],
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => vec![format!(
            "REJECTED attempt={} revision={} phase={:?} code={} message={}",
            receipt.attempt_id, revision, rejected.phase, rejected.code, rejected.message
        )],
        ReceiptState::Terminal(TerminalOutcome::Superseded(superseded)) => {
            let reason = match superseded.reason {
                SupersessionReason::Replaced => "replaced",
                SupersessionReason::Cancelled => "cancelled",
            };
            vec![format!(
                "SUPERSEDED attempt={} revision={} reason={} by_revision={}",
                receipt.attempt_id,
                revision,
                reason,
                superseded
                    .by_revision
                    .map_or_else(|| "none".into(), |revision| revision.to_string())
            )]
        }
        ReceiptState::Terminal(TerminalOutcome::Partial(partial)) => {
            let mut lines = vec![format!(
                "PARTIAL attempt={} revision={} phase={:?} code={} fenced={} rollback={:?}",
                receipt.attempt_id,
                revision,
                partial.phase,
                partial.code,
                partial.fenced,
                partial.rollback
            )];
            for component in &partial.components {
                lines.push(format!(
                    "  component={} action={} state={:?}",
                    component.path, component.action, component.state
                ));
            }
            if partial.fenced {
                lines.push(format!(
                    "  migration: further mutations are fenced; inspect this receipt, then explicitly acknowledge continue_best_effort({}) before retrying",
                    receipt.attempt_id
                ));
            }
            lines
        }
    };
    for diagnostic in &receipt.diagnostics {
        lines.push(format!(
            "  diagnostic severity={:?} code={} component={} message={}",
            diagnostic.severity,
            diagnostic.code,
            diagnostic.component_path.as_deref().unwrap_or("none"),
            diagnostic.message
        ));
    }
    lines
}

fn require_applied(receipt: MutationReceipt) -> Result<MutationReceipt> {
    match &receipt.state {
        ReceiptState::Terminal(TerminalOutcome::Applied(_)) => Ok(receipt),
        ReceiptState::Terminal(TerminalOutcome::Rejected(rejected)) => {
            if rejected.code == "runtime_fenced" {
                anyhow::bail!(
                    "attempt {} was rejected by the runtime fence; acknowledge the current fenced Partial with continue_best_effort before retrying",
                    receipt.attempt_id
                );
            }
            anyhow::bail!(
                "attempt {} was rejected [{}]: {}; correct the cause before retrying as a new attempt",
                receipt.attempt_id,
                rejected.code,
                rejected.message
            );
        }
        ReceiptState::Terminal(TerminalOutcome::Superseded(superseded)) => anyhow::bail!(
            "attempt {} was superseded ({:?}); only the replacing or newly submitted attempt can establish terminal truth",
            receipt.attempt_id,
            superseded.reason
        ),
        ReceiptState::Terminal(TerminalOutcome::Partial(partial)) => anyhow::bail!(
            "attempt {} is Partial [{}] (fenced={}); it is not success and must not be retried before component review{}",
            receipt.attempt_id,
            partial.code,
            partial.fenced,
            if partial.fenced {
                " and explicit continue_best_effort acknowledgement"
            } else {
                ""
            }
        ),
        state => anyhow::bail!(
            "attempt {} is still pending at {state:?}; queue admission is not application",
            receipt.attempt_id
        ),
    }
}

fn cli_submission(
    handle: &vibelang_core::RuntimeHandle,
    source: &std::path::Path,
    mode: CliMode,
    kind: MutationKind,
    supersession: SupersessionPolicy,
    operation: &str,
) -> Result<Submission> {
    let source = source.to_string_lossy().into_owned();
    let material = RequestMaterial::new(
        &("compat.vibelang.v1", "cli", operation, source.as_str()),
        Some(&("compat.vibelang.v1", "cli", operation, source.as_str())),
    )?;
    Ok(Submission {
        kind,
        source: MutationSource::Cli {
            mode,
            source: Some(source),
        },
        caller_namespace: "compat.vibelang.v1.local".into(),
        idempotency_key: None,
        require_idempotency_key: false,
        retry_epoch: Some(handle.mutation_status().runtime_epoch),
        expected_revision: None,
        atomicity: Atomicity::BestEffort,
        supersession,
        material,
    })
}

async fn submit_cli_mutation(
    handle: &vibelang_core::RuntimeHandle,
    source: &std::path::Path,
    mode: CliMode,
    message: Message,
    kind: MutationKind,
    supersession: SupersessionPolicy,
) -> Result<TrackedMutation> {
    let operation = message.operation().to_lowercase();
    let submission = cli_submission(handle, source, mode, kind, supersession, &operation)?;
    let (send, updates) = mpsc::unbounded_channel();
    let reply_sink = MutationReplySink::new(move |receipt| {
        let _ = send.send(receipt);
    });
    match handle
        .submit_with_sinks(
            message,
            submission,
            reply_sink,
            MutationEventSink::default(),
        )
        .await
    {
        Ok(initial_receipt) => Ok(TrackedMutation {
            initial_receipt,
            updates,
        }),
        Err(error) => {
            let mut updates = updates;
            let mut latest = None;
            while let Ok(receipt) = updates.try_recv() {
                latest = Some(receipt);
            }
            if let Some(receipt) = latest {
                print_receipt(&receipt);
                anyhow::bail!(
                    "mutation admission failed for attempt {}: {}",
                    receipt.attempt_id,
                    error
                );
            }
            Err(error.into())
        }
    }
}

async fn submit_cli_reload(
    handle: &vibelang_core::RuntimeHandle,
    source: &std::path::Path,
    mode: CliMode,
    state: vibelang_core::reload::ScriptState,
) -> Result<TrackedMutation> {
    let origin = match mode {
        CliMode::Startup => CandidateOrigin::ScriptFile,
        CliMode::Watch => CandidateOrigin::WatchReload,
        CliMode::EvalServer => CandidateOrigin::HttpEval,
    };
    let supersession = match mode {
        CliMode::Watch => SupersessionPolicy::ReplacePending {
            key: source.to_string_lossy().into_owned(),
        },
        CliMode::Startup | CliMode::EvalServer => SupersessionPolicy::Fifo,
    };
    submit_cli_mutation(
        handle,
        source,
        mode,
        ReloadMessage::Apply { state }.into(),
        MutationKind::Candidate { origin },
        supersession,
    )
    .await
}

async fn submit_cli_command(
    handle: &vibelang_core::RuntimeHandle,
    source: &std::path::Path,
    message: Message,
) -> Result<TrackedMutation> {
    let kind = MutationKind::Command {
        domain: message.domain(),
        operation: message.operation().to_lowercase(),
    };
    submit_cli_mutation(
        handle,
        source,
        CliMode::Startup,
        message,
        kind,
        SupersessionPolicy::Fifo,
    )
    .await
}

/// VibeLang CLI - A music livecoding language
#[derive(Parser)]
#[command(name = "vibe", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Script file to run (shorthand for `vibe run <file>`)
    file: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a .vibe script
    Run {
        /// The .vibe file to run
        file: PathBuf,

        /// Disable watching for file changes (watch is enabled by default)
        #[arg(long)]
        no_watch: bool,

        /// Disable HTTP API server (API is enabled by default)
        #[arg(long)]
        #[cfg_attr(not(feature = "api"), arg(hide = true))]
        no_api: bool,

        /// HTTP API server port (default: 1606)
        #[arg(long, default_value = "1606")]
        #[cfg_attr(not(feature = "api"), arg(hide = true))]
        api_port: u16,

        /// HTTP API bind address (default: 127.0.0.1; use 0.0.0.0 to expose on all interfaces)
        #[arg(long, default_value = "127.0.0.1", value_name = "ADDR")]
        #[cfg_attr(not(feature = "api"), arg(hide = true))]
        api_bind: std::net::IpAddr,

        /// Allow code sent to the HTTP /eval endpoint to use fs/exec/net extensions
        /// (by default /eval runs sandboxed even when the local script has them)
        #[arg(long)]
        #[cfg_attr(not(feature = "api"), arg(hide = true))]
        api_allow_extensions: bool,

        /// Include paths for imports
        #[arg(short = 'I', long = "include")]
        include_paths: Vec<PathBuf>,

        /// SuperCollider server address
        #[arg(long, default_value = "127.0.0.1:57110")]
        scsynth_addr: String,

        /// Don't start scsynth automatically
        #[arg(long)]
        no_boot: bool,

        /// Don't auto-connect JACK/PipeWire ports to system audio output
        #[arg(long)]
        no_jack_connect: bool,

        /// Manually specify JACK/PipeWire output ports to connect to (for audio output).
        /// Format: comma-separated list of ports, one per output channel.
        /// Example: "Device:playback_FL,Device:playback_FR"
        /// Use `pw-link -i` or `jack_lsp` to list available input ports.
        #[arg(long, value_name = "PORTS")]
        jack_connect_to: Option<String>,

        /// Manually specify JACK/PipeWire input ports to connect from (for audio input).
        /// Format: comma-separated list of ports, one per input channel.
        /// Example: "Device:capture_1,Device:capture_2,Device:capture_3,Device:capture_4"
        /// Use `pw-link -o` or `jack_lsp` to list available output ports.
        #[arg(long, value_name = "PORTS")]
        jack_connect_from: Option<String>,

        /// Audio device name (e.g., "default", "hw:0", "Focusrite USB ASIO")
        #[arg(long)]
        device: Option<String>,

        /// Sample rate (e.g., 44100, 48000, 96000). 0 = hardware default.
        #[arg(long, default_value = "0")]
        sample_rate: u32,

        /// Number of input channels (default: 2)
        #[arg(long)]
        input_channels: Option<u32>,

        /// Number of output channels (default: 2)
        #[arg(long)]
        output_channels: Option<u32>,

        /// Startup profile with explicit audio links and readiness requirements
        #[arg(long, value_name = "FILE")]
        profile: Option<PathBuf>,

        /// Disable all script extensions (filesystem, exec, networking)
        #[arg(long)]
        #[cfg_attr(not(feature = "extensions"), arg(hide = true))]
        no_extensions: bool,

        /// Disable filesystem extension (read_file, write_file, etc.)
        #[arg(long)]
        #[cfg_attr(not(feature = "ext-fs"), arg(hide = true))]
        no_fs: bool,

        /// Disable shell command execution extension
        #[arg(long)]
        #[cfg_attr(not(feature = "ext-exec"), arg(hide = true))]
        no_exec: bool,

        /// Disable networking extension (HTTP fetch)
        #[arg(long)]
        #[cfg_attr(not(feature = "ext-net"), arg(hide = true))]
        no_net: bool,

        /// Base directory for filesystem sandboxing (restricts file operations to this directory)
        #[arg(long)]
        #[cfg_attr(not(feature = "ext-fs"), arg(hide = true))]
        fs_sandbox: Option<String>,
    },

    /// Render a .vibescore file to audio
    Render {
        /// Input .vibescore file
        score_file: PathBuf,

        /// Output audio file
        output: PathBuf,

        /// Output format (wav, mp3, flac, ogg)
        #[arg(short, long)]
        format: Option<String>,

        /// Sample rate (default: 48000)
        #[arg(short, long, default_value = "48000")]
        sample_rate: u32,

        /// Bit depth (16, 24, 32)
        #[arg(short, long, default_value = "24")]
        bit_depth: u8,
    },

    /// List available MIDI devices
    #[cfg(feature = "midi")]
    Devices,

    /// Start the Language Server Protocol server (for IDE integration)
    #[cfg(feature = "lsp")]
    Lsp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Handle shorthand: `vibe file.vibe` -> `vibe run file.vibe`
    let command = if let Some(file) = cli.file {
        Commands::Run {
            file,
            no_watch: false,
            no_api: false,
            no_jack_connect: false,
            jack_connect_to: None,
            jack_connect_from: None,
            api_port: 1606,
            api_bind: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            api_allow_extensions: false,
            include_paths: Vec::new(),
            scsynth_addr: "127.0.0.1:57110".to_string(),
            no_boot: false,
            device: None,
            sample_rate: 0,
            input_channels: None,
            output_channels: None,
            profile: None,
            no_extensions: false,
            no_fs: false,
            no_exec: false,
            no_net: false,
            fs_sandbox: None,
        }
    } else {
        cli.command.unwrap_or_else(|| {
            eprintln!("Usage: vibe <file.vibe> or vibe <command>");
            eprintln!("Try 'vibe --help' for more information.");
            std::process::exit(1);
        })
    };

    match command {
        Commands::Run {
            file,
            no_watch,
            no_api,
            api_port,
            api_bind,
            api_allow_extensions,
            include_paths,
            scsynth_addr,
            no_boot,
            no_jack_connect,
            jack_connect_to,
            jack_connect_from,
            device,
            sample_rate,
            input_channels,
            output_channels,
            profile,
            no_extensions,
            no_fs,
            no_exec,
            no_net,
            fs_sandbox,
        } => {
            // Build extension config
            let ext_config =
                build_extension_config(no_extensions, no_fs, no_exec, no_net, fs_sandbox);

            let watch = !no_watch;
            let api = !no_api;
            run_simple_mode(
                file,
                watch,
                api,
                api_port,
                api_bind,
                api_allow_extensions,
                include_paths,
                scsynth_addr,
                no_boot,
                no_jack_connect,
                jack_connect_to,
                jack_connect_from,
                device,
                sample_rate,
                input_channels,
                output_channels,
                profile,
                ext_config,
            )
            .await
        }
        Commands::Render {
            score_file,
            output,
            format,
            sample_rate,
            bit_depth,
        } => render::render(render::RenderArgs {
            score_file,
            output,
            format,
            sample_rate,
            bit_depth,
        }),
        #[cfg(feature = "midi")]
        Commands::Devices => {
            list_midi_devices();
            Ok(())
        }
        #[cfg(feature = "lsp")]
        Commands::Lsp => vibelang_lsp::run_lsp_server().await,
    }
}

/// Run in simple mode (no TUI, just console output)
#[allow(clippy::too_many_arguments)]
async fn run_simple_mode(
    file: PathBuf,
    watch: bool,
    #[allow(unused_variables)] api: bool,
    #[allow(unused_variables)] api_port: u16,
    #[allow(unused_variables)] api_bind: std::net::IpAddr,
    #[allow(unused_variables)] api_allow_extensions: bool,
    include_paths: Vec<PathBuf>,
    scsynth_addr: String,
    no_boot: bool,
    no_jack_connect: bool,
    jack_connect_to: Option<String>,
    jack_connect_from: Option<String>,
    device: Option<String>,
    sample_rate: u32,
    requested_input_channels: Option<u32>,
    requested_output_channels: Option<u32>,
    profile_path: Option<PathBuf>,
    ext_config: ExtensionSettings,
) -> Result<()> {
    // Initialize logging - uses RUST_LOG env var
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let profile_path =
        match startup_profile::StartupProfile::resolve_path(&file, profile_path.as_deref()) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("FAILED {error:#}");
                return Err(error);
            }
        };
    let startup_profile = match profile_path.as_deref() {
        Some(path) => match startup_profile::StartupProfile::load(path) {
            Ok(profile) => Some(profile),
            Err(error) => {
                eprintln!("FAILED {error:#}");
                return Err(error);
            }
        },
        None => None,
    };
    let (input_channels, output_channels) = if let Some(profile) = &startup_profile {
        match profile.resolve_channel_counts(requested_input_channels, requested_output_channels) {
            Ok(counts) => counts,
            Err(error) => {
                eprintln!("{error}");
                return Err(error);
            }
        }
    } else {
        (
            requested_input_channels.unwrap_or(2),
            requested_output_channels.unwrap_or(2),
        )
    };
    let device = if let Some(profile) = &startup_profile {
        match profile.resolve_device(device) {
            Ok(device) => device,
            Err(error) => {
                eprintln!("{error}");
                return Err(error);
            }
        }
    } else {
        device
    };
    if startup_profile.is_some() && (jack_connect_to.is_some() || jack_connect_from.is_some()) {
        let error = anyhow::anyhow!(
            "FAILED --profile cannot be combined with --jack-connect-to or --jack-connect-from"
        );
        eprintln!("{error}");
        return Err(error);
    }
    if let Some(profile) = &startup_profile {
        let missing = profile.inactive_required_services();
        if !missing.is_empty() {
            eprintln!("WAITING profile '{}'", profile.name);
            for cause in missing {
                eprintln!("  required: {cause}");
            }
            anyhow::bail!("required startup services are unavailable; Transport Start withheld");
        }
    }

    // Setup shutdown signal
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C, shutting down...");
        r.store(false, Ordering::SeqCst);
    })?;

    // Start or connect to scsynth
    let _process = if no_boot {
        info!("Connecting to scsynth at {}", scsynth_addr);
        None
    } else {
        info!("Starting scsynth...");
        let profile_output_destinations = startup_profile
            .as_ref()
            .filter(|profile| profile.manages_links())
            .map(startup_profile::StartupProfile::output_destinations);
        let profile_input_sources = startup_profile
            .as_ref()
            .filter(|profile| profile.manages_links())
            .map(startup_profile::StartupProfile::input_sources);
        let mut config = ScsynthConfig::default()
            .auto_connect_jack(!no_jack_connect && startup_profile.is_none())
            .sample_rate(sample_rate)
            .input_channels(input_channels)
            .output_channels(output_channels)
            .verbose(true); // Enable verbose mode for debugging
        if let Some(dev) = &device {
            config = config.device(dev);
        }
        // Parse manual JACK output connection targets if specified
        if let Some(ref targets) = profile_output_destinations {
            config = config.jack_connect_outputs(targets.clone());
        } else if let Some(ref targets) = jack_connect_to {
            let ports: Vec<String> = targets.split(',').map(|s| s.trim().to_string()).collect();
            if ports.is_empty() || ports.iter().any(|p| p.is_empty()) {
                anyhow::bail!(
                    "Invalid --jack-connect-to format. Expected comma-separated port names, got: {}",
                    targets
                );
            }
            config = config.jack_connect_outputs(ports);
        }
        // Parse manual JACK input connection sources if specified
        if let Some(ref sources) = profile_input_sources {
            config = config.jack_connect_inputs(sources.clone());
        } else if let Some(ref sources) = jack_connect_from {
            let ports: Vec<String> = sources.split(',').map(|s| s.trim().to_string()).collect();
            if ports.is_empty() || ports.iter().any(|p| p.is_empty()) {
                anyhow::bail!(
                    "Invalid --jack-connect-from format. Expected comma-separated port names, got: {}",
                    sources
                );
            }
            config = config.jack_connect_inputs(ports);
        }
        let process = ScsynthProcess::spawn(config).context("Failed to start scsynth")?;
        process.wait_startup(Duration::from_secs(3)).await;
        info!("scsynth started");

        // Handle JACK port connections
        let externally_managed_links = startup_profile
            .as_ref()
            .is_some_and(|profile| !profile.manages_links());
        if !externally_managed_links {
            if no_jack_connect
                && startup_profile.is_none()
                && jack_connect_to.is_none()
                && jack_connect_from.is_none()
            {
                // Disconnect all ports that scsynth's JACK driver may have auto-connected
                process.disconnect_all_jack_ports();
            } else {
                // Auto-connect or use manual targets
                process.auto_connect_jack_ports();
            }
        }

        Some(process)
    };

    if let Some(profile) = &startup_profile {
        let report = match profile.wait_for_readiness() {
            Ok(report) => report,
            Err(error) => {
                eprintln!("WAITING profile '{}': {error:#}", profile.name);
                anyhow::bail!("startup readiness probe failed; Transport Start withheld");
            }
        };
        if !report.allow_transport_start {
            eprintln!("{}", report.format_status(&profile.name));
            anyhow::bail!("startup readiness gate blocked Transport Start");
        }
        println!("{}", profile.format_mapping(report.state));
        if !report.optional_missing.is_empty() {
            eprintln!("{}", report.format_status(&profile.name));
        }
    }

    // Connect to scsynth
    let backend = ScsynthBackend::connect(&scsynth_addr)
        .await
        .context("Failed to connect to scsynth")?;

    // Create runtime — thread the actual scsynth -i/-o channel counts in so
    // the audio bus allocator starts past the hardware I/O block instead of
    // colliding with hardware input buses on setups with >16 hardware buses.
    let mut runtime = Runtime::new_with_audio_config(backend, output_channels, input_channels);
    let handle = runtime.handle();

    // Set up metering callback to receive SendTrig messages from link synths
    setup_metering(runtime.backend(), runtime.state().clone());

    // Set up node tracking callback to remove ended nodes from voice state
    // This prevents "node not found" errors when synths free themselves via doneAction=2
    setup_node_tracking(runtime.backend(), runtime.state().clone());

    // Load built-in synthdefs
    info!("Loading built-in synthdefs...");
    runtime
        .load_builtins()
        .await
        .context("Failed to load built-in synthdefs")?;

    // Set up deploy callback for custom synthdefs defined in scripts
    let deploy_handle = handle.clone();
    vibelang_dsp::set_deploy_callback(move |bytes| {
        let name = extract_synthdef_name(&bytes).unwrap_or_else(|| "unknown".to_string());
        tracing::debug!(
            "Deploy callback: queuing synthdef '{}' ({} bytes)",
            name,
            bytes.len()
        );
        deploy_handle
            .try_send(vibelang_core::Message::SynthDef(
                vibelang_core::SynthDefMessage::Load { name, data: bytes },
            ))
            .map_err(|e| {
                tracing::error!("Deploy callback: failed to queue synthdef: {}", e);
                e.to_string()
            })
    });

    // Get state handle before spawning runtime (needed for HTTP API)
    let state_handle = runtime.state().clone();

    // Spawn the MIDI dispatcher task. It owns the latest compiled AST + script
    // FnPtr callback map, and invokes the right closure for each runtime-emitted
    // MidiEventNotification. Captured `running` flag drives shutdown.
    #[cfg(feature = "midi")]
    let midi_dispatch_tx = {
        let dispatcher_engine = Arc::new(make_dispatcher_engine());
        midi_dispatcher::spawn(
            dispatcher_engine,
            runtime.midi_callback_receiver(),
            running.clone(),
        )
    };

    // Start HTTP API server if requested (before spawning runtime task since runtime is moved)
    #[cfg(feature = "api")]
    if api {
        // Create eval channel for code evaluation
        let (eval_tx, eval_rx) = std::sync::mpsc::channel::<vibelang_http::EvalJob>();

        let api_handle = handle.clone();
        let api_state = state_handle.clone();
        tokio::spawn(async move {
            vibelang_http::start_server(api_handle, api_state, api_bind, api_port, Some(eval_tx))
                .await;
        });
        info!("HTTP API server started on {}:{}", api_bind, api_port);

        // Spawn eval handler task. Code from the /eval endpoint runs without
        // fs/exec/net extensions unless --api-allow-extensions is set, even
        // when the local script has them enabled.
        let eval_include_paths = include_paths.clone();
        let eval_ext_config = if api_allow_extensions {
            ext_config.clone()
        } else {
            if ext_config.filesystem || ext_config.exec || ext_config.networking {
                info!(
                    "HTTP /eval runs sandboxed (no fs/exec/net extensions); \
                     pass --api-allow-extensions to enable them"
                );
            }
            ExtensionSettings::default()
        };
        let eval_handle = handle.clone();
        tokio::spawn(async move {
            while let Ok(job) = eval_rx.recv() {
                let vibelang_http::EvalJob {
                    code,
                    submission,
                    latest_receipt,
                    reply_sink,
                    event_sink,
                    response_tx,
                } = job;
                let result = match evaluate_code(&code, &eval_include_paths, &eval_ext_config) {
                    Ok(state) => {
                        let submission_result = eval_handle
                            .submit_with_sinks(
                                ReloadMessage::Apply { state }.into(),
                                submission,
                                reply_sink,
                                event_sink,
                            )
                            .await;
                        let receipt = match submission_result {
                            Ok(receipt) => Some(receipt),
                            Err(_) => latest_receipt
                                .lock()
                                .ok()
                                .and_then(|latest| latest.clone())
                                .filter(|receipt| receipt.state.is_terminal()),
                        };
                        match receipt {
                            Some(receipt) => vibelang_http::EvalResult {
                                success: true,
                                result: Some("Code evaluated and submitted".to_string()),
                                error: None,
                                receipt: Some(receipt),
                            },
                            None => vibelang_http::EvalResult {
                                success: true,
                                result: None,
                                error: Some(
                                    "Evaluation succeeded but mutation dispatch failed without a canonical receipt"
                                        .to_string(),
                                ),
                                receipt: None,
                            },
                        }
                    }
                    Err(e) => vibelang_http::EvalResult {
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                        receipt: None,
                    },
                };
                let _ = response_tx.send(result);
            }
        });
    }

    // Spawn runtime task
    let runtime_running = running.clone();
    let runtime_task = tokio::spawn(async move {
        while runtime_running.load(Ordering::SeqCst) {
            runtime.tick().await;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });

    // Execute initial script
    info!("Loading script: {}", file.display());
    let output = execute_script(&file, &include_paths, &ext_config)?;

    // Wait for all synthdefs queued during script execution to be loaded
    // This ensures modulators and voices can find their synthdefs
    info!("Waiting for synthdefs to be loaded...");
    handle.sync_and_wait().await.context(
        "backend synchronization failed before candidate submission; no reload was reported applied",
    )?;
    info!("Synthdefs loaded, applying state...");

    // Check if script requested early exit (for integration tests)
    if let Some(exit_code) = vibelang_rhai::get_exit_code() {
        info!("Script requested exit with code {}", exit_code);
        let terminal = submit_cli_reload(&handle, &file, CliMode::Startup, output.state)
            .await?
            .wait_terminal()
            .await?;
        require_applied(terminal)?;
        // Stop the running flag so the runtime task exits
        running.store(false, Ordering::SeqCst);
        // Exit with the requested code
        std::process::exit(exit_code);
    }

    // Push the initial AST + callback map to the dispatcher before applying
    // state, so the first MIDI event after reconciliation finds the right FnPtr.
    #[cfg(feature = "midi")]
    {
        let _ = midi_dispatch_tx
            .send(midi_dispatcher::DispatchState {
                ast: output.ast,
                callbacks: output.midi_callbacks,
            })
            .await;
    }

    let initial_terminal = submit_cli_reload(&handle, &file, CliMode::Startup, output.state)
        .await?
        .wait_terminal()
        .await?;
    require_applied(initial_terminal)?;

    // Wait for state to be fully applied before starting transport
    // This ensures MIDI devices are registered before transport starts,
    // so they receive the Start message
    handle.sync_and_wait().await.context(
        "backend synchronization failed after the reload receipt; Transport Start withheld",
    )?;
    info!("State applied");

    // Start transport
    let start_terminal =
        submit_cli_command(&handle, &file, Message::Transport(TransportMessage::Start))
            .await?
            .wait_terminal()
            .await?;
    require_applied(start_terminal)?;
    info!("Transport started");

    // Watch for changes if requested
    // Keep watcher alive until end of function, otherwise it gets dropped and stops watching
    let _watcher: Option<RecommendedWatcher> = if watch {
        let (tx, mut rx) = mpsc::channel::<PathBuf>(16);
        let file_clone = file.clone();
        let include_paths_clone = include_paths.clone();
        let ext_config_clone = ext_config.clone();
        let handle_clone = handle.clone();
        #[cfg(feature = "midi")]
        let midi_dispatch_tx_clone = midi_dispatch_tx.clone();

        // Setup file watcher
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if event.kind.is_modify() || event.kind.is_create() {
                        for path in event.paths {
                            // Only reload for vibe script files. Editor swap
                            // files, git objects, and WAVs written by record(...)
                            // land in the watched tree and must not trigger a
                            // full ~600ms eval+reconcile.
                            if path
                                .extension()
                                .and_then(|ext| ext.to_str())
                                .map(|ext| ext.eq_ignore_ascii_case("vibe"))
                                .unwrap_or(false)
                            {
                                let _ = tx.blocking_send(path);
                            }
                        }
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        // Watch the script file and its directory
        watcher.watch(file.parent().unwrap_or(&file), RecursiveMode::Recursive)?;
        info!("Watching for changes...");

        // Handle file change events
        let watch_running = running.clone();
        tokio::spawn(async move {
            while watch_running.load(Ordering::SeqCst) {
                if let Some(_path) = rx.recv().await {
                    // Debounce - wait a bit for multiple changes to settle
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    // Drain any pending events
                    while rx.try_recv().is_ok() {}

                    info!("File changed, reloading...");
                    match execute_script(&file_clone, &include_paths_clone, &ext_config_clone) {
                        Ok(output) => {
                            #[cfg(feature = "midi")]
                            {
                                let _ = midi_dispatch_tx_clone
                                    .send(midi_dispatcher::DispatchState {
                                        ast: output.ast,
                                        callbacks: output.midi_callbacks,
                                    })
                                    .await;
                            }
                            match submit_cli_reload(
                                &handle_clone,
                                &file_clone,
                                CliMode::Watch,
                                output.state,
                            )
                            .await
                            {
                                Ok(tracked) => match tracked.wait_terminal().await {
                                    Ok(receipt) => match &receipt.state {
                                        ReceiptState::Terminal(TerminalOutcome::Applied(_)) => {
                                            info!(
                                                "Reload applied at revision {}",
                                                receipt.revision.map_or_else(
                                                    || "none".into(),
                                                    |revision| revision.to_string()
                                                )
                                            );
                                        }
                                        ReceiptState::Terminal(TerminalOutcome::Partial(_)) => {
                                            error!(
                                                "Watch submissions stopped after Partial attempt {}; status and receipt reads remain available",
                                                receipt.attempt_id
                                            );
                                            break;
                                        }
                                        ReceiptState::Terminal(TerminalOutcome::Rejected(_))
                                        | ReceiptState::Terminal(TerminalOutcome::Superseded(_)) => {
                                            error!(
                                                "Reload attempt {} was not applied; continuing to watch for a corrected change",
                                                receipt.attempt_id
                                            );
                                        }
                                        _ => {
                                            unreachable!("wait_terminal returned a pending receipt")
                                        }
                                    },
                                    Err(error) => {
                                        error!(
                                            "Watch submissions stopped without terminal truth: {error}"
                                        );
                                        break;
                                    }
                                },
                                Err(error) => {
                                    error!("Failed to submit reload: {error}");
                                }
                            }
                        }
                        Err(e) => {
                            error!("Script error: {}", e);
                        }
                    }
                }
            }
        });

        Some(watcher)
    } else {
        None
    };

    // Wait for shutdown
    while running.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Stop transport
    let stop_terminal =
        submit_cli_command(&handle, &file, Message::Transport(TransportMessage::Stop))
            .await?
            .wait_terminal()
            .await?;
    require_applied(stop_terminal)?;
    info!("Transport stopped");

    // Wait for runtime task
    runtime_task.abort();

    info!("Goodbye!");
    Ok(())
}

/// Run in TUI mode
#[allow(clippy::too_many_arguments)]
/// Extension configuration for scripts.
/// This is a simple struct that mirrors vibelang_rhai::ExtensionConfig
/// but is always available regardless of feature flags.
#[derive(Debug, Clone, Default)]
struct ExtensionSettings {
    pub filesystem: bool,
    pub exec: bool,
    pub networking: bool,
    pub fs_base_path: Option<String>,
}

/// Build extension configuration from CLI flags.
fn build_extension_config(
    no_extensions: bool,
    no_fs: bool,
    no_exec: bool,
    no_net: bool,
    fs_sandbox: Option<String>,
) -> ExtensionSettings {
    if no_extensions {
        // All extensions disabled
        ExtensionSettings::default()
    } else {
        ExtensionSettings {
            filesystem: !no_fs,
            exec: !no_exec,
            networking: !no_net,
            fs_base_path: fs_sandbox,
        }
    }
}

/// Output of a script execution — always includes the resolved [`ScriptState`];
/// when `midi` is on, also includes the compiled AST and any `FnPtr` callbacks
/// the script registered via `mpk.on_note(...)` / `on_cc(...)` / etc., so the
/// CLI can hand them off to `midi_dispatcher` for live event delivery.
struct ScriptOutput {
    state: vibelang_core::reload::ScriptState,
    #[cfg(feature = "midi")]
    ast: rhai::AST,
    #[cfg(feature = "midi")]
    midi_callbacks: std::collections::HashMap<u64, rhai::FnPtr>,
}

fn execute_script(
    file: &PathBuf,
    include_paths: &[PathBuf],
    ext_settings: &ExtensionSettings,
) -> Result<ScriptOutput> {
    let mut engine = ScriptEngine::new();

    // Register extensions if enabled
    #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
    {
        let mut config = vibelang_rhai::ExtensionConfig::new();
        if ext_settings.filesystem {
            config.filesystem = true;
        }
        if ext_settings.exec {
            config.exec = true;
        }
        if ext_settings.networking {
            config.networking = true;
        }
        if let Some(ref base_path) = ext_settings.fs_base_path {
            config.fs_base_path = Some(base_path.clone());
        }
        engine.register_extensions(&config);
    }
    #[cfg(not(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net")))]
    {
        let _ = ext_settings; // Suppress unused warning
    }

    // Add import paths
    for path in include_paths {
        engine.add_import_path(path.clone());
    }

    // Add stdlib path (auto-extracts if needed)
    let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());
    engine.add_import_path(stdlib_path.clone());
    // Also add parent so "stdlib/..." imports work
    if let Some(parent) = stdlib_path.parent() {
        engine.add_import_path(parent.to_path_buf());
    }

    // Use execute_file which sets up the module resolver properly.
    // With `midi`, also capture the compiled AST + registered FnPtr callbacks
    // so the CLI's `midi_dispatcher` can call them when MIDI events arrive.
    #[cfg(feature = "midi")]
    {
        let (state, ast, midi_callbacks) = engine
            .execute_file_full(file)
            .map_err(|e| anyhow::anyhow!("Script error: {}", e))?;
        Ok(ScriptOutput {
            state,
            ast,
            midi_callbacks,
        })
    }
    #[cfg(not(feature = "midi"))]
    {
        let state = engine
            .execute_file(file)
            .map_err(|e| anyhow::anyhow!("Script error: {}", e))?;
        Ok(ScriptOutput { state })
    }
}

/// Evaluate a code string dynamically (for /eval endpoint).
#[cfg(feature = "api")]
fn evaluate_code(
    code: &str,
    include_paths: &[PathBuf],
    ext_settings: &ExtensionSettings,
) -> Result<vibelang_core::reload::ScriptState> {
    let mut engine = ScriptEngine::new();

    // Register extensions if enabled
    #[cfg(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net"))]
    {
        let mut config = vibelang_rhai::ExtensionConfig::new();
        if ext_settings.filesystem {
            config.filesystem = true;
        }
        if ext_settings.exec {
            config.exec = true;
        }
        if ext_settings.networking {
            config.networking = true;
        }
        if let Some(ref base_path) = ext_settings.fs_base_path {
            config.fs_base_path = Some(base_path.clone());
        }
        engine.register_extensions(&config);
    }
    #[cfg(not(any(feature = "ext-fs", feature = "ext-exec", feature = "ext-net")))]
    {
        let _ = ext_settings;
    }

    // Add import paths
    for path in include_paths {
        engine.add_import_path(path.clone());
    }

    // Add stdlib path (auto-extracts if needed)
    let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());
    engine.add_import_path(stdlib_path.clone());
    if let Some(parent) = stdlib_path.parent() {
        engine.add_import_path(parent.to_path_buf());
    }

    // Execute the code string
    let state = engine
        .execute(code)
        .map_err(|e| anyhow::anyhow!("Eval error: {}", e))?;

    Ok(state)
}

/// Build a Rhai engine for dispatching MIDI callbacks.
///
/// The dispatcher engine is separate from the per-execution `ScriptEngine` so
/// it can outlive script reloads and be shared across the dispatcher task.
/// It registers the same VibeLang API + DSP API used during script execution
/// so user callbacks (e.g. `print(n)`) resolve normally.
#[cfg(feature = "midi")]
fn make_dispatcher_engine() -> rhai::Engine {
    let mut engine = rhai::Engine::new();
    engine.set_max_expr_depths(4096, 4096);
    engine.set_max_call_levels(4096);
    engine.on_print(|text| log::info!("[script] {}", text));
    engine.on_debug(|text, source, pos| {
        let loc = match (source, pos) {
            (Some(src), pos) if !pos.is_none() => format!(" ({}:{})", src, pos),
            (Some(src), _) => format!(" ({})", src),
            (None, pos) if !pos.is_none() => format!(" ({})", pos),
            _ => String::new(),
        };
        log::debug!("[script]{} {}", loc, text);
    });
    vibelang_rhai::api::register_api(&mut engine);
    vibelang_dsp::register_dsp_api(&mut engine);
    engine
}

/// Extract the synthdef name from SuperCollider synthdef bytes.
fn extract_synthdef_name(bytes: &[u8]) -> Option<String> {
    // Minimum size: 4 (magic) + 4 (version) + 2 (count) + 1 (name length) + 1 (at least one char)
    if bytes.len() < 12 {
        return None;
    }

    // Check magic "SCgf"
    if &bytes[0..4] != b"SCgf" {
        return None;
    }

    // Skip version (4 bytes) and count (2 bytes)
    // Name starts at offset 10
    let name_len = bytes[10] as usize;
    if bytes.len() < 11 + name_len {
        return None;
    }

    String::from_utf8(bytes[11..11 + name_len].to_vec()).ok()
}

#[cfg(feature = "midi")]
fn list_midi_devices() {
    use midir::{MidiInput, MidiOutput};

    println!("MIDI Input Devices:");
    println!("-------------------");
    if let Ok(midi_in) = MidiInput::new("vibelang-cli2") {
        for (i, port) in midi_in.ports().iter().enumerate() {
            if let Ok(name) = midi_in.port_name(port) {
                println!("  {}: {}", i, name);
            }
        }
    }

    println!();
    println!("PipeWire MIDI 2.0 / UMP Input Devices:");
    println!("--------------------------------------");
    for dev in vibelang_core::midi::list_pipewire_midi2_inputs() {
        println!("  {}: {}", dev.id.raw(), dev.name);
    }

    println!();
    println!("MIDI Output Devices:");
    println!("--------------------");
    if let Ok(midi_out) = MidiOutput::new("vibelang-cli2") {
        for (i, port) in midi_out.ports().iter().enumerate() {
            if let Ok(name) = midi_out.port_name(port) {
                println!("  {}: {}", i, name);
            }
        }
    }
}

#[cfg(test)]
mod receipt_tests {
    use super::*;
    use vibelang_core::mutation::{
        ComponentOutcome, ComponentState, Diagnostic, DiagnosticSeverity, EventSequence,
        FailurePhase, Partial, ReceiptTimestamps, Rejected, RequestIdentity, RevisionId,
        RollbackState, RuntimeEpoch, Superseded, Timestamp, MUTATION_SCHEMA_VERSION,
    };

    fn receipt(state: ReceiptState) -> MutationReceipt {
        let now = Timestamp::parse("2026-07-17T08:00:00Z").unwrap();
        MutationReceipt {
            schema_version: MUTATION_SCHEMA_VERSION,
            attempt_id: vibelang_core::mutation::AttemptId::new(),
            runtime_epoch: RuntimeEpoch::new(),
            revision: Some(RevisionId::new(7).unwrap()),
            event_sequence: EventSequence::new(11).unwrap(),
            request: RequestIdentity {
                kind: MutationKind::Candidate {
                    origin: CandidateOrigin::WatchReload,
                },
                source: MutationSource::Cli {
                    mode: CliMode::Watch,
                    source: Some("song.vibe".into()),
                },
                submission_digest: None,
                operation_digest: None,
                idempotency_key_present: false,
                expected_revision: None,
                atomicity: Atomicity::BestEffort,
                supersession: SupersessionPolicy::ReplacePending {
                    key: "song.vibe".into(),
                },
            },
            state,
            previous_confirmed_revision: Some(RevisionId::new(6).unwrap()),
            timestamps: ReceiptTimestamps {
                submitted_at: now.clone(),
                accepted_at: Some(now.clone()),
                last_transition_at: now,
                terminal_at: None,
            },
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn queue_admission_is_pending_not_terminal_success() {
        let accepted = receipt(ReceiptState::Accepted {
            queue_position: Some(2),
        });
        let projection = receipt_projection(&accepted).join("\n");

        assert!(projection.starts_with("PENDING"));
        assert!(projection.contains("scope=queue_admitted"));
        assert!(projection.contains("terminal truth pending"));
        assert!(!projection.contains("APPLIED"));
        assert!(require_applied(accepted)
            .unwrap_err()
            .to_string()
            .contains("queue admission is not application"));
    }

    #[tokio::test]
    async fn terminal_wait_preserves_readiness_and_late_partial_truth() {
        let accepted = receipt(ReceiptState::Accepted {
            queue_position: Some(1),
        });
        let mut staging = accepted.clone();
        staging.event_sequence = EventSequence::new(12).unwrap();
        staging.state = ReceiptState::Staging {
            completed: 1,
            total: 2,
        };
        let mut partial = accepted.clone();
        partial.event_sequence = EventSequence::new(13).unwrap();
        partial.state = ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::BackendBarrier,
            code: "backend_sync_failed".into(),
            components: Vec::new(),
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: Some(RevisionId::new(6).unwrap()),
        }));
        let (send, updates) = mpsc::unbounded_channel();
        send.send(accepted.clone()).unwrap();
        send.send(staging).unwrap();
        send.send(partial.clone()).unwrap();
        drop(send);

        let terminal = TrackedMutation {
            initial_receipt: accepted,
            updates,
        }
        .wait_terminal()
        .await
        .unwrap();

        assert_eq!(terminal, partial);
        assert!(require_applied(terminal).is_err());
    }

    #[test]
    fn late_partial_overrides_accepted_and_fences_retry() {
        let mut partial = receipt(ReceiptState::Terminal(TerminalOutcome::Partial(Partial {
            phase: FailurePhase::BackendBarrier,
            code: "backend_sync_failed".into(),
            components: vec![ComponentOutcome {
                path: "reload/routes".into(),
                action: "reconcile".into(),
                state: ComponentState::Uncertain,
                effective_at: None,
                confirmation: None,
                diagnostic: None,
            }],
            rollback: RollbackState::Uncertain,
            fenced: true,
            last_confirmed_revision: Some(RevisionId::new(6).unwrap()),
        })));
        partial.diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            code: "backend_sync_failed".into(),
            message: "scsynth rejected the barrier".into(),
            component_path: Some("reload/routes".into()),
            source_span: None,
        });
        let projection = receipt_projection(&partial).join("\n");

        assert!(projection.starts_with("PARTIAL"));
        assert!(projection.contains("component=reload/routes"));
        assert!(projection.contains("fenced=true"));
        assert!(projection.contains("continue_best_effort"));
        assert!(!projection.contains("APPLIED"));
        let error = require_applied(partial).unwrap_err().to_string();
        assert!(error.contains("is Partial"));
        assert!(error.contains("explicit continue_best_effort acknowledgement"));
    }

    #[test]
    fn sync_failure_remains_structured_rejection() {
        let rejected = receipt(ReceiptState::Terminal(TerminalOutcome::Rejected(
            Rejected {
                phase: FailurePhase::BackendBarrier,
                code: "sync_timeout".into(),
                message: "backend barrier timed out".into(),
                rollback: RollbackState::NotNeeded,
                preserved_revision: Some(RevisionId::new(6).unwrap()),
            },
        )));
        let projection = receipt_projection(&rejected).join("\n");

        assert!(projection.contains("phase=BackendBarrier"));
        assert!(projection.contains("code=sync_timeout"));
        assert!(require_applied(rejected)
            .unwrap_err()
            .to_string()
            .contains("correct the cause before retrying"));
    }

    #[test]
    fn runtime_fence_requires_explicit_acknowledgement() {
        let rejected = receipt(ReceiptState::Terminal(TerminalOutcome::Rejected(
            Rejected {
                phase: FailurePhase::Admission,
                code: "runtime_fenced".into(),
                message: "a partial must be acknowledged".into(),
                rollback: RollbackState::NotNeeded,
                preserved_revision: Some(RevisionId::new(6).unwrap()),
            },
        )));
        let error = require_applied(rejected).unwrap_err().to_string();

        assert!(error.contains("runtime fence"));
        assert!(error.contains("continue_best_effort"));
        assert!(error.contains("before retrying"));
    }

    #[test]
    fn cancellation_and_replacement_project_as_superseded() {
        for (reason, expected) in [
            (SupersessionReason::Cancelled, "reason=cancelled"),
            (SupersessionReason::Replaced, "reason=replaced"),
        ] {
            let superseded = receipt(ReceiptState::Terminal(TerminalOutcome::Superseded(
                Superseded {
                    reason,
                    by_revision: Some(RevisionId::new(8).unwrap()),
                },
            )));
            let projection = receipt_projection(&superseded).join("\n");

            assert!(projection.starts_with("SUPERSEDED"));
            assert!(projection.contains(expected));
            assert!(!projection.contains("APPLIED"));
            assert!(require_applied(superseded).is_err());
        }
    }

    #[test]
    fn rejected_attempt_retry_diagnostic_names_new_attempt() {
        let rejected = receipt(ReceiptState::Terminal(TerminalOutcome::Rejected(
            Rejected {
                phase: FailurePhase::Validate,
                code: "invalid_candidate".into(),
                message: "voice reference is missing".into(),
                rollback: RollbackState::NotNeeded,
                preserved_revision: Some(RevisionId::new(6).unwrap()),
            },
        )));
        let error = require_applied(rejected).unwrap_err().to_string();

        assert!(error.contains("correct the cause"));
        assert!(error.contains("new attempt"));
    }
}
