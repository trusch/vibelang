//! VibeLang CLI - The `vibelang` command.
//!
//! This is the main entry point for executing `.vibe` files.
//!
//! # Architecture
//!
//! The CLI binary orchestrates the following modular crates:
//!
//! - **vibelang-core**: State management, scheduling, OSC communication, Rhai API
//! - **vibelang-dsp**: SynthDef generation and UGen graph building
//! - **vibelang-std**: Standard library of `.vibe` files
//!
//! # Commands
//!
//! - `vibe run <file>` - Run a .vibe file interactively (default)
//! - `vibe render <file>` - Render a .vibe file to audio

mod render;
mod tui;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use rhai::AST;
use vibelang_core::api::context;
use vibelang_core::state::StateMessage;
use vibelang_core::{AudioConfig, RuntimeHandle};

/// VibeLang - SuperCollider Live Coding
#[derive(Parser, Debug)]
#[command(name = "vibe")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "A musical programming language for SuperCollider", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the .vibe file (shortcut for `vibe run <file>`)
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Disable watch mode (watching is enabled by default)
    #[arg(long, global = true)]
    no_watch: bool,

    /// Enable TUI mode (Terminal User Interface)
    #[arg(long, global = true)]
    tui: bool,

    /// Additional import directories
    #[arg(short = 'I', long = "import-path", value_name = "PATH", global = true)]
    import_paths: Vec<PathBuf>,

    /// Enable HTTP REST API server (can run without a file)
    #[arg(long, global = true)]
    api: bool,

    /// HTTP API server port (default: 1606)
    #[arg(long, value_name = "PORT", default_value = "1606", global = true)]
    api_port: u16,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a .vibe file interactively (default behavior)
    Run(RunArgs),

    /// Render a .vibe file to an audio file (offline)
    Render(RenderArgs),

    /// Start the Language Server Protocol (LSP) server
    Lsp,

    /// List available audio devices
    Devices,
}

#[derive(Args, Debug)]
struct RunArgs {
    /// Path to the .vibe file to execute (optional when using --api)
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,

    /// Disable watch mode (watching is enabled by default)
    #[arg(long)]
    no_watch: bool,

    /// Enable TUI mode (Terminal User Interface)
    #[arg(long)]
    tui: bool,

    /// Additional import directories
    #[arg(short = 'I', long = "import-path", value_name = "PATH")]
    import_paths: Vec<PathBuf>,

    /// Record events to an audio file (wav, mp3, flac, ogg) or score file (.vibescore)
    /// Use --record alone to create out.wav, or --record <path> for a specific output
    #[arg(long, value_name = "PATH", default_missing_value = "out.wav", num_args = 0..=1)]
    record: Option<PathBuf>,

    /// Exit automatically when the specified sequence completes (useful with --record)
    #[arg(long, value_name = "NAME")]
    exit_after_sequence: Option<String>,

    /// Enable HTTP REST API server
    #[arg(long)]
    api: bool,

    /// HTTP API server port (default: 1606)
    #[arg(long, value_name = "PORT", default_value = "1606")]
    api_port: u16,

    /// Audio input device name
    #[arg(long, value_name = "DEVICE")]
    input_device: Option<String>,

    /// Audio output device name
    #[arg(long, value_name = "DEVICE")]
    output_device: Option<String>,

    /// Number of input channels (default: 2)
    #[arg(long, default_value = "2")]
    input_channels: u32,

    /// Number of output channels (default: 2)
    #[arg(long, default_value = "2")]
    output_channels: u32,

    /// Sample rate in Hz (e.g., 44100, 48000, 96000)
    #[arg(long, value_name = "RATE")]
    sample_rate: Option<u32>,
}

#[derive(Args, Debug, Clone)]
pub struct RenderArgs {
    /// Path to the score file (.osc) to render
    #[arg(value_name = "SCORE_FILE")]
    pub score_file: PathBuf,

    /// Output audio file path
    #[arg(value_name = "OUTPUT")]
    pub output: PathBuf,

    /// Output format (wav, mp3, flac, ogg) - inferred from extension if not specified
    #[arg(long)]
    pub format: Option<String>,

    /// Sample rate
    #[arg(long, default_value = "48000")]
    pub sample_rate: u32,

    /// Bit depth for WAV (16, 24, 32)
    #[arg(long, default_value = "24")]
    pub bit_depth: u8,

