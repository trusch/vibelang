//! VibeLang CLI v2 - using the new core2 async runtime
//!
//! This is the main entry point for the vibelang command-line tool with full TUI support.

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

use vibelang_core2::{
    Message, ReloadMessage, Runtime, RuntimeHandle, ScsynthBackend, ScsynthConfig, ScsynthProcess,
    State, TransportMessage,
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

        /// Watch for file changes and reload
        #[arg(short, long)]
        watch: bool,

        /// Use TUI mode (terminal user interface)
        #[arg(long)]
        tui: bool,

        /// Start HTTP API server
        #[arg(long)]
        #[cfg_attr(not(feature = "api"), arg(hide = true))]
        api: bool,

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
            watch: true,
            tui: false,
            api: false,
            api_port: 1606,
            include_paths: Vec::new(),
            scsynth_addr: "127.0.0.1:57110".to_string(),
            no_boot: false,
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
            watch,
            tui,
            api,
            api_port,
            include_paths,
            scsynth_addr,
            no_boot,
        } => {
            if tui {
                run_tui_mode(file, watch, api, api_port, include_paths, scsynth_addr, no_boot).await
            } else {
                run_simple_mode(file, watch, api, api_port, include_paths, scsynth_addr, no_boot).await
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
        Commands::Lsp => {
            vibelang_lsp2::run_lsp_server().await
        }
    }
}

