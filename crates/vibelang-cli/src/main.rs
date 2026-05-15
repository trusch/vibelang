//! VibeLang CLI v2 - using the new core2 async runtime
//!
//! This is the main entry point for the vibelang command-line tool with full TUI support.

#[cfg(feature = "midi")]
mod midi_dispatcher;
mod render;
mod tui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use vibelang_core::{
    setup_metering, setup_node_tracking, Message, ReloadMessage, Runtime, ScsynthBackend,
    ScsynthConfig, ScsynthProcess, TransportMessage, VoiceMessage,
};
use vibelang_rhai::ScriptEngine;

/// VibeLang CLI - A music livecoding language (core2 runtime)
#[derive(Parser)]
#[command(name = "vibe2", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Script file to run (shorthand for `vibe2 run <file>`)
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

        /// Use TUI mode (terminal user interface)
        #[arg(long)]
        tui: bool,

        /// Disable HTTP API server (API is enabled by default)
        #[arg(long)]
        #[cfg_attr(not(feature = "api"), arg(hide = true))]
        no_api: bool,

        /// HTTP API server port (default: 1606)
        #[arg(long, default_value = "1606")]
        #[cfg_attr(not(feature = "api"), arg(hide = true))]
        api_port: u16,

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
        #[arg(long, default_value = "2")]
        input_channels: u32,

        /// Number of output channels (default: 2)
        #[arg(long, default_value = "2")]
        output_channels: u32,

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

    // Handle shorthand: `vibe2 file.vibe` -> `vibe2 run file.vibe`
    let command = if let Some(file) = cli.file {
        Commands::Run {
            file,
            no_watch: false,
            tui: false,
            no_api: false,
            no_jack_connect: false,
            jack_connect_to: None,
            jack_connect_from: None,
            api_port: 1606,
            include_paths: Vec::new(),
            scsynth_addr: "127.0.0.1:57110".to_string(),
            no_boot: false,
            device: None,
            sample_rate: 0,
            input_channels: 2,
            output_channels: 2,
            no_extensions: false,
            no_fs: false,
            no_exec: false,
            no_net: false,
            fs_sandbox: None,
        }
    } else {
        cli.command.unwrap_or_else(|| {
            eprintln!("Usage: vibe2 <file.vibe> or vibe2 <command>");
            eprintln!("Try 'vibe2 --help' for more information.");
            std::process::exit(1);
        })
    };

    match command {
        Commands::Run {
            file,
            no_watch,
            tui,
            no_api,
            api_port,
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
            if tui {
                run_tui_mode(
                    file,
                    watch,
                    api,
                    api_port,
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
                    ext_config,
                )
                .await
            } else {
                run_simple_mode(
                    file,
                    watch,
                    api,
                    api_port,
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
                    ext_config,
                )
                .await
            }
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
    include_paths: Vec<PathBuf>,
    scsynth_addr: String,
    no_boot: bool,
    no_jack_connect: bool,
    jack_connect_to: Option<String>,
    jack_connect_from: Option<String>,
    device: Option<String>,
    sample_rate: u32,
    input_channels: u32,
    output_channels: u32,
    ext_config: ExtensionSettings,
) -> Result<()> {
    // Initialize logging - uses RUST_LOG env var
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

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
        let mut config = ScsynthConfig::default()
            .auto_connect_jack(!no_jack_connect)
            .sample_rate(sample_rate)
            .input_channels(input_channels)
            .output_channels(output_channels)
            .verbose(true); // Enable verbose mode for debugging
        if let Some(dev) = &device {
            config = config.device(dev);
        }
        // Parse manual JACK output connection targets if specified
        if let Some(ref targets) = jack_connect_to {
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
        if let Some(ref sources) = jack_connect_from {
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
        if no_jack_connect && jack_connect_to.is_none() && jack_connect_from.is_none() {
            // Disconnect all ports that scsynth's JACK driver may have auto-connected
            process.disconnect_all_jack_ports();
        } else {
            // Auto-connect or use manual targets
            process.auto_connect_jack_ports();
        }

        Some(process)
    };

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
            vibelang_http::start_server(api_handle, api_state, api_port, Some(eval_tx)).await;
        });
        info!("HTTP API server started on port {}", api_port);

        // Spawn eval handler task
        let eval_include_paths = include_paths.clone();
        let eval_ext_config = ext_config.clone();
        let eval_handle = handle.clone();
        tokio::spawn(async move {
            while let Ok(job) = eval_rx.recv() {
                let result = match evaluate_code(&job.code, &eval_include_paths, &eval_ext_config) {
                    Ok(state) => {
                        // Apply the state
                        match eval_handle
                            .send(ReloadMessage::Apply { state }.into())
                            .await
                        {
                            Ok(_) => vibelang_http::EvalResult {
                                success: true,
                                result: Some("Code executed successfully".to_string()),
                                error: None,
                            },
                            Err(e) => vibelang_http::EvalResult {
                                success: false,
                                result: None,
                                error: Some(format!("Failed to apply: {}", e)),
                            },
                        }
                    }
                    Err(e) => vibelang_http::EvalResult {
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                    },
                };
                let _ = job.response_tx.send(result);
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
    handle.sync_and_wait().await?;
    info!("Synthdefs loaded, applying state...");

    // Check if script requested early exit (for integration tests)
    if let Some(exit_code) = vibelang_rhai::get_exit_code() {
        info!("Script requested exit with code {}", exit_code);
        // Apply state before exiting (for any cleanup)
        let _ = handle
            .send(
                ReloadMessage::Apply {
                    state: output.state,
                }
                .into(),
            )
            .await;
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

    handle
        .send(
            ReloadMessage::Apply {
                state: output.state,
            }
            .into(),
        )
        .await?;

    // Wait for state to be fully applied before starting transport
    // This ensures MIDI devices are registered before transport starts,
    // so they receive the Start message
    handle.sync_and_wait().await?;
    info!("State applied");

    // Start transport
    handle
        .send(Message::Transport(TransportMessage::Start))
        .await?;
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
                            let _ = tx.blocking_send(path);
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
                            if let Err(e) = handle_clone
                                .send(
                                    ReloadMessage::Apply {
                                        state: output.state,
                                    }
                                    .into(),
                                )
                                .await
                            {
                                error!("Failed to apply reload: {}", e);
                            } else {
                                info!("Reload successful");
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
    handle
        .send(Message::Transport(TransportMessage::Stop))
        .await?;
    info!("Transport stopped");

    // Wait for runtime task
    runtime_task.abort();

    info!("Goodbye!");
    Ok(())
}

/// Run in TUI mode
#[allow(clippy::too_many_arguments)]
async fn run_tui_mode(
    file: PathBuf,
    watch: bool,
    #[allow(unused_variables)] api: bool,
    #[allow(unused_variables)] api_port: u16,
    include_paths: Vec<PathBuf>,
    scsynth_addr: String,
    no_boot: bool,
    no_jack_connect: bool,
    jack_connect_to: Option<String>,
    jack_connect_from: Option<String>,
    device: Option<String>,
    sample_rate: u32,
    input_channels: u32,
    output_channels: u32,
    ext_config: ExtensionSettings,
) -> Result<()> {
    // Initialize TUI event channel
    let mut tui_events = tui::init_tui_channel();

    // Initialize TUI logger (routes log messages to TUI)
    let _ = tui::init_tui_logger();

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create TUI app
    let mut app = tui::TuiApp::new();
    app.set_script_path(file.display().to_string());

    log::info!("VibeLang v2 - TUI Mode");
    log::info!("Script: {}", file.display());

    // Start or connect to scsynth
    let _process = if no_boot {
        log::info!("Connecting to scsynth at {}", scsynth_addr);
        None
    } else {
        log::info!("Starting scsynth...");
        let mut config = ScsynthConfig::default()
            .auto_connect_jack(!no_jack_connect)
            .sample_rate(sample_rate)
            .input_channels(input_channels)
            .output_channels(output_channels);
        if let Some(dev) = &device {
            config = config.device(dev);
        }
        // Parse manual JACK output connection targets if specified
        if let Some(ref targets) = jack_connect_to {
            let ports: Vec<String> = targets.split(',').map(|s| s.trim().to_string()).collect();
            if ports.is_empty() || ports.iter().any(|p| p.is_empty()) {
                // Clean up terminal before returning error
                disable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
                terminal.show_cursor()?;
                return Err(anyhow::anyhow!(
                    "Invalid --jack-connect-to format. Expected comma-separated port names, got: {}",
                    targets
                ));
            }
            config = config.jack_connect_outputs(ports);
        }
        // Parse manual JACK input connection sources if specified
        if let Some(ref sources) = jack_connect_from {
            let ports: Vec<String> = sources.split(',').map(|s| s.trim().to_string()).collect();
            if ports.is_empty() || ports.iter().any(|p| p.is_empty()) {
                // Clean up terminal before returning error
                disable_raw_mode()?;
                execute!(
                    terminal.backend_mut(),
                    LeaveAlternateScreen,
                    DisableMouseCapture
                )?;
                terminal.show_cursor()?;
                return Err(anyhow::anyhow!(
                    "Invalid --jack-connect-from format. Expected comma-separated port names, got: {}",
                    sources
                ));
            }
            config = config.jack_connect_inputs(ports);
        }
        match ScsynthProcess::spawn(config) {
            Ok(process) => {
                process.wait_startup(Duration::from_secs(3)).await;
                log::info!("scsynth started");

                // Handle JACK port connections
                if no_jack_connect && jack_connect_to.is_none() && jack_connect_from.is_none() {
                    // Disconnect all ports that scsynth's JACK driver may have auto-connected
                    process.disconnect_all_jack_ports();
                } else {
                    // Auto-connect or use manual targets
                    process.auto_connect_jack_ports();
                }

                Some(process)
            }
            Err(e) => {
                log::error!("Failed to start scsynth: {}", e);
                app.process_event(tui::TuiEvent::Error(format!(
                    "Failed to start scsynth: {}",
                    e
                )));
                None
            }
        }
    };

    // Connect to scsynth
    let osc_backend = match ScsynthBackend::connect(&scsynth_addr).await {
        Ok(b) => b,
        Err(e) => {
            // Clean up terminal before returning error
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            return Err(anyhow::anyhow!("Failed to connect to scsynth: {}", e));
        }
    };

    // Create runtime — thread the actual scsynth -i/-o channel counts in so
    // the audio bus allocator starts past the hardware I/O block instead of
    // colliding with hardware input buses on setups with >16 hardware buses.
    let mut runtime = Runtime::new_with_audio_config(osc_backend, output_channels, input_channels);
    let handle = runtime.handle();

    // Set up metering callback to receive SendTrig messages from link synths
    setup_metering(runtime.backend(), runtime.state().clone());

    // Set up node tracking callback to remove ended nodes from voice state
    // This prevents "node not found" errors when synths free themselves via doneAction=2
    setup_node_tracking(runtime.backend(), runtime.state().clone());

    // Load built-in synthdefs
    log::info!("Loading built-in synthdefs...");
    if let Err(e) = runtime.load_builtins().await {
        log::error!("Failed to load built-in synthdefs: {}", e);
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;
        return Err(anyhow::anyhow!("Failed to load built-in synthdefs: {}", e));
    }

    // Set up deploy callback for custom synthdefs
    let deploy_handle = handle.clone();
    vibelang_dsp::set_deploy_callback(move |bytes| {
        let name = extract_synthdef_name(&bytes).unwrap_or_else(|| "unknown".to_string());
        log::info!(
            "Deploy callback: queuing synthdef '{}' ({} bytes)",
            name,
            bytes.len()
        );
        deploy_handle
            .try_send(vibelang_core::Message::SynthDef(
                vibelang_core::SynthDefMessage::Load { name, data: bytes },
            ))
            .map_err(|e| {
                log::error!("Deploy callback: failed to queue synthdef: {}", e);
                e.to_string()
            })
    });

    // Get state handle before spawning runtime (needed for HTTP API)
    let state_handle = runtime.state().clone();

    // Start HTTP API server if requested
    #[cfg(feature = "api")]
    if api {
        let api_handle = handle.clone();
        let api_state = state_handle.clone();
        tokio::spawn(async move {
            vibelang_http::start_server(api_handle, api_state, api_port, None).await;
        });
        log::info!("HTTP API server started on port {}", api_port);
    }

    // Create shutdown flag
    let running = Arc::new(AtomicBool::new(true));

    // Spawn the MIDI dispatcher task (must happen before `runtime` is moved
    // into the runtime task below).
    #[cfg(feature = "midi")]
    let midi_dispatch_tx = {
        let dispatcher_engine = Arc::new(make_dispatcher_engine());
        midi_dispatcher::spawn(
            dispatcher_engine,
            runtime.midi_callback_receiver(),
            running.clone(),
        )
    };

    // Create channels
    let (state_tx, mut state_rx) = mpsc::channel::<vibelang_core::State>(32);
    let (reload_tx, mut reload_rx) = mpsc::channel::<PathBuf>(16);

    // Set up file watcher if watch mode enabled
    let _watcher: Option<RecommendedWatcher> = if watch {
        let file_clone = file.clone();
        let reload_tx_clone = reload_tx.clone();
        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if event.kind.is_modify() || event.kind.is_create() {
                        let _ = reload_tx_clone.blocking_send(file_clone.clone());
                    }
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        // Watch the script file and its directory
        let mut w = watcher;
        if let Some(parent) = file.parent() {
            let _ = w.watch(parent, RecursiveMode::NonRecursive);
        }
        let _ = w.watch(&file, RecursiveMode::NonRecursive);
        log::info!("Watching for changes...");
        Some(w)
    } else {
        None
    };

    // Run initial script
    match execute_script(&file, &include_paths, &ext_config) {
        Ok(output) => {
            #[cfg(feature = "midi")]
            {
                let _ = midi_dispatch_tx
                    .send(midi_dispatcher::DispatchState {
                        ast: output.ast,
                        callbacks: output.midi_callbacks,
                    })
                    .await;
            }
            if let Err(e) = handle
                .send(
                    ReloadMessage::Apply {
                        state: output.state,
                    }
                    .into(),
                )
                .await
            {
                log::error!("Failed to apply script: {}", e);
                app.process_event(tui::TuiEvent::Error(format!("{}", e)));
            } else {
                // Wait for state to be fully applied before starting transport
                // This ensures MIDI devices are registered before transport starts
                let _ = handle.sync_and_wait().await;
                log::info!("Script loaded successfully");
            }
        }
        Err(e) => {
            log::error!("Script error: {}", e);
            app.process_event(tui::TuiEvent::Error(format!("{}", e)));
        }
    }

    // Start transport
    let _ = handle
        .send(Message::Transport(TransportMessage::Start))
        .await;

    // Start OS keyboard listener for reliable key release
    let os_keyboard_rx = tui::os_keyboard::start_os_keyboard_listener();
    app.set_os_keyboard_active(os_keyboard_rx.is_some());

    // Spawn runtime task
    let runtime_running = running.clone();
    let runtime_task = tokio::spawn(async move {
        let mut state_interval = tokio::time::interval(Duration::from_millis(50));
        while runtime_running.load(Ordering::SeqCst) {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(1)) => {
                    runtime.tick().await;
                }
                _ = state_interval.tick() => {
                    // Send state update to TUI
                    let state = runtime.state().read().await.clone();
                    let _ = state_tx.send(state).await;
                }
            }
        }
    });

    // Handle reload requests in a separate task
    let reload_handle = handle.clone();
    let include_paths_clone = include_paths.clone();
    let ext_config_clone = ext_config.clone();
    let reload_running = running.clone();
    #[cfg(feature = "midi")]
    let reload_dispatch_tx = midi_dispatch_tx.clone();
    let reload_task = tokio::spawn(async move {
        let mut last_reload = Instant::now();
        while reload_running.load(Ordering::SeqCst) {
            if let Some(changed_file) = reload_rx.recv().await {
                // Debounce
                if last_reload.elapsed() < Duration::from_millis(100) {
                    continue;
                }
                last_reload = Instant::now();

                log::info!("Reloading: {}", changed_file.display());
                match execute_script(&changed_file, &include_paths_clone, &ext_config_clone) {
                    Ok(output) => {
                        // NOTE: We skip sync_and_wait here for hot reloads to avoid blocking
                        // the tick loop and causing audio gaps. Synthdefs are loaded async
                        // and will be available for subsequent reloads. Initial load still
                        // syncs to ensure synthdefs are ready before first playback.
                        #[cfg(feature = "midi")]
                        {
                            let _ = reload_dispatch_tx
                                .send(midi_dispatcher::DispatchState {
                                    ast: output.ast,
                                    callbacks: output.midi_callbacks,
                                })
                                .await;
                        }
                        if let Err(e) = reload_handle
                            .send(
                                ReloadMessage::Apply {
                                    state: output.state,
                                }
                                .into(),
                            )
                            .await
                        {
                            log::error!("Failed to apply reload: {}", e);
                        } else {
                            log::info!("Reload successful");
                        }
                    }
                    Err(e) => {
                        log::error!("Script error: {}", e);
                    }
                }
            }
        }
    });

    // Main TUI loop
    let mut last_draw = Instant::now();
    let draw_interval = Duration::from_millis(33); // ~30 FPS

    loop {
        // Check shutdown flag
        if !running.load(Ordering::SeqCst) {
            break;
        }

        // Handle TUI events from log system
        while let Ok(event) = tui_events.try_recv() {
            app.process_event(event);
        }

        // Handle state updates from runtime
        while let Ok(state) = state_rx.try_recv() {
            app.update_state(state);
        }

        // Handle OS keyboard events
        if let Some(rx) = &os_keyboard_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    tui::os_keyboard::OsKeyEvent::KeyDown(c) => {
                        if app.keyboard_active() {
                            if let Some((note, velocity)) =
                                app.virtual_keyboard.key_down(KeyCode::Char(c))
                            {
                                if let Some(voice_id) = app.get_keyboard_target_voice() {
                                    let _ = handle
                                        .send(Message::Voice(VoiceMessage::NoteOn {
                                            voice: voice_id,
                                            note,
                                            velocity: velocity as f32 / 127.0,
                                        }))
                                        .await;
                                    log::debug!(
                                        "Note on: {} vel:{} -> {}",
                                        note,
                                        velocity,
                                        voice_id
                                    );
                                } else {
                                    log::debug!("Note on: {} (no target voice)", note);
                                }
                            }
                        }
                    }
                    tui::os_keyboard::OsKeyEvent::KeyUp(c) => {
                        if app.keyboard_active() {
                            if let Some(note) = app.virtual_keyboard.key_up(KeyCode::Char(c)) {
                                if let Some(voice_id) = app.get_keyboard_target_voice() {
                                    let _ = handle
                                        .send(Message::Voice(VoiceMessage::NoteOff {
                                            voice: voice_id,
                                            note,
                                        }))
                                        .await;
                                    log::debug!("Note off: {} -> {}", note, voice_id);
                                } else {
                                    log::debug!("Note off: {} (no target voice)", note);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Handle pending seek
        if let Some(seek_offset) = app.check_seek_debounce() {
            // Calculate target beat from current position + offset
            let current_beat = app
                .runtime_state
                .as_ref()
                .map(|s| s.current_beat.to_f64())
                .unwrap_or(0.0);
            let target_beat = (current_beat + seek_offset).max(0.0);
            let _ = handle
                .send(Message::Transport(TransportMessage::Seek {
                    beat: vibelang_core::Beat::from_f64(target_beat),
                }))
                .await;
        }

        // Update flash tracking
        app.update_flash_tracking();

        // Draw TUI
        if last_draw.elapsed() >= draw_interval {
            terminal.draw(|f| tui::ui::render_ui(f, &mut app))?;
            last_draw = Instant::now();
        }

        // Handle terminal events
        if event::poll(Duration::from_millis(10))? {
            app.mark_terminal_event();

            match event::read()? {
                Event::Key(key) => {
                    // Handle modals first
                    if app.show_help_modal {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter => {
                                app.toggle_help_modal();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    if app.show_error_modal {
                        match key.code {
                            KeyCode::Esc | KeyCode::Enter => {
                                app.close_error_modal();
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle search mode
                    if app.in_input_mode() {
                        match key.code {
                            KeyCode::Esc => {
                                app.exit_search_mode();
                            }
                            KeyCode::Enter => {
                                app.exit_search_mode();
                            }
                            KeyCode::Backspace => {
                                app.search_pop_char();
                            }
                            KeyCode::Char(c) => {
                                app.search_push_char(c);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Handle keyboard mode
                    if app.keyboard_active() && !key.modifiers.contains(KeyModifiers::CONTROL) {
                        match key.code {
                            KeyCode::Char('k') | KeyCode::Esc => {
                                let released = app.virtual_keyboard.hide();
                                if let Some(voice_id) = app.get_keyboard_target_voice() {
                                    for note in released {
                                        let _ = handle
                                            .send(Message::Voice(VoiceMessage::NoteOff {
                                                voice: voice_id,
                                                note,
                                            }))
                                            .await;
                                        log::debug!("Note off: {} -> {}", note, voice_id);
                                    }
                                }
                            }
                            KeyCode::Char('<') => {
                                let released = app.virtual_keyboard.octave_down();
                                if let Some(voice_id) = app.get_keyboard_target_voice() {
                                    for note in released {
                                        let _ = handle
                                            .send(Message::Voice(VoiceMessage::NoteOff {
                                                voice: voice_id,
                                                note,
                                            }))
                                            .await;
                                        log::debug!("Note off: {} -> {}", note, voice_id);
                                    }
                                }
                            }
                            KeyCode::Char('>') => {
                                let released = app.virtual_keyboard.octave_up();
                                if let Some(voice_id) = app.get_keyboard_target_voice() {
                                    for note in released {
                                        let _ = handle
                                            .send(Message::Voice(VoiceMessage::NoteOff {
                                                voice: voice_id,
                                                note,
                                            }))
                                            .await;
                                        log::debug!("Note off: {} -> {}", note, voice_id);
                                    }
                                }
                            }
                            KeyCode::Char(' ') => {
                                // Toggle playback based on current state
                                let is_playing = app
                                    .runtime_state
                                    .as_ref()
                                    .map(|s| s.playing)
                                    .unwrap_or(false);
                                let msg = if is_playing {
                                    TransportMessage::Stop
                                } else {
                                    TransportMessage::Start
                                };
                                let _ = handle.send(Message::Transport(msg)).await;
                            }
                            KeyCode::Char(c) => {
                                if let Some((note, velocity)) =
                                    app.virtual_keyboard.key_down(KeyCode::Char(c))
                                {
                                    if let Some(voice_id) = app.get_keyboard_target_voice() {
                                        let _ = handle
                                            .send(Message::Voice(VoiceMessage::NoteOn {
                                                voice: voice_id,
                                                note,
                                                velocity: velocity as f32 / 127.0,
                                            }))
                                            .await;
                                        log::debug!(
                                            "Note on: {} vel:{} -> {}",
                                            note,
                                            velocity,
                                            voice_id
                                        );
                                    } else {
                                        log::debug!("Note on: {} (no target voice)", note);
                                    }
                                }
                            }
                            _ => {}
                        }
                        continue;
                    }

                    // Normal key handling
                    match key.code {
                        KeyCode::Char('q') => {
                            running.store(false, Ordering::SeqCst);
                            break;
                        }
                        KeyCode::Char('?') => {
                            app.toggle_help_modal();
                        }
                        KeyCode::Char(' ') => {
                            // Toggle playback based on current state
                            let is_playing = app
                                .runtime_state
                                .as_ref()
                                .map(|s| s.playing)
                                .unwrap_or(false);
                            let msg = if is_playing {
                                TransportMessage::Stop
                            } else {
                                TransportMessage::Start
                            };
                            let _ = handle.send(Message::Transport(msg)).await;
                        }
                        KeyCode::Left => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                let beat_info = app.get_beat_info();
                                app.add_pending_seek(-beat_info.total_beats_in_bar as f64);
                            } else {
                                app.add_pending_seek(-1.0);
                            }
                        }
                        KeyCode::Right => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                let beat_info = app.get_beat_info();
                                app.add_pending_seek(beat_info.total_beats_in_bar as f64);
                            } else {
                                app.add_pending_seek(1.0);
                            }
                        }
                        KeyCode::Home | KeyCode::Char('0') => {
                            let _ = handle
                                .send(Message::Transport(TransportMessage::Seek {
                                    beat: vibelang_core::Beat::ZERO,
                                }))
                                .await;
                            app.cancel_pending_seek();
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.move_selection_up();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.move_selection_down();
                        }
                        KeyCode::PageUp => {
                            app.move_selection_page_up();
                        }
                        KeyCode::PageDown => {
                            app.move_selection_page_down();
                        }
                        KeyCode::Tab | KeyCode::Char('l')
                            if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            app.toggle_focus();
                        }
                        KeyCode::Enter => {
                            app.toggle_collapse();
                        }
                        KeyCode::Char('[') => {
                            app.collapse_all();
                        }
                        KeyCode::Char(']') => {
                            app.expand_all();
                        }
                        KeyCode::Char('/') => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                app.enter_log_search_mode();
                            } else {
                                app.enter_search_mode();
                            }
                        }
                        KeyCode::Char('i') => {
                            app.toggle_hide_inactive();
                        }
                        KeyCode::Char('L') => {
                            app.toggle_log_maximized();
                        }
                        KeyCode::Char('K') => {
                            app.virtual_keyboard.show();
                        }
                        KeyCode::Char('V') => {
                            // Set selected voice as keyboard target
                            app.set_keyboard_target_from_selection();
                            let name = app.keyboard_target_voice_name();
                            log::info!("Keyboard target: {}", name);
                        }
                        KeyCode::Char(c) if ('1'..='5').contains(&c) => {
                            app.set_log_level(c.to_digit(10).unwrap() as u8);
                        }
                        KeyCode::Esc => {
                            if app.error_message.is_some() {
                                app.close_error_modal();
                            }
                        }
                        _ => {}
                    }
                }
                Event::FocusGained => {
                    app.set_focus_events_supported(true);
                    app.set_has_focus(true);
                }
                Event::FocusLost => {
                    app.set_focus_events_supported(true);
                    app.set_has_focus(false);
                }
                _ => {}
            }
        }

        // Auto-release expired notes from virtual keyboard
        let expired = app.virtual_keyboard.get_expired_notes();
        if !expired.is_empty() {
            if let Some(voice_id) = app.get_keyboard_target_voice() {
                for note in expired {
                    let _ = handle
                        .send(Message::Voice(VoiceMessage::NoteOff {
                            voice: voice_id,
                            note,
                        }))
                        .await;
                    log::debug!("Note off (expired): {} -> {}", note, voice_id);
                }
            }
        }
    }

    // Cleanup
    running.store(false, Ordering::SeqCst);

    // Stop transport
    let _ = handle
        .send(Message::Transport(TransportMessage::Stop))
        .await;

    // Abort tasks
    runtime_task.abort();
    reload_task.abort();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    log::info!("Goodbye!");
    Ok(())
}

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
