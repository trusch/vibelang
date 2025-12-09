//! vibe-keys - Terminal MIDI Keyboard for VibeLang
//!
//! A terminal-based MIDI keyboard that lets you play music using your computer keyboard.

use anyhow::Result;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers, EnableFocusChange, DisableFocusChange},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear};
use std::io::{self, stdout};
use std::time::{Duration, Instant};

use vibelang_keys::{
    config::Config,
    keyboard::VirtualKeyboard,
    midi::{is_jack_running, JackMidiOutput, MidiOutput},
    os_keyboard::{is_available as os_keyboard_available, OsKeyEvent, OsKeyboardListener},
    ui::render_keyboard_standalone,
};

#[derive(Parser)]
#[command(name = "vibe-keys")]
#[command(author, version, about = "Terminal MIDI Keyboard for VibeLang", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Config file path (default: ~/.config/vibe-keys/config.toml)
    #[arg(short, long)]
    config: Option<String>,

    /// Use US QWERTY layout instead of German QWERTZ
    #[arg(long)]
    us_layout: bool,

    /// JACK client name
    #[arg(long, default_value = "vibe-keys")]
    client_name: String,

    /// Initial octave (0-9, default 3 for C3)
    #[arg(short, long)]
    octave: Option<u8>,

    /// MIDI channel (0-15)
    #[arg(long, default_value = "0")]
    channel: u8,

    /// Velocity (1-127)
    #[arg(long, default_value = "100")]
    velocity: u8,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a default configuration file
    Init,
    /// Show the configuration file path
    ConfigPath,
    /// List available JACK MIDI ports
    ListPorts,
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init) => {
            let path = Config::create_default_config_file()?;
            println!("Created default config at: {}", path.display());
            return Ok(());
        }
        Some(Commands::ConfigPath) => {
            let path = Config::config_path()?;
            println!("{}", path.display());
            return Ok(());
        }
        Some(Commands::ListPorts) => {
            if !is_jack_running() {
                println!("JACK is not running");
                return Ok(());
            }
            let ports = vibelang_keys::midi::list_jack_midi_ports();
            if ports.is_empty() {
                println!("No JACK MIDI input ports found");
            } else {
                println!("Available JACK MIDI input ports:");
                for port in ports {
                    println!("  {}", port);
                }
            }
            return Ok(());
        }
        None => {}
    }

    // Load config
    let mut config = if let Some(path) = cli.config {
        let content = std::fs::read_to_string(&path)?;
        toml::from_str(&content)?
    } else {
        Config::load_or_default()
    };

    // Apply CLI overrides
    if cli.us_layout {
        config.keyboard.layout = vibelang_keys::config::KeyboardLayout::Us;
    }
    if let Some(octave) = cli.octave {
        config.keyboard.base_note = 12 + (octave.min(9) * 12);
    }
    config.keyboard.channel = cli.channel.min(15);
    config.keyboard.velocity = cli.velocity.clamp(1, 127);
    config.midi.client_name = cli.client_name;

    // Run the TUI
    run_tui(config)
}