/// Run in simple mode (no TUI, just console output)
async fn run_simple_mode(
    file: PathBuf,
    watch: bool,
    #[allow(unused_variables)] api: bool,
    #[allow(unused_variables)] api_port: u16,
    include_paths: Vec<PathBuf>,
    scsynth_addr: String,
    no_boot: bool,
) -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive("vibelang=info".parse()?)
                .add_directive("vibe2=info".parse()?),
        )
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
        let config = ScsynthConfig::default();
        let process = ScsynthProcess::spawn(config).context("Failed to start scsynth")?;
        process.wait_startup(Duration::from_secs(3)).await;
        info!("scsynth started");
        Some(process)
    };

    // Connect to scsynth
    let backend = ScsynthBackend::connect(&scsynth_addr)
        .await
        .context("Failed to connect to scsynth")?;

    // Create runtime
    let mut runtime = Runtime::new(backend);
    let handle = runtime.handle();

    // Load built-in synthdefs
    info!("Loading built-in synthdefs...");
    runtime
        .load_builtins()
        .await
        .context("Failed to load built-in synthdefs")?;

    // Set up deploy callback for custom synthdefs defined in scripts
    let deploy_handle = handle.clone();
    vibelang_dsp::set_deploy_callback(move |bytes| {
        deploy_handle
            .try_send(vibelang_core2::Message::SynthDef(
                vibelang_core2::SynthDefMessage::Load {
                    name: extract_synthdef_name(&bytes).unwrap_or_else(|| "unknown".to_string()),
                    data: bytes,
                },
            ))
            .map_err(|e| e.to_string())
    });

    // Get state handle before spawning runtime (needed for HTTP API)
    let state_handle = runtime.state().clone();

    // Start HTTP API server if requested (before spawning runtime task since runtime is moved)
    #[cfg(feature = "api")]
    if api {
        let api_handle = handle.clone();
        let api_state = state_handle.clone();
        tokio::spawn(async move {
            vibelang_http2::start_server(api_handle, api_state, api_port, None).await;
        });
        info!("HTTP API server started on port {}", api_port);
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
    let state = execute_script(&file, &include_paths)?;
    handle
        .send(ReloadMessage::Apply { state }.into())
        .await?;

    // Start transport
    handle
        .send(Message::Transport(TransportMessage::Start))
        .await?;
    info!("Transport started");

    // Watch for changes if requested
    if watch {
        let (tx, mut rx) = mpsc::channel::<PathBuf>(16);
        let file_clone = file.clone();
        let include_paths_clone = include_paths.clone();
        let handle_clone = handle.clone();

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
                    match execute_script(&file_clone, &include_paths_clone) {
                        Ok(state) => {
                            if let Err(e) = handle_clone
                                .send(ReloadMessage::Apply { state }.into())
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
    }

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
async fn run_tui_mode(
    file: PathBuf,
    watch: bool,
    #[allow(unused_variables)] api: bool,
    #[allow(unused_variables)] api_port: u16,
    include_paths: Vec<PathBuf>,
    scsynth_addr: String,
    no_boot: bool,
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
        let config = ScsynthConfig::default();
        match ScsynthProcess::spawn(config) {
            Ok(process) => {
                process.wait_startup(Duration::from_secs(3)).await;
                log::info!("scsynth started");
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

    // Create runtime
    let mut runtime = Runtime::new(osc_backend);
    let handle = runtime.handle();

    // Load built-in synthdefs
    log::info!("Loading built-in synthdefs...");
    if let Err(e) = runtime.load_builtins().await {
        log::error!("Failed to load built-in synthdefs: {}", e);
    }

    // Set up deploy callback for custom synthdefs
    let deploy_handle = handle.clone();
    vibelang_dsp::set_deploy_callback(move |bytes| {
        deploy_handle
            .try_send(vibelang_core2::Message::SynthDef(
                vibelang_core2::SynthDefMessage::Load {
                    name: extract_synthdef_name(&bytes).unwrap_or_else(|| "unknown".to_string()),
                    data: bytes,
                },
            ))
            .map_err(|e| e.to_string())
    });

    // Get state handle before spawning runtime (needed for HTTP API)
    let state_handle = runtime.state().clone();

    // Start HTTP API server if requested
    #[cfg(feature = "api")]
    if api {
        let api_handle = handle.clone();
        let api_state = state_handle.clone();
        tokio::spawn(async move {
            vibelang_http2::start_server(api_handle, api_state, api_port, None).await;
        });
        log::info!("HTTP API server started on port {}", api_port);
    }

    // Create shutdown flag
    let running = Arc::new(AtomicBool::new(true));

    // Create channels
    let (state_tx, mut state_rx) = mpsc::channel::<vibelang_core2::State>(32);
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
    match execute_script(&file, &include_paths) {
        Ok(state) => {
            if let Err(e) = handle
                .send(ReloadMessage::Apply { state }.into())
                .await
            {
                log::error!("Failed to apply script: {}", e);
                app.process_event(tui::TuiEvent::Error(format!("{}", e)));
            } else {
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
    let reload_running = running.clone();
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
                match execute_script(&changed_file, &include_paths_clone) {
                    Ok(state) => {
                        if let Err(e) = reload_handle
                            .send(ReloadMessage::Apply { state }.into())
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
                                log::debug!("Note on: {} vel:{}", note, velocity);
                            }
                        }
                    }
                    tui::os_keyboard::OsKeyEvent::KeyUp(c) => {
                        if app.keyboard_active() {
                            if let Some(note) = app.virtual_keyboard.key_up(KeyCode::Char(c)) {
                                log::debug!("Note off: {}", note);
                            }
                        }
                    }
                }
            }
        }

        // Handle pending seek
        if let Some(seek_offset) = app.check_seek_debounce() {
            // Calculate target beat from current position + offset
            let current_beat = app.runtime_state
                .as_ref()
                .map(|s| s.current_beat.to_f64())
                .unwrap_or(0.0);
            let target_beat = (current_beat + seek_offset).max(0.0);
            let _ = handle
                .send(Message::Transport(TransportMessage::Seek {
                    beat: vibelang_core2::Beat::from_f64(target_beat),
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
                                for note in released {
                                    log::debug!("Note off: {}", note);
                                }
                            }
                            KeyCode::Char('<') => {
                                let released = app.virtual_keyboard.octave_down();
                                for note in released {
                                    log::debug!("Note off: {}", note);
                                }
                            }
                            KeyCode::Char('>') => {
                                let released = app.virtual_keyboard.octave_up();
                                for note in released {
                                    log::debug!("Note off: {}", note);
                                }
                            }
                            KeyCode::Char(' ') => {
                                // Toggle playback based on current state
                                let is_playing = app.runtime_state
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
                                    log::debug!("Note on: {} vel:{}", note, velocity);
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
                            let is_playing = app.runtime_state
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
                                    beat: vibelang_core2::Beat::ZERO,
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
                        KeyCode::Tab
                        | KeyCode::Char('l')
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
                        KeyCode::Char(c) if c >= '1' && c <= '5' => {
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
        for note in expired {
            log::debug!("Note off (expired): {}", note);
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

fn execute_script(
    file: &PathBuf,
    include_paths: &[PathBuf],
) -> Result<vibelang_core2::reload::ScriptState> {
    let mut engine = ScriptEngine::new();

    // Add import paths
    for path in include_paths {
        engine.add_import_path(path.clone());
    }

    // Add script directory to import paths
    if let Some(parent) = file.parent() {
        engine.add_import_path(parent.to_path_buf());
    }

    // Read and execute script
    let script = std::fs::read_to_string(file)
        .with_context(|| format!("Failed to read script: {}", file.display()))?;

    engine
        .execute(&script)
        .map_err(|e| anyhow::anyhow!("Script error: {}", e))
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
