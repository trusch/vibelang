//! Configuration file support for vibe-keys
//!
//! Configuration is stored in TOML format at:
//! - Linux: `~/.config/vibe-keys/config.toml`
//! - macOS: `~/Library/Application Support/vibe-keys/config.toml`
//! - Windows: `%APPDATA%\vibe-keys\config.toml`

use crate::error::{Error, Result};
use crate::keyboard::{KeyboardConfig, KeyMapping, C3_MIDI, DEFAULT_NOTE_RELEASE_MS, DEFAULT_VELOCITY};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Keyboard configuration
    pub keyboard: KeyboardSettings,
    /// MIDI configuration
    pub midi: MidiSettings,
    /// UI/Theme configuration
    pub theme: Theme,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keyboard: KeyboardSettings::default(),
            midi: MidiSettings::default(),
            theme: Theme::default(),
        }
    }
}

impl Config {
    /// Load configuration from the default config file location
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Err(Error::Config(format!("Config file not found at {:?}", path)))
        }
    }

    /// Load configuration or return default if not found
    pub fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    /// Save configuration to the default config file location
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    /// Get the default configuration file path
    pub fn config_path() -> Result<PathBuf> {
        if let Some(proj_dirs) = ProjectDirs::from("", "", "vibe-keys") {
            Ok(proj_dirs.config_dir().join("config.toml"))
        } else {
            Err(Error::Config("Could not determine config directory".to_string()))
        }
    }

    /// Create a default config file with comments
    pub fn create_default_config_file() -> Result<PathBuf> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = r#"# vibe-keys configuration file
# https://github.com/trusch/vibelang

[keyboard]
# Keyboard layout: "german", "us", or "custom"
layout = "german"

# Base MIDI note (48 = C3)
base_note = 48

# Default velocity (1-127)
velocity = 100

# MIDI channel (0-15)
channel = 0

# Auto-release timeout in milliseconds
# Notes are released after this time if no key-up event is detected
note_release_ms = 400

[midi]
# JACK client name
client_name = "vibe-keys"

# MIDI output port name
port_name = "midi_out"

# Auto-connect to these JACK MIDI inputs (optional)
# auto_connect = ["a2j:Hydrogen"]

[theme]
# Colors for the keyboard display
white_key_color = "white"
black_key_color = "dark_gray"
pressed_key_color = "cyan"
border_color = "cyan"

# Show note names on keys
show_note_names = true

# Show keyboard shortcuts help
show_help = true
"#;

        fs::write(&path, content)?;
        Ok(path)
    }

    /// Convert to KeyboardConfig for the keyboard module
    pub fn to_keyboard_config(&self) -> KeyboardConfig {
        let base_config = match self.keyboard.layout {
            KeyboardLayout::German => KeyboardConfig::german_layout(),
            KeyboardLayout::Us => KeyboardConfig::us_layout(),
            KeyboardLayout::Custom => {
                // Use custom mappings if provided, otherwise default to German
                if let Some(ref mappings) = self.keyboard.custom_mappings {
                    KeyboardConfig {
                        mappings: mappings.iter().map(|m| m.to_key_mapping()).collect(),
                        base_note: self.keyboard.base_note,
                        velocity: self.keyboard.velocity,
                        channel: self.keyboard.channel,
                        note_release_duration: Duration::from_millis(self.keyboard.note_release_ms),
                    }
                } else {
                    KeyboardConfig::german_layout()
                }
            }
        };

        KeyboardConfig {
            mappings: base_config.mappings,
            base_note: self.keyboard.base_note,
            velocity: self.keyboard.velocity,
            channel: self.keyboard.channel,
            note_release_duration: Duration::from_millis(self.keyboard.note_release_ms),
        }
    }
}

/// Keyboard layout preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyboardLayout {
    /// German QWERTZ layout
    German,
    /// US QWERTY layout
    Us,
    /// Custom layout (use custom_mappings)
    Custom,
}

impl Default for KeyboardLayout {
    fn default() -> Self {
        Self::German
    }
}

/// Keyboard settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardSettings {
    /// Keyboard layout preset
    pub layout: KeyboardLayout,
    /// Base MIDI note (48 = C3)
    pub base_note: u8,
    /// Default velocity (1-127)
    pub velocity: u8,
    /// MIDI channel (0-15)
    pub channel: u8,
    /// Auto-release timeout in milliseconds
    pub note_release_ms: u64,
    /// Custom key mappings (only used when layout = "custom")
    pub custom_mappings: Option<Vec<CustomKeyMapping>>,
}