fn run_tui(config: Config) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableFocusChange)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create keyboard
    let keyboard_config = config.to_keyboard_config();
    let mut keyboard = VirtualKeyboard::new(keyboard_config);
    keyboard.visible = true;

    // Create MIDI output
    let midi_output: Option<Box<dyn MidiOutput>> = if is_jack_running() {
        match JackMidiOutput::from_settings(&config.midi) {
            Ok(output) => {
                log::info!("JACK MIDI output created: {}", output.port_name());
                Some(Box::new(output))
            }
            Err(e) => {
                log::warn!("Failed to create JACK MIDI output: {}", e);
                None
            }
        }
    } else {
        log::warn!("JACK is not running, MIDI output disabled");
        None
    };

    // Create OS keyboard listener
    let os_keyboard = if os_keyboard_available() {
        OsKeyboardListener::new()
    } else {
        None
    };
    let os_keyboard_active = os_keyboard.is_some();

    // Main loop
    let result = run_event_loop(
        &mut terminal,
        &mut keyboard,
        midi_output.as_deref(),
        os_keyboard.as_ref(),
        os_keyboard_active,
        &config.theme,
    );

    // Cleanup
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableFocusChange)?;

    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    keyboard: &mut VirtualKeyboard,
    midi_output: Option<&dyn MidiOutput>,
    os_keyboard: Option<&OsKeyboardListener>,
    os_keyboard_active: bool,
    theme: &vibelang_keys::config::Theme,
) -> Result<()> {
    let port_name = midi_output.map(|m| m.port_name());

    // Focus tracking
    let mut has_focus = true;
    let mut last_activity = Instant::now();
    const FOCUS_TIMEOUT_MS: u128 = 500; // Assume focus lost if no terminal events for this long

    loop {
        // Draw
        terminal.draw(|frame| {
            let area = frame.area();

            // Clear the screen with a dark background
            frame.render_widget(Clear, area);
            let bg_block = Block::default().style(Style::default().bg(Color::Rgb(20, 20, 30)));
            frame.render_widget(bg_block, area);

            // Render centered keyboard
            render_keyboard_standalone(
                frame,
                area,
                keyboard,
                port_name,
                os_keyboard_active && has_focus,
                theme,
            );
        })?;

        // Process OS keyboard events only when focused
        if has_focus {
            if let Some(os_kb) = os_keyboard {
                while let Some(event) = os_kb.try_recv() {
                    match event {
                        OsKeyEvent::Press(c) => {
                            if c == '\x1b' {
                                // ESC - quit
                                return Ok(());
                            } else if let Some((note, velocity)) = keyboard.key_down(c) {
                                if let Some(midi) = midi_output {
                                    midi.note_on(keyboard.channel(), note, velocity);
                                }
                            }
                        }
                        OsKeyEvent::Release(c) => {
                            if let Some(note) = keyboard.key_up(c) {
                                if let Some(midi) = midi_output {
                                    midi.note_off(keyboard.channel(), note);
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // Drain OS keyboard events when not focused
            if let Some(os_kb) = os_keyboard {
                while os_kb.try_recv().is_some() {}
            }
        }

        // Check for auto-release (for terminals without key-up detection)
        if os_keyboard.is_none() {
            let expired = keyboard.get_expired_notes();
            send_note_offs(midi_output, keyboard.channel(), &expired);
        }

        // Poll for terminal events
        if event::poll(Duration::from_millis(16))? {
            let event = event::read()?;

            match event {
                Event::FocusGained => {
                    has_focus = true;
                    last_activity = Instant::now();
                }
                Event::FocusLost => {
                    has_focus = false;
                    // Release all notes when losing focus
                    let released = keyboard.release_all();
                    send_note_offs(midi_output, keyboard.channel(), &released);
                }
                Event::Key(key) => {
                    last_activity = Instant::now();
                    has_focus = true; // Key events mean we have focus

                    // Only process key press events (not release/repeat unless OS keyboard not available)
                    let should_process = match key.kind {
                        KeyEventKind::Press => true,
                        KeyEventKind::Repeat if os_keyboard.is_none() => {
                            // Touch note on repeat to extend auto-release
                            if let KeyCode::Char(c) = key.code {
                                keyboard.touch_note(c);
                            }
                            false
                        }
                        KeyEventKind::Release if os_keyboard.is_none() => {
                            // Process release if no OS keyboard
                            if let KeyCode::Char(c) = key.code {
                                if let Some(note) = keyboard.key_up(c) {
                                    if let Some(midi) = midi_output {
                                        midi.note_off(keyboard.channel(), note);
                                    }
                                }
                            }
                            false
                        }
                        _ => false,
                    };

                    if !should_process {
                        continue;
                    }

                    // Handle key press
                    match key.code {
                        KeyCode::Esc => {
                            // Release all notes and quit
                            let released = keyboard.release_all();
                            send_note_offs(midi_output, keyboard.channel(), &released);
                            return Ok(());
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Ctrl+C - quit
                            let released = keyboard.release_all();
                            send_note_offs(midi_output, keyboard.channel(), &released);
                            return Ok(());
                        }
                        // Octave down: < (Shift+,) or Left arrow
                        KeyCode::Char('<') => {
                            let released = keyboard.octave_down();
                            send_note_offs(midi_output, keyboard.channel(), &released);
                        }
                        KeyCode::Left => {
                            let released = keyboard.octave_down();
                            send_note_offs(midi_output, keyboard.channel(), &released);
                        }
                        // Octave up: > (Shift+.) or Right arrow
                        KeyCode::Char('>') => {
                            let released = keyboard.octave_up();
                            send_note_offs(midi_output, keyboard.channel(), &released);
                        }
                        KeyCode::Right => {
                            let released = keyboard.octave_up();
                            send_note_offs(midi_output, keyboard.channel(), &released);
                        }
                        KeyCode::Char(c) if os_keyboard.is_none() => {
                            // Only process keyboard input if OS keyboard not available
                            if let Some((note, velocity)) = keyboard.key_down(c) {
                                if let Some(midi) = midi_output {
                                    midi.note_on(keyboard.channel(), note, velocity);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {
                    last_activity = Instant::now();
                }
            }
        } else {
            // No terminal event - check if we should assume focus lost
            // (for terminals that don't support focus events)
            if has_focus && os_keyboard.is_some() && last_activity.elapsed().as_millis() > FOCUS_TIMEOUT_MS {
                // Don't auto-lose focus, but be more conservative
                // This is a fallback for terminals without focus event support
            }
        }
    }
}

fn send_note_offs(midi_output: Option<&dyn MidiOutput>, channel: u8, notes: &[u8]) {
    if let Some(midi) = midi_output {
        for &note in notes {
            midi.note_off(channel, note);
        }
    }
}
