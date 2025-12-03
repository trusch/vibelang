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

/// Simplified layout slots - header, main (full width), log, footer
pub struct LayoutSlots {
    pub size: SizeCategory,
    pub header: Rect,
    pub main: Rect,
    pub log: Rect,
    pub footer: Rect,
}

/// Create the main layout - simplified to just header/main/log/footer
pub fn create_layout(area: Rect) -> LayoutSlots {
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

    let constraints = vec![
        Constraint::Length(header_height),   // Header with stats
        Constraint::Min(8),                   // Main hierarchy (takes all remaining)
        Constraint::Length(log_height),       // Log
        Constraint::Length(3),                // Footer (needs 3 for borders + content)
    ];

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    LayoutSlots {
        size,
        header: sections[0],
        main: sections[1],
        log: sections[2],
        footer: sections[3],
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
