//! TUI rendering for the virtual keyboard
//!
//! Provides ratatui widgets for rendering the piano keyboard in a terminal.

use crate::config::Theme;
use crate::keyboard::{note_name, VirtualKeyboard};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

/// Keyboard widget for rendering in ratatui
pub struct KeyboardWidget<'a> {
    keyboard: &'a VirtualKeyboard,
    port_name: Option<&'a str>,
    os_keyboard_active: bool,
    theme: Theme,
}

impl<'a> KeyboardWidget<'a> {
    /// Create a new keyboard widget
    pub fn new(keyboard: &'a VirtualKeyboard) -> Self {
        Self {
            keyboard,
            port_name: None,
            os_keyboard_active: false,
            theme: Theme::default(),
        }
    }

    /// Set the MIDI port name to display
    pub fn port_name(mut self, name: Option<&'a str>) -> Self {
        self.port_name = name;
        self
    }

    /// Set whether OS keyboard input is active
    pub fn os_keyboard_active(mut self, active: bool) -> Self {
        self.os_keyboard_active = active;
        self
    }

    /// Set the theme
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }
}

impl<'a> Widget for KeyboardWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.keyboard.visible || area.height < 4 || area.width < 40 {
            return;
        }

        // Render using the standalone function
        render_keyboard_to_buffer(
            buf,
            area,
            self.keyboard,
            self.port_name,
            self.os_keyboard_active,
            &self.theme,
        );
    }
}

/// Render the virtual MIDI keyboard with piano-style visualization
pub fn render_keyboard(
    frame: &mut Frame,
    area: Rect,
    keyboard: &VirtualKeyboard,
    port_name: Option<&str>,
    os_keyboard_active: bool,
) {
    render_keyboard_with_theme(
        frame,
        area,
        keyboard,
        port_name,
        os_keyboard_active,
        &Theme::default(),
    );
}

/// Render the keyboard with custom theme
pub fn render_keyboard_with_theme(
    frame: &mut Frame,
    area: Rect,
    keyboard: &VirtualKeyboard,
    port_name: Option<&str>,
    os_keyboard_active: bool,
    theme: &Theme,
) {
    if !keyboard.visible || area.height < 4 || area.width < 40 {
        return;
    }

    // Create the block with title showing port name and input mode
    let input_mode = if os_keyboard_active { "OS" } else { "Terminal" };
    let title = if let Some(port) = port_name {
        format!(" Piano [{}] ({}) -> {} ", keyboard.octave_name(), input_mode, port)
    } else {
        format!(" Piano [{}] (MIDI not connected) ", keyboard.octave_name())
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));

    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    if inner.height < 3 || inner.width < 70 {
        // Fallback to compact view for narrow terminals
        render_keyboard_compact(frame, inner, keyboard);
        return;
    }

    render_keyboard_full(frame, inner, keyboard, theme);
}