    /// Add tail time at the end (seconds)
    #[arg(long, default_value = "2.0")]
    pub tail: f64,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Run(args)) => {
            let watch = !args.no_watch;
            // Validate: file is required unless --api is specified
            if args.file.is_none() && !args.api {
                anyhow::bail!(
                    "Missing required argument: FILE\n\n\
                    Usage: vibe run <FILE> [OPTIONS]\n\
                           vibe run --api           (API-only mode, no file needed)\n\n\
                    For more information, try '--help'"
                );
            }
            // Build audio configuration from CLI args
            let audio_config = AudioConfig::new()
                .with_input_device(args.input_device)
                .with_output_device(args.output_device)
                .with_input_channels(args.input_channels)
                .with_output_channels(args.output_channels)
                .with_sample_rate(args.sample_rate);
            run_vibe_file(args.file, watch, args.tui, args.import_paths, args.record, args.exit_after_sequence, args.api, args.api_port, audio_config)
        }
        Some(Commands::Render(args)) => {
            render::render(args)
        }
        Some(Commands::Lsp) => {
            // Run the LSP server
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(vibelang_lsp::run_lsp_server())
        }
        Some(Commands::Devices) => {
            // List available audio devices
            vibelang_core::print_audio_devices()
        }
        None => {
            // No subcommand - check if a file was provided directly or if --api is enabled
            if cli.file.is_some() || cli.api {
                let watch = !cli.no_watch;
                run_vibe_file(cli.file, watch, cli.tui, cli.import_paths, None, None, cli.api, cli.api_port, AudioConfig::default())
            } else {
                anyhow::bail!(
                    "Missing required argument: FILE\n\n\
                    Usage: vibe <FILE> [OPTIONS]\n\
                           vibe run <FILE> [OPTIONS]\n\
                           vibe --api               (API-only mode, no file needed)\n\
                           vibe devices             (list available audio devices)\n\
                           vibe render <SCORE_FILE> [OPTIONS]\n\n\
                    For more information, try '--help'"
                )
            }
        }
    }
}

