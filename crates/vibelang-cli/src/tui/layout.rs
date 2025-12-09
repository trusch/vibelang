//! Layout management for adaptive terminal sizing

use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Size category for the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeCategory {
    /// Less than 30 lines
    Small,
    /// 30-50 lines
    Medium,
    /// More than 50 lines
    Large,
}

impl SizeCategory {
    pub fn from_height(height: u16) -> Self {
        if height < 30 {
            SizeCategory::Small
        } else if height < 50 {
            SizeCategory::Medium
        } else {
            SizeCategory::Large
        }
    }
}

/// Simplified layout slots - header, main (full width), log, footer, optional keyboard
pub struct LayoutSlots {
    pub size: SizeCategory,
    pub header: Rect,
    pub main: Rect,
    pub log: Rect,
    pub footer: Rect,
    pub keyboard: Option<Rect>,
}

/// Create the main layout - simplified to just header/main/log/footer
pub fn create_layout(area: Rect) -> LayoutSlots {
    create_layout_with_keyboard(area, false)
}

/// Create the main layout with optional keyboard area
pub fn create_layout_with_keyboard(area: Rect, show_keyboard: bool) -> LayoutSlots {
    let size = SizeCategory::from_height(area.height);

    // Header height based on terminal size (includes progress bar)
    let header_height = match size {
        SizeCategory::Small => 6,
        SizeCategory::Medium => 7,
        SizeCategory::Large => 7,
    };

    // Log height
    let log_height = match size {
        SizeCategory::Small => 4,
        SizeCategory::Medium => 5,
        SizeCategory::Large => 6,
    };

    // Keyboard height (when visible) - 6 for extended keyboard
    let keyboard_height = if show_keyboard { 6 } else { 0 };

    let constraints = if show_keyboard {
        vec![
            Constraint::Length(header_height),   // Header with stats
            Constraint::Min(6),                  // Main hierarchy (takes remaining)
            Constraint::Length(log_height),      // Log
            Constraint::Length(keyboard_height), // Keyboard
            Constraint::Length(3),               // Footer (needs 3 for borders + content)
        ]
    } else {
        vec![
            Constraint::Length(header_height),   // Header with stats
            Constraint::Min(8),                  // Main hierarchy (takes all remaining)
            Constraint::Length(log_height),      // Log
            Constraint::Length(3),               // Footer (needs 3 for borders + content)
        ]
    };

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    if show_keyboard {
        LayoutSlots {
            size,
            header: sections[0],
            main: sections[1],
            log: sections[2],
            keyboard: Some(sections[3]),
            footer: sections[4],
        }
    } else {
        LayoutSlots {
            size,
            header: sections[0],
            main: sections[1],
            log: sections[2],
            keyboard: None,
            footer: sections[3],
        }
    }
}

/// Truncate a string to fit within a given width
pub fn truncate_string(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width <= 3 {
        s.chars().take(max_width).collect()
    } else {
        let mut result: String = s.chars().take(max_width - 3).collect();
        result.push_str("...");
        result
    }
}