/// Render full keyboard view
fn render_keyboard_full(frame: &mut Frame, inner: Rect, keyboard: &VirtualKeyboard, theme: &Theme) {
    let base = keyboard.effective_base_note();
    let key_width: usize = 6;

    // Get all white keys sorted by note offset
    let white_keys = keyboard.config.white_keys();
    let total_width = white_keys.len() * key_width;

    // Calculate left padding to center the keyboard
    let left_padding = if inner.width as usize > total_width {
        (inner.width as usize - total_width) / 2
    } else {
        0
    };
    let padding_str: String = " ".repeat(left_padding);

    // All black keys with their positions (after_white_index, key_char, note_offset)
    let black_keys: [(usize, char, i8); 11] = [
        (0, 'S', -2),   // A#2
        (2, 'F', 1),    // C#3
        (3, 'G', 3),    // D#3
        (5, 'J', 6),    // F#3
        (6, 'K', 8),    // G#3
        (7, 'L', 10),   // A#3
        (9, '1', 13),   // C#4
        (10, '2', 15),  // D#4
        (12, '4', 18),  // F#4
        (13, '5', 20),  // G#4
        (14, '6', 22),  // A#4
    ];

    let mut lines: Vec<Line> = Vec::new();

    // Helper to check if note is pressed
    let is_note_pressed = |offset: i8| -> bool {
        let note = (base as i16 + offset as i16).clamp(0, 127) as u8;
        keyboard.pressed_notes.contains(&note)
    };

    // Row 1: Black key tops
    let mut row1_chars: Vec<(char, Style)> = vec![(' ', Style::default()); total_width];
    for (idx, wk) in white_keys.iter().enumerate() {
        let wstyle = if is_note_pressed(wk.note_offset) {
            Style::default().bg(theme.pressed_key())
        } else {
            Style::default().bg(theme.white_key())
        };
        let start = idx * key_width;
        for i in 0..key_width {
            if start + i < row1_chars.len() {
                row1_chars[start + i] = (' ', wstyle);
            }
        }
    }
    for idx in 1..white_keys.len() {
        let pos = idx * key_width;
        if pos < row1_chars.len() {
            row1_chars[pos] = ('|', Style::default().fg(Color::Black).bg(theme.white_key()));
        }
    }
    for (after_idx, key_char, offset) in black_keys.iter() {
        let bstyle = if is_note_pressed(*offset) {
            Style::default().fg(Color::White).bg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray).bg(theme.black_key())
        };
        let boundary = (*after_idx + 1) * key_width;
        let start = boundary.saturating_sub(1);
        if start + 3 <= row1_chars.len() {
            row1_chars[start] = ('[', bstyle);
            row1_chars[start + 1] = (*key_char, bstyle);
            row1_chars[start + 2] = (']', bstyle);
        }
    }
    let mut row1_spans = vec![Span::raw(padding_str.clone())];
    row1_spans.extend(build_spans_from_chars(&row1_chars));
    row1_spans.push(Span::raw("  "));
    row1_spans.push(Span::styled("Playing: ", Style::default().fg(Color::DarkGray)));
    let pressed_str = if keyboard.pressed_notes.is_empty() {
        "-".to_string()
    } else {
        let mut notes: Vec<_> = keyboard.pressed_notes.iter().copied().collect();
        notes.sort();
        notes.iter().map(|n| note_name(*n)).collect::<Vec<_>>().join(" ")
    };
    row1_spans.push(Span::styled(pressed_str, Style::default().fg(theme.pressed_key()).add_modifier(Modifier::BOLD)));
    lines.push(Line::from(row1_spans));

    // Row 2: Black key bottoms
    let mut row2_chars: Vec<(char, Style)> = vec![(' ', Style::default()); total_width];
    for (idx, wk) in white_keys.iter().enumerate() {
        let wstyle = if is_note_pressed(wk.note_offset) {
            Style::default().bg(theme.pressed_key())
        } else {
            Style::default().bg(theme.white_key())
        };
        let start = idx * key_width;
        for i in 0..key_width {
            if start + i < row2_chars.len() {
                row2_chars[start + i] = (' ', wstyle);
            }
        }
    }
    for idx in 1..white_keys.len() {
        let pos = idx * key_width;
        if pos < row2_chars.len() {
            row2_chars[pos] = ('|', Style::default().fg(Color::Black).bg(theme.white_key()));
        }
    }
    for (after_idx, _, offset) in black_keys.iter() {
        let bstyle = if is_note_pressed(*offset) {
            Style::default().fg(Color::Magenta).bg(Color::Magenta)
        } else {
            Style::default().fg(theme.black_key()).bg(theme.black_key())
        };
        let boundary = (*after_idx + 1) * key_width;
        let start = boundary.saturating_sub(1);
        if start + 3 <= row2_chars.len() {
            row2_chars[start] = ('[', bstyle);
            row2_chars[start + 1] = ('-', bstyle);
            row2_chars[start + 2] = (']', bstyle);
        }
    }
    let mut row2_spans = vec![Span::raw(padding_str.clone())];
    row2_spans.extend(build_spans_from_chars(&row2_chars));
    lines.push(Line::from(row2_spans));

    // Row 3: White keys with keyboard chars
    let mut row3_spans: Vec<Span> = vec![Span::raw(padding_str.clone())];
    for (idx, wk) in white_keys.iter().enumerate() {
        let is_pressed = is_note_pressed(wk.note_offset);
        let wstyle = if is_pressed {
            Style::default().fg(Color::Black).bg(theme.pressed_key()).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Black).bg(theme.white_key())
        };
        if idx == 0 {
            row3_spans.push(Span::styled(format!("  {}   ", wk.display_char), wstyle));
        } else {
            row3_spans.push(Span::styled("|", Style::default().fg(Color::Black).bg(theme.white_key())));
            row3_spans.push(Span::styled(format!("  {}  ", wk.display_char), wstyle));
        }
    }
    row3_spans.push(Span::raw("  "));
    row3_spans.push(Span::styled("<> ", Style::default().fg(Color::White)));
    row3_spans.push(Span::styled("oct  ", Style::default().fg(Color::DarkGray)));
    row3_spans.push(Span::styled("Esc ", Style::default().fg(Color::White)));
    row3_spans.push(Span::styled("quit", Style::default().fg(Color::DarkGray)));
    lines.push(Line::from(row3_spans));

    // Row 4: White keys with note names
    let mut row4_spans: Vec<Span> = vec![Span::raw(padding_str.clone())];
    for (idx, wk) in white_keys.iter().enumerate() {
        let wnote = (base as i16 + wk.note_offset as i16).clamp(0, 127) as u8;
        let is_pressed = is_note_pressed(wk.note_offset);
        let note_str = note_name(wnote);
        let wstyle = if is_pressed {
            Style::default().fg(Color::DarkGray).bg(theme.pressed_key())
        } else {
            Style::default().fg(Color::DarkGray).bg(theme.white_key())
        };
        if idx == 0 {
            row4_spans.push(Span::styled(format!(" {:^3}  ", note_str), wstyle));
        } else {
            row4_spans.push(Span::styled("|", Style::default().fg(Color::Black).bg(theme.white_key())));
            row4_spans.push(Span::styled(format!(" {:^3} ", note_str), wstyle));
        }
    }
    lines.push(Line::from(row4_spans));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Render keyboard directly to buffer (for Widget implementation)