impl Default for KeyboardSettings {
    fn default() -> Self {
        Self {
            layout: KeyboardLayout::German,
            base_note: C3_MIDI,
            velocity: DEFAULT_VELOCITY,
            channel: 0,
            note_release_ms: DEFAULT_NOTE_RELEASE_MS,
            custom_mappings: None,
        }
    }
}

/// Custom key mapping for TOML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomKeyMapping {
    /// The key character
    pub key: char,
    /// Display character (optional, defaults to uppercase of key)
    pub display: Option<char>,
    /// MIDI note offset from base note
    pub offset: i8,
    /// Whether this is a black key
    pub black: bool,
}

impl CustomKeyMapping {
    /// Convert to KeyMapping
    pub fn to_key_mapping(&self) -> KeyMapping {
        KeyMapping {
            key_char: self.key.to_ascii_lowercase(),
            display_char: self.display.unwrap_or_else(|| self.key.to_ascii_uppercase()),
            note_offset: self.offset,
            is_black_key: self.black,
        }
    }
}

/// MIDI settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MidiSettings {
    /// JACK client name
    pub client_name: String,
    /// MIDI output port name
    pub port_name: String,
    /// Auto-connect to these JACK MIDI inputs
    pub auto_connect: Option<Vec<String>>,
}

impl Default for MidiSettings {
    fn default() -> Self {
        Self {
            client_name: "vibe-keys".to_string(),
            port_name: "midi_out".to_string(),
            auto_connect: None,
        }
    }
}

/// Theme/UI settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    /// White key color
    pub white_key_color: String,
    /// Black key color
    pub black_key_color: String,
    /// Pressed key color
    pub pressed_key_color: String,
    /// Border color
    pub border_color: String,
    /// Show note names on keys
    pub show_note_names: bool,
    /// Show help text
    pub show_help: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            white_key_color: "white".to_string(),
            black_key_color: "dark_gray".to_string(),
            pressed_key_color: "cyan".to_string(),
            border_color: "cyan".to_string(),
            show_note_names: true,
            show_help: true,
        }
    }
}

impl Theme {
    /// Parse a color string to ratatui Color
    pub fn parse_color(s: &str) -> ratatui::style::Color {
        use ratatui::style::Color;
        match s.to_lowercase().as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "gray" | "grey" => Color::Gray,
            "dark_gray" | "dark_grey" | "darkgray" | "darkgrey" => Color::DarkGray,
            "light_red" | "lightred" => Color::LightRed,
            "light_green" | "lightgreen" => Color::LightGreen,
            "light_yellow" | "lightyellow" => Color::LightYellow,
            "light_blue" | "lightblue" => Color::LightBlue,
            "light_magenta" | "lightmagenta" => Color::LightMagenta,
            "light_cyan" | "lightcyan" => Color::LightCyan,
            "white" => Color::White,
            // Try parsing as RGB hex
            s if s.starts_with('#') && s.len() == 7 => {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&s[1..3], 16),
                    u8::from_str_radix(&s[3..5], 16),
                    u8::from_str_radix(&s[5..7], 16),
                ) {
                    Color::Rgb(r, g, b)
                } else {
                    Color::White
                }
            }
            _ => Color::White,
        }
    }

    /// Get white key color
    pub fn white_key(&self) -> ratatui::style::Color {
        Self::parse_color(&self.white_key_color)
    }

    /// Get black key color
    pub fn black_key(&self) -> ratatui::style::Color {
        Self::parse_color(&self.black_key_color)
    }

    /// Get pressed key color
    pub fn pressed_key(&self) -> ratatui::style::Color {
        Self::parse_color(&self.pressed_key_color)
    }

    /// Get border color
    pub fn border(&self) -> ratatui::style::Color {
        Self::parse_color(&self.border_color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.keyboard.base_note, 48);
        assert_eq!(config.keyboard.velocity, 100);
        assert_eq!(config.keyboard.layout, KeyboardLayout::German);
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.keyboard.base_note, config.keyboard.base_note);
    }

    #[test]
    fn test_color_parsing() {
        use ratatui::style::Color;
        assert_eq!(Theme::parse_color("cyan"), Color::Cyan);
        assert_eq!(Theme::parse_color("white"), Color::White);
        assert_eq!(Theme::parse_color("#ff0000"), Color::Rgb(255, 0, 0));
    }
}
