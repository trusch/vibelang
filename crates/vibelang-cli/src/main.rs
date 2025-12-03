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

mod tui;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use vibelang_core::state::StateMessage;
use vibelang_core::RuntimeHandle;

/// VibeLang - SuperCollider Live Coding
#[derive(Parser, Debug)]
#[command(name = "vibe")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "A musical programming language for SuperCollider", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run a .vibe file
    Run {
        /// Path to the .vibe file to execute
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// Enable watch mode for live reloading
        #[arg(short, long)]
        watch: bool,

        /// Enable TUI mode (Terminal User Interface)
        #[arg(long)]
        tui: bool,

        /// Additional import directories
        #[arg(short = 'I', long = "import-path", value_name = "PATH")]
        import_paths: Vec<PathBuf>,
    },

    /// Show version information
    Version,
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Run {
            file,
            watch,
            tui: tui_mode,
            import_paths,
        } => run_vibe_file(file, watch, tui_mode, import_paths),
        Commands::Version => {
            println!("vibe {}", env!("CARGO_PKG_VERSION"));
            println!();
            println!("Part of the VibeLang project");
            println!("A musical programming language for SuperCollider");
            println!();
            println!("Modular Architecture:");
            println!("  - vibelang-core: State management and scheduling");
            println!("  - vibelang-dsp:  SynthDef/UGen generation");
            println!(
                "  - vibelang-std:  Standard library ({} files)",
                count_stdlib_files()
            );
            Ok(())
        }
    }
}

/// Count the number of .vibe files in the stdlib.
fn count_stdlib_files() -> usize {
    let stdlib_path = vibelang_std::stdlib_path();
    walkdir(stdlib_path)
}

fn walkdir(path: &str) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += walkdir(path.to_str().unwrap_or(""));
            } else if path.extension().and_then(|s| s.to_str()) == Some("vibe") {
                count += 1;
            }
        }
    }
    count
}

fn run_vibe_file(
    file: PathBuf,
    watch: bool,
    tui_mode: bool,
    import_paths: Vec<PathBuf>,
) -> Result<()> {
    // Initialize logger based on TUI mode
    if tui_mode {
        tui::init_tui_logger();
    } else {
        tui::init_logger();
    }

    // Validate the file exists
    if !file.exists() {
        anyhow::bail!("File not found: {}", file.display());
    }

    // Check file extension
    if file.extension().and_then(|s| s.to_str()) != Some("vibe") {
        log::warn!("File doesn't have .vibe extension");
    }

    if !tui_mode {
        println!("🎵 VibeLang - SuperCollider Live Coding");
        println!("=======================================\n");
        println!("📄 Loading: {}\n", file.display());
    } else {
        log::info!("🎵 VibeLang - SuperCollider Live Coding");
        log::info!("📄 Loading: {}", file.display());
    }

    // 1-3. Start runtime (includes scsynth process, connection, and runtime thread)
    log::info!("Starting runtime...");
    let runtime = vibelang_core::Runtime::start_default()
        .context("Failed to start runtime")?;
    let handle = runtime.handle();

    // Initialize the API with the runtime handle
    vibelang_core::init_api(handle.clone());

    // Set up the synthdef deploy callback
    let deploy_handle = handle.clone();
    vibelang_dsp::set_deploy_callback(move |bytes| {
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
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Could not determine parent directory of file"))?
        .to_path_buf();

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

    // 7. Read and execute the script
    log::info!("7. Executing .vibe file...");
    let script = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    match engine.eval::<()>(&script) {
        Ok(_) => {
            log::info!("   ✓ Script executed successfully");
        }
        Err(e) => {
            log::error!("Script error: {}", e);
            // Continue running to allow sounds to play
        }
    }

    // Finalize groups
    handle.send(StateMessage::FinalizeGroups)?;
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Keep the process running
    if tui_mode {
        // TUI mode - run the TUI event loop
        run_tui_loop(&file, engine, handle.clone(), watch, &import_paths)?;
    } else {
        // Set up Ctrl-C handler
        ctrlc::set_handler(move || {
            log::info!("\n\n⚠️  Interrupted by user (Ctrl+C)");
            log::info!("👋 Exiting...");
            std::process::exit(0);
        })
        .expect("Error setting Ctrl-C handler");

        if watch {
            log::info!("\n8. Watch mode enabled - monitoring file for changes...");
            log::info!("   (Press Ctrl+C to exit)\n");

            // Simple watch loop - poll file modification time
            let mut last_modified = fs::metadata(&file)
                .ok()
                .and_then(|m| m.modified().ok());

            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));

                let current_modified = fs::metadata(&file)
                    .ok()
                    .and_then(|m| m.modified().ok());

                if current_modified != last_modified {
                    last_modified = current_modified;
                    log::info!("\n🔄 File changed, reloading...");

                    // Signal reload
                    if let Some(h) = vibelang_core::get_handle() {
                        let _ = h.send(StateMessage::BeginReload);
                    }

                    // Re-read and execute
                    match fs::read_to_string(&file) {
                        Ok(new_script) => {
                            match engine.eval::<()>(&new_script) {
                                Ok(_) => {
                                    log::info!("   ✓ Reload successful");
                                }
                                Err(e) => {
                                    log::error!("   Reload failed: {}", e);
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
        } else {
            log::info!("\n8. Script running... (Press Ctrl+C to exit)");

            // Wait indefinitely
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }

    Ok(())
}

/// Run the TUI event loop
fn run_tui_loop(
    vibe_file: &PathBuf,
    engine: rhai::Engine,
    handle: RuntimeHandle,
    watch: bool,
    _import_paths: &[PathBuf],
) -> Result<()> {
    // Shutdown signal shared between threads
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();

    // Clone handle for TUI thread
    let tui_handle = handle.clone();

    // Spawn TUI rendering thread
    let tui_thread = std::thread::spawn(move || run_tui_render_thread(shutdown_clone, tui_handle));

    // Main thread handles file watching and reloading
    let mut last_modified = fs::metadata(vibe_file)
        .ok()
        .and_then(|m| m.modified().ok());

    loop {
        // Check if TUI thread signaled shutdown
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // Check for file changes if watch mode is enabled
        if watch {
            let current_modified = fs::metadata(vibe_file)
                .ok()
                .and_then(|m| m.modified().ok());

            if current_modified != last_modified {
                last_modified = current_modified;
                log::info!("🔄 File changed, reloading...");

                // Signal reload
                let _ = handle.send(StateMessage::BeginReload);

                // Re-read and execute
                match fs::read_to_string(vibe_file) {
                    Ok(new_script) => {
                        match engine.eval::<()>(&new_script) {
                            Ok(_) => {
                                log::info!("✅ Reload successful");
                            }
                            Err(e) => {
                                log::error!("Reload failed: {}", e);
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

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Wait for TUI thread to finish
    let _ = tui_thread.join();

    Ok(())
}

/// TUI rendering thread - handles all UI updates and input
fn run_tui_render_thread(shutdown: Arc<AtomicBool>, handle: RuntimeHandle) -> Result<()> {
    use crossterm::{
        event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::{backend::CrosstermBackend, Terminal};
    use std::io;
    use std::time::Duration;

    // Initialize TUI event receiver
    let tui_receiver = tui::init_tui_channel();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = tui::TuiApp::new();

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
            match event::read()? {
                Event::Key(key) => {
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
                _ => {}
            }
        }
    };

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}