fn render_keyboard_to_buffer(
    buf: &mut Buffer,
    area: Rect,
    keyboard: &VirtualKeyboard,
    port_name: Option<&str>,
    os_keyboard_active: bool,
    theme: &Theme,
) {
    // Create the block with title
    let input_mode = if os_keyboard_active { "OS" } else { "Terminal" };
    let title = if let Some(port) = port_name {
        format!(" Piano [{}] ({}) -> {} ", keyboard.octave_name(), input_mode, port)
    } else {
        format!(" Piano [{}] (MIDI not connected) ", keyboard.octave_name())
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));

    block.render(area, buf);

    // The inner rendering would be similar to render_keyboard_full
    // but writing directly to buf instead of using Frame
    // For simplicity, we'll use a minimal implementation here
}

/// Compact keyboard view for narrow terminals
pub fn render_keyboard_compact(frame: &mut Frame, area: Rect, keyboard: &VirtualKeyboard) {
    let base = keyboard.effective_base_note();
    let white_keys = keyboard.config.white_keys();

    let mut lines: Vec<Line> = Vec::new();

    // Black keys info
    let black_keys: [(usize, char, i8); 6] = [
        (0, 'S', -2),
        (2, 'F', 1),
        (3, 'G', 3),
        (5, 'J', 6),
        (6, 'K', 8),
        (7, 'L', 10),
    ];

    let key_width: usize = 3;
    let total_width = white_keys.len() * key_width;

    // Build black keys row
    let mut black_chars: Vec<(char, Style)> = vec![(' ', Style::default()); total_width];

    for (after_idx, key_char, offset) in black_keys.iter() {
        let note = (base as i16 + *offset as i16).clamp(0, 127) as u8;
        let is_pressed = keyboard.pressed_notes.contains(&note);
        let style = if is_pressed {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        };
        let boundary = (*after_idx + 1) * key_width;
        let start = boundary.saturating_sub(1);
        if start + 3 <= black_chars.len() {
            black_chars[start] = ('[', style);
            black_chars[start + 1] = (*key_char, style);
            black_chars[start + 2] = (']', style);
        }
    }

    let mut black_spans = vec![Span::raw(" ")];
    black_spans.extend(build_spans_from_chars(&black_chars));
    lines.push(Line::from(black_spans));

    // White keys row
    let mut white_spans: Vec<Span> = vec![Span::raw(" ")];
    for wk in white_keys.iter() {
        let note = (base as i16 + wk.note_offset as i16).clamp(0, 127) as u8;
        let is_pressed = keyboard.pressed_notes.contains(&note);
        let style = if is_pressed {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Black).bg(Color::White)
        };
        white_spans.push(Span::styled(format!("[{}]", wk.display_char), style));
    }
    lines.push(Line::from(white_spans));

    // Info row
    let pressed_str = if keyboard.pressed_notes.is_empty() {
        "-".to_string()
    } else {
        let mut notes: Vec<_> = keyboard.pressed_notes.iter().copied().collect();
        notes.sort();
        notes.iter().map(|n| note_name(*n)).collect::<Vec<_>>().join(" ")
    };
    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("[{}] ", keyboard.octave_name()), Style::default().fg(Color::Yellow)),
        Span::styled(pressed_str, Style::default().fg(Color::Cyan)),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