fn run_vibe_file(
    file: Option<PathBuf>,
    watch: bool,
    tui_mode: bool,
    import_paths: Vec<PathBuf>,
    record: Option<PathBuf>,
    exit_after_sequence: Option<String>,
    api_enabled: bool,
    api_port: u16,
    audio_config: AudioConfig,
) -> Result<()> {
    use vibelang_core::JackMidiOutput;

    // Initialize logger based on TUI mode
    if tui_mode {
        tui::init_tui_logger();
    } else {
        tui::init_logger();
    }

    // Create JACK MIDI output for virtual keyboard EARLY (before script runs)
    // This ensures the MIDI port exists when script calls midi_open("vibelang-keyboard")
    let jack_keyboard = if tui_mode {
        match JackMidiOutput::new("vibelang-keyboard", "midi_out") {
            Ok(output) => {
                log::info!("Virtual keyboard JACK port created: {}", output.port_name);
                Some(output)
            }
            Err(e) => {
                log::warn!("Could not create JACK MIDI output for virtual keyboard: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Validate the file exists (if provided)
    if let Some(ref f) = file {
        if !f.exists() {
            anyhow::bail!("File not found: {}", f.display());
        }

        // Check file extension
        if f.extension().and_then(|s| s.to_str()) != Some("vibe") {
            log::warn!("File doesn't have .vibe extension");
        }

        if !tui_mode {
            println!("🎵 VibeLang - SuperCollider Live Coding");
            println!("=======================================\n");
            println!("📄 Loading: {}\n", f.display());
        } else {
            log::info!("🎵 VibeLang - SuperCollider Live Coding");
            log::info!("📄 Loading: {}", f.display());
        }
    } else {
        // API-only mode
        if !tui_mode {
            println!("🎵 VibeLang - SuperCollider Live Coding");
            println!("=======================================\n");
            println!("🌐 API-only mode (no script file)\n");
        } else {
            log::info!("🎵 VibeLang - SuperCollider Live Coding");
            log::info!("🌐 API-only mode (no script file)");
        }
    }

    // 1-3. Start runtime (includes scsynth process, connection, and runtime thread)
    log::info!("Starting runtime...");
    let runtime = vibelang_core::Runtime::start_with_audio_config(audio_config)
        .context("Failed to start runtime")?;
    let handle = runtime.handle();

    // Initialize the API with the runtime handle
    vibelang_core::init_api(handle.clone());

    // Set up the synthdef deploy callback
    // This callback both sends to scsynth AND stores in state for score capture
    let deploy_handle = handle.clone();
    vibelang_dsp::set_deploy_callback(move |bytes| {
        // Extract synthdef name from bytes (SuperCollider synthdef format)
        let name = extract_synthdef_name(&bytes).unwrap_or_else(|| "unknown".to_string());

        // Store in state for score capture
        let _ = deploy_handle.send(StateMessage::LoadSynthDef {
            name: name.clone(),
            bytes: bytes.clone(),
        });

        // Send to scsynth
        deploy_handle.scsynth().d_recv_bytes(bytes)
            .map_err(|e| e.to_string())
    });
    log::info!("   ✓ Runtime started");

    // 4. Create main group
    log::info!("4. Creating main group...");
    vibelang_core::api::group::create_main_group();
    log::info!("   ✓ Main group created");

    // 6. Create Rhai engine
    log::info!("6. Initializing Rhai engine...");
    let base_path = file
        .as_ref()
        .and_then(|f| f.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Create engine with import paths
    let mut all_import_paths = import_paths.clone();
    // Add stdlib path
    let stdlib_path = PathBuf::from(vibelang_std::stdlib_path());
    all_import_paths.push(stdlib_path.clone());
    all_import_paths.push(stdlib_path.parent().unwrap().to_path_buf());

    // Set up the script context for file resolution
    vibelang_core::api::context::set_script_dir(base_path.clone());
    vibelang_core::api::context::set_import_paths(all_import_paths.clone());

    let mut engine = vibelang_core::create_engine_with_paths(base_path, all_import_paths);

    // Register DSP functions (UGens, NodeRef, etc.)
    vibelang_dsp::register_dsp_api(&mut engine);
    log::info!("   ✓ Engine ready");

    // Enable score capture BEFORE script runs if --record flag is set
    // This ensures all events from beat 0 are captured
    // Determine if we need to auto-render after recording
    let (score_capture_path, render_output_path) = if let Some(ref record_path) = record {
        let ext = record_path.extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if ext == "vibescore" {
            // Direct vibescore output - no rendering needed
            log::info!("📼 Recording score to: {}", record_path.display());
            (record_path.clone(), None)
        } else {
            // Audio output - use temp vibescore, then render
            let temp_score = std::env::temp_dir().join(format!(
                "vibelang_record_{}.vibescore",
                std::process::id()
            ));
            log::info!("📼 Recording to: {} (via temp score)", record_path.display());
            (temp_score, Some(record_path.clone()))
        }
    } else {
        (PathBuf::new(), None)
    };

    if record.is_some() {
        handle.send(StateMessage::EnableScoreCapture { path: score_capture_path.clone() })?;
    }

    // Clear any existing callbacks and MIDI devices from previous runs
    vibelang_core::api::clear_callbacks();
    vibelang_core::api::clear_midi_devices();

    // 7. Read and compile the script (if a file was provided)
    let mut current_ast: Option<AST> = if let Some(ref f) = file {
        log::info!("7. Compiling .vibe file...");
        let script = fs::read_to_string(f)
            .with_context(|| format!("Failed to read file: {}", f.display()))?;

        // Compile the script to AST (we need the AST for callback execution)
        match engine.compile(&script) {
            Ok(ast) => {
                log::info!("   ✓ Script compiled successfully");
                Some(ast)
            }
            Err(e) => {
                log::error!("Compile error: {}", e);
                None
            }
        }
    } else {
        log::info!("7. No script file - API-only mode");
        None
    };

    // Execute the compiled AST
    if let Some(ref ast) = current_ast {
        // Set the current script file for source location tracking
        if let Some(ref f) = file {
            let abs_path = f.canonicalize().unwrap_or_else(|_| f.clone());
            context::set_current_script_file(Some(abs_path.to_string_lossy().to_string()));
        }

        match engine.run_ast(ast) {
            Ok(_) => {
                log::info!("   ✓ Script executed successfully");
            }
            Err(e) => {
                log::error!("Script error: {}", e);
                // Continue running to allow sounds to play
            }
        }
    }

    // Start the scheduler AFTER script evaluation
    // This ensures sequences started during initial evaluation anchor at beat 0.0
    // (before this, transport.beat_at() returns 0.0, so quantization gives beat 0.0)
    handle.send(StateMessage::StartScheduler)?;
    log::info!("   ✓ Scheduler started");

    // Finalize groups
    handle.send(StateMessage::FinalizeGroups)?;
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Create eval channel for the HTTP server to send code evaluation requests
    let (eval_tx, eval_rx) = std::sync::mpsc::channel::<vibelang_http::EvalJob>();

    // Start HTTP API server if enabled
    if api_enabled {
        let api_handle = handle.clone();
        let eval_sender = eval_tx.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
            rt.block_on(async {
                vibelang_http::start_server(api_handle, api_port, Some(eval_sender)).await;
            });
        });
        log::info!("   ✓ HTTP API server started on port {}", api_port);
    }

    // Keep the process running
    if tui_mode {
        // TUI mode - run the TUI event loop
        run_tui_loop(file.as_ref(), engine, handle.clone(), watch, &import_paths, current_ast, jack_keyboard)?;
    } else {
        // Set up signal handlers for graceful shutdown (SIGINT and SIGTERM)
        let shutdown = Arc::new(AtomicBool::new(false));

        // Register SIGINT (Ctrl+C) and SIGTERM handlers
        for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            signal_hook::flag::register(sig, Arc::clone(&shutdown))
                .expect("Failed to register signal handler");
        }

        // Log status message
        if let Some(ref seq_name) = exit_after_sequence {
            log::info!("\n8. Waiting for sequence '{}' to complete...", seq_name);
        } else if watch && file.is_some() {
            log::info!("\n8. Watch mode enabled - monitoring file for changes...");
            log::info!("   (Press Ctrl+C to exit)\n");
        } else if api_enabled {
            log::info!("\n8. API server running on http://localhost:{}", api_port);
            log::info!("   (Press Ctrl+C to exit)\n");
        } else {
            log::info!("\n8. Script running... (Press Ctrl+C to exit)");
        }

        // Simple watch loop - poll file modification time (only if file provided)
        let mut last_modified = file.as_ref()
            .and_then(|f| fs::metadata(f).ok())
            .and_then(|m| m.modified().ok());

        // Create a scope for callback execution
        let mut callback_scope = rhai::Scope::new();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));

            // Check for shutdown signal (SIGINT/Ctrl+C or SIGTERM)
            if shutdown.load(Ordering::Relaxed) {
                log::info!("\n\n⚠️  Shutdown signal received");
                // Disable score capture to flush and save the file
                if record.is_some() {
                    log::info!("📼 Stopping recording and saving score file...");
                    let _ = handle.send(StateMessage::DisableScoreCapture);
                    // Give time for the message to be processed
                    std::thread::sleep(std::time::Duration::from_millis(500));

                    // Auto-render if output is an audio format
                    if let Some(ref output_path) = render_output_path {
                        log::info!("🎬 Rendering audio...");
                        let render_args = crate::RenderArgs {
                            score_file: score_capture_path.clone(),
                            output: output_path.clone(),
                            format: None,
                            sample_rate: 48000,
                            bit_depth: 24,
                            tail: 2.0,
                        };
                        if let Err(e) = crate::render::render_score(render_args) {
                            log::error!("Render failed: {}", e);
                        } else {
                            // Clean up temp score file
                            let _ = std::fs::remove_file(&score_capture_path);
                        }
                    }
                }
                log::info!("👋 Exiting gracefully...");
                break;
            }

            // Process any pending eval requests from the HTTP server
            while let Ok(job) = eval_rx.try_recv() {
                let result = match engine.eval::<rhai::Dynamic>(&job.code) {
                    Ok(val) => vibelang_http::EvalResult {
                        success: true,
                        result: if val.is_unit() { None } else { Some(format!("{:?}", val)) },
                        error: None,
                    },
                    Err(e) => vibelang_http::EvalResult {
                        success: false,
                        result: None,
                        error: Some(e.to_string()),
                    },
                };
                let _ = job.response_tx.send(result);
            }

            // Execute any pending MIDI callbacks
            if let Some(ref ast) = current_ast {
                let executed = vibelang_core::api::execute_pending_callbacks(
                    &engine,
                    ast,
                    &mut callback_scope,
                );
                if executed > 0 {
                    log::debug!("Executed {} MIDI callback(s)", executed);
                }
            }

            // Check for sequence completion if --exit-after-sequence was specified
            if let Some(ref seq_name) = exit_after_sequence {
                if handle.is_sequence_completed(seq_name) {
                    log::info!("\n✅ Sequence '{}' completed!", seq_name);
                    // Disable score capture if recording
                    if record.is_some() {
                        log::info!("📼 Stopping recording and saving score file...");
                        let _ = handle.send(StateMessage::DisableScoreCapture);
                        std::thread::sleep(std::time::Duration::from_millis(500));

                        // Auto-render if output is an audio format
                        if let Some(ref output_path) = render_output_path {
                            log::info!("🎬 Rendering audio...");
                            let render_args = crate::RenderArgs {
                                score_file: score_capture_path.clone(),
                                output: output_path.clone(),
                                format: None,
                                sample_rate: 48000,
                                bit_depth: 24,
                                tail: 2.0,
                            };
                            if let Err(e) = crate::render::render_score(render_args) {
                                log::error!("Render failed: {}", e);
                            } else {
                                // Clean up temp score file
                                let _ = std::fs::remove_file(&score_capture_path);
                            }
                        }
                    }
                    log::info!("👋 Exiting...");
                    break;
                }
            }

            // Check for file changes if watch mode is enabled and a file was provided
            if watch {
                if let Some(ref f) = file {
                    let current_modified = fs::metadata(f)
                        .ok()
                        .and_then(|m| m.modified().ok());

                    if current_modified != last_modified {
                        last_modified = current_modified;
                        log::info!("\n🔄 File changed, reloading...");

                        // Signal reload
                        if let Some(h) = vibelang_core::get_handle() {
                            let _ = h.send(StateMessage::BeginReload);
                        }

                        // Clear existing callbacks and MIDI devices before reload
                        vibelang_core::api::clear_callbacks();
                        vibelang_core::api::clear_midi_devices();

                        // Re-read, compile, and execute
                        match fs::read_to_string(f) {
                            Ok(new_script) => {
                                match engine.compile(&new_script) {
                                    Ok(ast) => {
                                        // Set the current script file for source location tracking
                                        let abs_path = f.canonicalize().unwrap_or_else(|_| f.clone());
                                        context::set_current_script_file(Some(abs_path.to_string_lossy().to_string()));

                                        match engine.run_ast(&ast) {
                                            Ok(_) => {
                                                log::info!("   ✓ Reload successful");
                                                // Update the current AST for callback execution
                                                current_ast = Some(ast);
                                            }
                                            Err(e) => {
                                                log::error!("   Reload failed: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("   Compile failed: {}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("   Failed to read file: {}", e);
                            }
                        }

                        // Finalize groups after reload
                        if let Some(h) = vibelang_core::get_handle() {
                            let _ = h.send(StateMessage::FinalizeGroups);
                        }
                    }
                }
            }
        }
    }

    // Explicitly shutdown the runtime to ensure scsynth is killed
    // This triggers Runtime::drop() which kills the scsynth child process
    log::info!("🔌 Shutting down runtime...");
    drop(runtime);
    log::info!("   ✓ Runtime shutdown complete");

    Ok(())
}

/// Run the TUI event loop
fn run_tui_loop(
    vibe_file: Option<&PathBuf>,
    engine: rhai::Engine,
    handle: RuntimeHandle,
    watch: bool,
    _import_paths: &[PathBuf],
    initial_ast: Option<AST>,
    jack_keyboard: Option<vibelang_core::JackMidiOutput>,
) -> Result<()> {
    // Shutdown signal shared between threads
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    // Clone handle for TUI thread
    let tui_handle = handle.clone();

    // Spawn TUI rendering thread (pass in the pre-created JACK output)
    let tui_thread = std::thread::spawn(move || run_tui_render_thread(shutdown_clone, tui_handle, jack_keyboard));

    // Main thread handles file watching, reloading, and callback execution
    let mut last_modified = vibe_file
        .and_then(|f| fs::metadata(f).ok())
        .and_then(|m| m.modified().ok());

    // Track current AST for callback execution
    let mut current_ast = initial_ast;
    let mut callback_scope = rhai::Scope::new();

    loop {
        // Check if TUI thread signaled shutdown
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // Execute any pending MIDI callbacks
        if let Some(ref ast) = current_ast {
            let executed = vibelang_core::api::execute_pending_callbacks(
                &engine,
                ast,
                &mut callback_scope,
            );
            if executed > 0 {
                log::debug!("Executed {} MIDI callback(s)", executed);
            }
        }

        // Check for file changes if watch mode is enabled and file provided
        if watch {
            if let Some(vibe_file) = vibe_file {
                let current_modified = fs::metadata(vibe_file)
                    .ok()
                    .and_then(|m| m.modified().ok());

                if current_modified != last_modified {
                    last_modified = current_modified;
                    log::info!("🔄 File changed, reloading...");

                    // Signal reload
                    let _ = handle.send(StateMessage::BeginReload);

                    // Clear existing callbacks and MIDI devices before reload
                    vibelang_core::api::clear_callbacks();
                    vibelang_core::api::clear_midi_devices();

                    // Re-read, compile, and execute
                    match fs::read_to_string(vibe_file) {
                        Ok(new_script) => {
                            match engine.compile(&new_script) {
                                Ok(ast) => {
                                    // Set the current script file for source location tracking
                                    let abs_path = vibe_file.canonicalize().unwrap_or_else(|_| vibe_file.to_path_buf());
                                    context::set_current_script_file(Some(abs_path.to_string_lossy().to_string()));

                                    match engine.run_ast(&ast) {
                                        Ok(_) => {
                                            log::info!("✅ Reload successful");
                                            // Update the current AST for callback execution
                                            current_ast = Some(ast);
                                        }
                                        Err(e) => {
                                            log::error!("Reload failed: {}", e);
                                        }
                                }
                            }
                            Err(e) => {
                                log::error!("Compile failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to read file: {}", e);
                    }
                }

                // Finalize groups after reload
                let _ = handle.send(StateMessage::FinalizeGroups);
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Wait for TUI thread to finish
    let _ = tui_thread.join();

    Ok(())
}

/// TUI rendering thread - handles all UI updates and input
fn run_tui_render_thread(
    shutdown: Arc<AtomicBool>,
    handle: RuntimeHandle,
    jack_output: Option<vibelang_core::JackMidiOutput>,
) -> Result<()> {
    use crossterm::{
        event::{
            self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture,
            Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
            PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
        },
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::io;
    use std::time::Duration;

    // Initialize TUI event receiver
    let tui_receiver = tui::init_tui_channel();

    // Log the JACK output status (it was created earlier, before script execution)
    if let Some(ref output) = jack_output {
        log::info!("Virtual keyboard using JACK port: {}", output.port_name);
    } else {
        log::warn!("Virtual keyboard disabled (no JACK output)");
    }

    // Create OS-level keyboard listener for reliable key release detection
    // This bypasses terminal limitations by capturing events at the OS level
    let os_keyboard = if jack_output.is_some() && tui::os_keyboard::is_available() {
        match tui::os_keyboard::OsKeyboardListener::new() {
            Some(listener) => {
                log::info!("OS keyboard listener started - key release events will work reliably");
                Some(listener)
            }
            None => {
                log::warn!("Could not start OS keyboard listener - falling back to terminal input");
                None
            }
        }
    } else {
        None
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Enable keyboard enhancement for key release events (kitty protocol)
    // This may not be supported by all terminals, so we ignore errors
    let keyboard_enhanced = execute!(
        stdout,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::REPORT_EVENT_TYPES)
    )
    .is_ok();

    // Enable focus change events so we know when terminal loses/gains focus
    // This is important for the OS keyboard listener to avoid capturing keys
    // when the user switches to another application or tab
    let focus_enabled = execute!(stdout, EnableFocusChange).is_ok();
    if focus_enabled {
        log::debug!("Terminal focus change events enabled");
    }

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = tui::TuiApp::new();

    // If focus events are supported, start with focus (assume we have it)
    // Otherwise fall back to timestamp-based detection
    app.set_focus_events_supported(focus_enabled);
    if focus_enabled {
        app.set_has_focus(true);
    }

    // Set the JACK port name for the keyboard UI display
    app.set_keyboard_port(jack_output.as_ref().map(|o| o.port_name.clone()));

    // Set whether OS keyboard is active
    app.set_os_keyboard_active(os_keyboard.is_some());

    // Main TUI render loop
    let result = loop {
        // Process TUI log events
        while let Ok(tui_event) = tui_receiver.try_recv() {
            app.process_event(tui_event);
        }

        // Get current state from runtime handle
        handle.with_state(|state| {
            app.update_state(state.clone());
        });

        // Check for debounced seek (apply after user stops pressing keys)
        if let Some(seek_offset) = app.check_seek_debounce() {
            // Unmute audio now that scrubbing is done
            let _ = handle.send(StateMessage::SetScrubMute { muted: false });
            if seek_offset.abs() > 0.001 {
                // Get current beat and compute new position
                let current_beat = handle.with_state(|s| s.current_beat);
                let new_beat = (current_beat + seek_offset).max(0.0);
                let _ = handle.send(StateMessage::SeekTransport { beat: new_beat });
            }
        }

        // Check for expired keyboard notes (auto-release for terminals without key-up support)
        // Only needed when OS keyboard listener is not available
        if os_keyboard.is_none() {
            if let Some(ref jack) = jack_output {
                let channel = app.virtual_keyboard.channel();
                for note in app.virtual_keyboard.get_expired_notes() {
                    log::debug!("Auto-releasing expired note: {}", note);
                    let _ = jack.note_off(channel, note);
                }
            }
        }

        // Process OS-level keyboard events (for reliable key release detection)
        // Only process when terminal has focus to avoid capturing keys in other apps
        if let (Some(ref os_kb), Some(ref jack)) = (&os_keyboard, &jack_output) {
            let channel = app.virtual_keyboard.channel();
            while let Some(event) = os_kb.try_recv() {
                // Only process when keyboard is visible AND terminal has focus
                if app.keyboard_active() && app.terminal_has_focus() {
                    match event {
                        tui::os_keyboard::OsKeyEvent::Press(c) => {
                            // Handle special keys
                            if c == '\x1b' {
                                // Escape - hide keyboard
                                for note in app.virtual_keyboard.hide() {
                                    let _ = jack.note_off(channel, note);
                                }
                            } else if c == 'k' || c == 'K' {
                                // K - toggle keyboard off (need shift check separately)
                                // For now, lowercase k toggles too for simplicity
                            } else if c == ' ' {
                                // Space - play/pause
                                let is_running = handle.with_state(|s| s.transport_running);
                                if is_running {
                                    let _ = handle.send(StateMessage::StopScheduler);
                                } else {
                                    let _ = handle.send(StateMessage::StartScheduler);
                                }
                            } else {
                                // Note key press
                                if let Some((note, velocity)) = app.virtual_keyboard.key_down(KeyCode::Char(c)) {
                                    log::debug!("OS key press: '{}' -> note {} on", c, note);
                                    let _ = jack.note_on(channel, note, velocity);
                                }
                            }
                        }
                        tui::os_keyboard::OsKeyEvent::Release(c) => {
                            // Note key release
                            if let Some(note) = app.virtual_keyboard.key_up(KeyCode::Char(c)) {
                                log::debug!("OS key release: '{}' -> note {} off", c, note);
                                let _ = jack.note_off(channel, note);
                            }
                        }
                    }
                }
            }
        }

        // Draw UI
        if let Err(e) = terminal.draw(|f| tui::ui::render_ui(f, &mut app)) {
            // If drawing fails, we should exit
            break Err(e.into());
        }

        // Update flash tracking and VU meter
        app.update_flash_tracking();
        app.update_vu_level();

        // Check for keyboard/terminal events (short timeout to keep UI responsive)
        if event::poll(Duration::from_millis(50))? {
            let evt = event::read()?;
            // Mark that we received a terminal event (for focus detection)
            app.mark_terminal_event();

            match evt {
                Event::Key(key) => {
                    // Log every key event for debugging
                    log::trace!("Key event: code={:?}, kind={:?}, modifiers={:?}", key.code, key.kind, key.modifiers);

                    // Handle search mode input first
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
                    } else if app.midi_export.visible {
                        // MIDI export panel mode
                        match key.code {
                            // Close panel
                            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('M') => {
                                app.midi_export.visible = false;
                            }
                            // Toggle mode (melody/pattern)
                            KeyCode::Tab => {
                                app.midi_export_toggle_mode();
                            }
                            // Voice cursor up
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.midi_export_voice_up();
                            }
                            // Voice cursor down
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.midi_export_voice_down();
                            }
                            // Toggle voice selection
                            KeyCode::Char(' ') => {
                                app.midi_export_toggle_voice();
                            }
                            // Decrease quantization (finer)
                            KeyCode::Left | KeyCode::Char('h') => {
                                if let Some(new_quant) = app.midi_export_decrease_quantization() {
                                    let _ = handle.send(StateMessage::MidiSetRecordingQuantization {
                                        positions_per_bar: new_quant,
                                    });
                                }
                            }
                            // Increase quantization (coarser)
                            KeyCode::Right | KeyCode::Char('l') => {
                                if let Some(new_quant) = app.midi_export_increase_quantization() {
                                    let _ = handle.send(StateMessage::MidiSetRecordingQuantization {
                                        positions_per_bar: new_quant,
                                    });
                                }
                            }
                            // Increase bar count
                            KeyCode::Char('+') | KeyCode::Char('=') => {
                                app.midi_export_increase_bars();
                            }
                            // Decrease bar count
                            KeyCode::Char('-') | KeyCode::Char('_') => {
                                app.midi_export_decrease_bars();
                            }
                            // Select all voices
                            KeyCode::Char('a') => {
                                app.midi_export_select_all();
                            }
                            // Select no voices
                            KeyCode::Char('n') => {
                                app.midi_export_select_none();
                            }
                            // Copy to clipboard
                            KeyCode::Enter | KeyCode::Char('y') => {
                                app.copy_midi_export_to_clipboard();
                            }
                            // Clear recording history
                            KeyCode::Char('c') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                let _ = handle.send(StateMessage::MidiClearRecording);
                                // Refresh voices list and preview
                                app.midi_export.available_voices.clear();
                                app.midi_export.selected_voices.clear();
                                app.update_midi_export_preview();
                            }
                            // Quit with Ctrl+C
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                shutdown.store(true, Ordering::Relaxed);
                                break Ok(());
                            }
                            _ => {}
                        }
                    } else if app.keyboard_active() && jack_output.is_some() {
                        // Virtual keyboard mode - intercept note keys
                        let jack = jack_output.as_ref().unwrap();
                        let channel = app.virtual_keyboard.channel();
                        log::debug!("Keyboard mode active - processing key: code={:?}, kind={:?}", key.code, key.kind);

                        // Handle key release events for note-off
                        if key.kind == KeyEventKind::Release {
                            if let KeyCode::Char(c) = key.code {
                                if let Some(note) = app.virtual_keyboard.key_up(KeyCode::Char(c)) {
                                    let _ = jack.note_off(channel, note);
                                }
                            }
                        } else if key.kind == KeyEventKind::Press {
                            // Handle key press events
                            match key.code {
                                // Quit still works
                                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    // Release all notes before quitting
                                    for note in app.virtual_keyboard.hide() {
                                        let _ = jack.note_off(channel, note);
                                    }
                                    shutdown.store(true, Ordering::Relaxed);
                                    break Ok(());
                                }
                                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    // Release all notes before quitting
                                    for note in app.virtual_keyboard.hide() {
                                        let _ = jack.note_off(channel, note);
                                    }
                                    shutdown.store(true, Ordering::Relaxed);
                                    break Ok(());
                                }
                                // Toggle keyboard off
                                KeyCode::Char('K') | KeyCode::Esc => {
                                    // Release all notes when hiding keyboard
                                    for note in app.virtual_keyboard.hide() {
                                        let _ = jack.note_off(channel, note);
                                    }
                                }
                                // Octave shift
                                KeyCode::Char('<') => {
                                    for note in app.virtual_keyboard.octave_down() {
                                        let _ = jack.note_off(channel, note);
                                    }
                                }
                                KeyCode::Char('>') => {
                                    for note in app.virtual_keyboard.octave_up() {
                                        let _ = jack.note_off(channel, note);
                                    }
                                }
                                // Play/pause still works
                                KeyCode::Char(' ') => {
                                    let is_running = handle.with_state(|s| s.transport_running);
                                    if is_running {
                                        let _ = handle.send(StateMessage::StopScheduler);
                                    } else {
                                        let _ = handle.send(StateMessage::StartScheduler);
                                    }
                                }
                                // Handle note key presses
                                KeyCode::Char(c) => {
                                    log::debug!("Keyboard key press: '{}', kind: {:?}", c, key.kind);
                                    if let Some((note, velocity)) = app.virtual_keyboard.key_down(KeyCode::Char(c)) {
                                        log::debug!("Sending note-on: note={}, velocity={}", note, velocity);
                                        let _ = jack.note_on(channel, note, velocity);
                                    } else {
                                        log::debug!("key_down returned None (note already pressed or not a keyboard key)");
                                    }
                                }
                                _ => {}
                            }
                        } else if key.kind == KeyEventKind::Repeat {
                            // Key repeat - extend the note duration OR trigger if not already playing
                            // Some terminals (like VS Code) may send Repeat instead of Press
                            if let KeyCode::Char(c) = key.code {
                                // First try to trigger note-on (in case this is the first event)
                                if let Some((note, velocity)) = app.virtual_keyboard.key_down(KeyCode::Char(c)) {
                                    log::debug!("Repeat event triggered note-on: note={}, velocity={}", note, velocity);
                                    let _ = jack.note_on(channel, note, velocity);
                                }
                                // key_down already updates timestamp for existing notes
                            }
                        } else {
                            // Log unexpected event kinds
                            log::debug!("Unexpected key event kind: {:?}", key.kind);
                        }
                    } else {
                        // Normal mode key handling
                        match key.code {
                            // Quit
                            KeyCode::Char('q') => {
                                shutdown.store(true, Ordering::Relaxed);
                                break Ok(());
                            }
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                shutdown.store(true, Ordering::Relaxed);
                                break Ok(());
                            }
                            // Help modal
                            KeyCode::Char('?') => {
                                app.toggle_help_modal();
                            }
                            // Error modal
                            KeyCode::Char('e') => {
                                app.toggle_error_modal();
                            }
                            KeyCode::Esc => {
                                if app.show_help_modal {
                                    app.show_help_modal = false;
                                } else {
                                    app.close_error_modal();
                                }
                            }
                            // Toggle virtual keyboard
                            KeyCode::Char('K') => {
                                app.virtual_keyboard.toggle();
                                log::info!("Virtual keyboard toggled: visible={}", app.virtual_keyboard.visible);
                            }
                            // Toggle MIDI export panel
                            KeyCode::Char('M') => {
                                app.toggle_midi_export_panel();
                            }
                            // Filter toggle
                            KeyCode::Char('f') => {
                                app.toggle_hide_inactive();
                            }
                            // Focus toggle between hierarchy and log
                            KeyCode::Char('l') | KeyCode::Tab => {
                                app.toggle_focus();
                            }
                            // Log maximize toggle
                            KeyCode::Char('L') => {
                                app.toggle_log_maximized();
                            }
                            // Navigation
                            KeyCode::Up | KeyCode::Char('k') => {
                                app.move_selection_up();
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                app.move_selection_down();
                            }
                            // Page navigation
                            KeyCode::PageUp => {
                                app.move_selection_page_up();
                            }
                            KeyCode::PageDown => {
                                app.move_selection_page_down();
                            }
                            // Jump to active
                            KeyCode::Char('a') => {
                                app.jump_to_active();
                            }
                            // Expand/collapse all
                            KeyCode::Char('[') => {
                                app.collapse_all();
                            }
                            KeyCode::Char(']') => {
                                app.expand_all();
                            }
                            // Toggle collapse/expand selected
                            KeyCode::Enter => {
                                app.toggle_collapse();
                            }
                            // Play/pause transport (space)
                            KeyCode::Char(' ') => {
                                let is_running = handle.with_state(|s| s.transport_running);
                                if is_running {
                                    let _ = handle.send(StateMessage::StopScheduler);
                                } else {
                                    let _ = handle.send(StateMessage::StartScheduler);
                                }
                            }
                            // Search hierarchy
                            KeyCode::Char('/') => {
                                if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    app.enter_log_search_mode();
                                } else {
                                    app.enter_search_mode();
                                }
                            }
                            // Log level filter (1-5)
                            KeyCode::Char('1') => app.set_log_level(1),
                            KeyCode::Char('2') => app.set_log_level(2),
                            KeyCode::Char('3') => app.set_log_level(3),
                            KeyCode::Char('4') => app.set_log_level(4),
                            KeyCode::Char('5') => app.set_log_level(5),
                            // Time navigation (debounced to avoid sound glitches)
                            KeyCode::Left => {
                                let was_scrubbing = app.is_scrubbing;
                                let step = if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    app.state
                                        .as_ref()
                                        .map(|s| s.time_signature.beats_per_bar())
                                        .unwrap_or(4.0)
                                } else {
                                    1.0
                                };
                                app.add_pending_seek(-step);
                                // Mute audio when scrubbing starts
                                if !was_scrubbing {
                                    let _ = handle.send(StateMessage::SetScrubMute { muted: true });
                                }
                            }
                            KeyCode::Right => {
                                let was_scrubbing = app.is_scrubbing;
                                let step = if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    app.state
                                        .as_ref()
                                        .map(|s| s.time_signature.beats_per_bar())
                                        .unwrap_or(4.0)
                                } else {
                                    1.0
                                };
                                app.add_pending_seek(step);
                                // Mute audio when scrubbing starts
                                if !was_scrubbing {
                                    let _ = handle.send(StateMessage::SetScrubMute { muted: true });
                                }
                            }
                            // Jump to start (immediate, not debounced)
                            KeyCode::Char('0') | KeyCode::Home => {
                                // Unmute if we were scrubbing
                                if app.is_scrubbing {
                                    let _ = handle.send(StateMessage::SetScrubMute { muted: false });
                                }
                                app.cancel_pending_seek();
                                let _ = handle.send(StateMessage::SeekTransport { beat: 0.0 });
                            }
                            _ => {}
                        }
                    }
                },
                Event::Mouse(mouse_event) => {
                    use crossterm::event::MouseEventKind;
                    match mouse_event.kind {
                        MouseEventKind::ScrollUp => {
                            app.move_selection_up();
                        }
                        MouseEventKind::ScrollDown => {
                            app.move_selection_down();
                        }
                        _ => {}
                    }
                }
                Event::Resize(_, _) => {
                    // Terminal was resized, the next draw will handle it automatically
                }
                Event::FocusGained => {
                    log::debug!("Terminal focus gained");
                    app.set_has_focus(true);
                }
                Event::FocusLost => {
                    log::debug!("Terminal focus lost");
                    app.set_has_focus(false);
                    // Release all notes when losing focus to prevent stuck notes
                    if let Some(ref jack) = jack_output {
                        let channel = app.virtual_keyboard.channel();
                        for note in app.virtual_keyboard.release_all() {
                            let _ = jack.note_off(channel, note);
                        }
                    }
                }
                _ => {}
            }
        }
    };

    // Cleanup terminal
    disable_raw_mode()?;
    // Pop keyboard enhancement flags if we enabled them
    if keyboard_enhanced {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    // Disable focus change events if we enabled them
    if focus_enabled {
        let _ = execute!(terminal.backend_mut(), DisableFocusChange);
    }
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

/// Extract the synthdef name from SuperCollider synthdef bytes.
///
/// SuperCollider synthdef format:
/// - 4 bytes: "SCgf" magic
/// - 4 bytes: file version (int32 big-endian)
/// - 2 bytes: number of synthdefs (int16 big-endian)
/// - For each synthdef:
///   - pstring: 1 byte length + name bytes
fn extract_synthdef_name(bytes: &[u8]) -> Option<String> {
    // Minimum size: 4 (magic) + 4 (version) + 2 (count) + 1 (name length) + 1 (at least one char)
    if bytes.len() < 12 {
        return None;
    }

    // Check magic "SCgf"
    if &bytes[0..4] != b"SCgf" {
        return None;
    }

    // Skip version (4 bytes) and count (2 bytes), get name at offset 10
    let name_len = bytes[10] as usize;
    if bytes.len() < 11 + name_len {
        return None;
    }

    String::from_utf8(bytes[11..11 + name_len].to_vec()).ok()
}