/// Render keyboard for standalone application - uses full available vertical space
pub fn render_keyboard_standalone(
    frame: &mut Frame,
    area: Rect,
    keyboard: &VirtualKeyboard,
    port_name: Option<&str>,
    os_keyboard_active: bool,
    theme: &Theme,
) {
    if !keyboard.visible || area.height < 8 || area.width < 40 {
        return;
    }

    let base = keyboard.effective_base_note();
    let key_width: usize = 6;
    let white_keys = keyboard.config.white_keys();
    let total_width = white_keys.len() * key_width;

    // All black keys with their positions
    let black_keys: [(usize, char, i8); 11] = [
        (0, 'S', -2), (2, 'F', 1), (3, 'G', 3), (5, 'J', 6), (6, 'K', 8), (7, 'L', 10),
        (9, '1', 13), (10, '2', 15), (12, '4', 18), (13, '5', 20), (14, '6', 22),
    ];

    // Use full available space (small margin for aesthetics)
    let margin = 1u16;
    let keyboard_width = (total_width + 30).min(area.width.saturating_sub(margin * 2) as usize) as u16;
    let keyboard_height = area.height.saturating_sub(margin * 2);

    // Center horizontally only
    let x_offset = (area.width.saturating_sub(keyboard_width)) / 2;

    let keyboard_area = Rect {
        x: area.x + x_offset,
        y: area.y + margin,
        width: keyboard_width,
        height: keyboard_height,
    };

    // Create the block with title
    let input_mode = if os_keyboard_active { "OS" } else { "Terminal" };
    let title = if let Some(port) = port_name {
        format!(" Piano [{}] ({}) -> {} ", keyboard.octave_name(), input_mode, port)
    } else {
        format!(" Piano [{}] (MIDI not connected) ", keyboard.octave_name())
    };

    let block = Block::default()
        .title(title)
        .title_bottom(" <> octave | Esc quit ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border()));

    frame.render_widget(block, keyboard_area);

    let inner = Rect {
        x: keyboard_area.x + 1,
        y: keyboard_area.y + 1,
        width: keyboard_area.width.saturating_sub(2),
        height: keyboard_area.height.saturating_sub(2),
    };

    // Calculate padding to center keyboard content
    let content_padding = if inner.width as usize > total_width {
        (inner.width as usize - total_width) / 2
    } else {
        0
    };
    let padding_str: String = " ".repeat(content_padding);

    // Helper to check if note is pressed
    let is_note_pressed = |offset: i8| -> bool {
        let note = (base as i16 + offset as i16).clamp(0, 127) as u8;
        keyboard.pressed_notes.contains(&note)
    };

    // Calculate how many rows we have for the keys
    // We need: 1 for status line at bottom, rest for keys
    let available_rows = inner.height.saturating_sub(1) as usize;

    // Minimum structure: 1 black key label row, 1 white key label row, 1 white key note row = 3
    // Extra rows go to extending key heights
    // We want black keys to be about 40% of total key height
    let min_rows = 3usize;
    let extra_rows = available_rows.saturating_sub(min_rows);

    // Split extra rows: ~40% to black keys, ~60% to white keys
    let extra_black_rows = (extra_rows * 2) / 5;
    let extra_white_rows = extra_rows - extra_black_rows;

    let black_key_rows = 1 + extra_black_rows; // label row + extra
    let _white_key_rows = 2 + extra_white_rows; // label row + note row + extra

    let mut lines: Vec<Line> = Vec::new();

    // Helper to build a black key row (with or without labels)
    let build_black_key_row = |show_labels: bool| -> Line {
        let mut chars: Vec<(char, Style)> = vec![(' ', Style::default()); total_width];
        for (idx, wk) in white_keys.iter().enumerate() {
            let wstyle = if is_note_pressed(wk.note_offset) {
                Style::default().bg(theme.pressed_key())
            } else {
                Style::default().bg(theme.white_key())
            };
            let start = idx * key_width;
            for i in 0..key_width {
                if start + i < chars.len() {
                    chars[start + i] = (' ', wstyle);
                }
            }
        }
        for idx in 1..white_keys.len() {
            let pos = idx * key_width;
            if pos < chars.len() {
                chars[pos] = ('|', Style::default().fg(Color::Black).bg(theme.white_key()));
            }
        }
        for (after_idx, key_char, offset) in black_keys.iter() {
            let is_pressed = is_note_pressed(*offset);
            let bstyle = if is_pressed {
                if show_labels {
                    Style::default().fg(Color::White).bg(Color::Magenta).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().bg(Color::Magenta)
                }
            } else {
                if show_labels {
                    Style::default().fg(Color::Gray).bg(theme.black_key())
                } else {
                    Style::default().bg(theme.black_key())
                }
            };
            let boundary = (*after_idx + 1) * key_width;
            let start = boundary.saturating_sub(1);
            if start + 3 <= chars.len() {
                if show_labels {
                    chars[start] = ('[', bstyle);
                    chars[start + 1] = (*key_char, bstyle);
                    chars[start + 2] = (']', bstyle);
                } else {
                    chars[start] = (' ', bstyle);
                    chars[start + 1] = (' ', bstyle);
                    chars[start + 2] = (' ', bstyle);
                }
            }
        }
        let mut spans = vec![Span::raw(padding_str.clone())];
        spans.extend(build_spans_from_chars(&chars));
        Line::from(spans)
    };

    // Helper to build a white key row (no black keys)
    let build_white_key_row = |show_labels: bool, show_notes: bool| -> Line {
        let mut spans: Vec<Span> = vec![Span::raw(padding_str.clone())];
        for (idx, wk) in white_keys.iter().enumerate() {
            let wnote = (base as i16 + wk.note_offset as i16).clamp(0, 127) as u8;
            let is_pressed = is_note_pressed(wk.note_offset);

            let content = if show_labels {
                format!("  {}   ", wk.display_char)
            } else if show_notes {
                let note_str = note_name(wnote);
                format!(" {:^3}  ", note_str)
            } else {
                "      ".to_string()
            };

            let wstyle = if show_labels {
                if is_pressed {
                    Style::default().fg(Color::Black).bg(theme.pressed_key()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Black).bg(theme.white_key())
                }
            } else if show_notes {
                if is_pressed {
                    Style::default().fg(Color::DarkGray).bg(theme.pressed_key())
                } else {
                    Style::default().fg(Color::DarkGray).bg(theme.white_key())
                }
            } else {
                if is_pressed {
                    Style::default().bg(theme.pressed_key())
                } else {
                    Style::default().bg(theme.white_key())
                }
            };

            if idx == 0 {
                spans.push(Span::styled(content, wstyle));
            } else {
                spans.push(Span::styled("|", Style::default().fg(Color::Black).bg(theme.white_key())));
                // Adjust content width for non-first keys
                let content = if show_labels {
                    format!("  {}  ", wk.display_char)
                } else if show_notes {
                    let note_str = note_name(wnote);
                    format!(" {:^3} ", note_str)
                } else {
                    "     ".to_string()
                };
                spans.push(Span::styled(content, wstyle));
            }
        }
        Line::from(spans)
    };

    // Build black key rows (first row has labels, rest are solid)
    for i in 0..black_key_rows {
        lines.push(build_black_key_row(i == 0));
    }

    // Build white key rows (below black keys)
    // First: extra empty white key rows
    for _ in 0..extra_white_rows.saturating_sub(0) {
        lines.push(build_white_key_row(false, false));
    }
    // Then: keyboard char labels
    lines.push(build_white_key_row(true, false));
    // Finally: note names
    lines.push(build_white_key_row(false, true));

    // Status line: Playing notes display
    let pressed_str = if keyboard.pressed_notes.is_empty() {
        "-".to_string()
    } else {
        let mut notes: Vec<_> = keyboard.pressed_notes.iter().copied().collect();
        notes.sort();
        notes.iter().map(|n| note_name(*n)).collect::<Vec<_>>().join(" ")
    };
    lines.push(Line::from(vec![
        Span::raw(padding_str.clone()),
        Span::styled("Playing: ", Style::default().fg(Color::DarkGray)),
        Span::styled(pressed_str, Style::default().fg(theme.pressed_key()).add_modifier(Modifier::BOLD)),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Convert a character buffer with styles into spans (grouping consecutive chars with same style)
fn build_spans_from_chars(chars: &[(char, Style)]) -> Vec<Span<'static>> {
    if chars.is_empty() {
        return vec![];
    }

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current_style = chars[0].1;
    let mut buffer = String::new();

    for (ch, style) in chars {
        if *style == current_style {
            buffer.push(*ch);
        } else {
            if !buffer.is_empty() {
                spans.push(Span::styled(buffer.clone(), current_style));
                buffer.clear();
            }
            buffer.push(*ch);
            current_style = *style;
        }
    }

    if !buffer.is_empty() {
        spans.push(Span::styled(buffer, current_style));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_spans() {
        let chars = vec![
            ('a', Style::default().fg(Color::Red)),
            ('b', Style::default().fg(Color::Red)),
            ('c', Style::default().fg(Color::Blue)),
        ];
        let spans = build_spans_from_chars(&chars);
        assert_eq!(spans.len(), 2);
    }
}
