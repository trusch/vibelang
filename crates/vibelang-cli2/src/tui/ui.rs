//! UI rendering logic for the TUI

use crate::tui::app::{
    BeatInfo, HierarchyEntry, HierarchyKind, LogEntry, PanelFocus, ResourceStats,
    SummaryStats, TuiApp,
};
use crate::tui::keyboard::{note_name, VirtualKeyboard};
use crate::tui::layout::{create_layout_with_keyboard, truncate_string};
use log::Level;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

/// Render the entire UI
pub fn render_ui(frame: &mut Frame, app: &mut TuiApp) {
    let area = frame.area();
    let show_keyboard = app.virtual_keyboard.visible;
    let layout = create_layout_with_keyboard(area, show_keyboard);

    // Update page size based on visible area
    app.page_size = layout.main.height.saturating_sub(2) as usize;

    // Handle modals first (they overlay everything)
    if app.show_help_modal {
        render_help_modal(frame, area);
        return;
    }

    if app.show_error_modal {
        render_error_modal(frame, app, area);
        return;
    }

    let hierarchy = if app.search_query.is_empty() {
        app.hierarchy_entries()
    } else {
        app.filtered_hierarchy_entries()
    };
    let summary_stats = app.summary_stats();
    let resource_stats = app.resource_stats();
    let beat_info = app.get_beat_info();

    // Render header with all stats
    render_header(
        frame,
        layout.header,
        &beat_info,
        &summary_stats,
        &resource_stats,
        app.timeline_offset_beats,
        app.vu_level,
    );

    // Handle maximized log view
    if app.log_maximized {
        render_log_maximized(frame, layout.main, app, app.focused_panel == PanelFocus::Log);
        let focused = app.focused_panel == PanelFocus::Hierarchy;
        render_unified_hierarchy(
            frame,
            layout.log,
            &mut app.hierarchy_list_state,
            &hierarchy,
            focused,
            &app.search_query,
            &app.flash_items,
        );
    } else {
        let focused = app.focused_panel == PanelFocus::Hierarchy;
        render_unified_hierarchy(
            frame,
            layout.main,
            &mut app.hierarchy_list_state,
            &hierarchy,
            focused,
            &app.search_query,
            &app.flash_items,
        );

        render_log(frame, layout.log, app, app.focused_panel == PanelFocus::Log);
    }

    // Render keyboard if visible
    if let Some(keyboard_area) = layout.keyboard {
        render_keyboard(frame, keyboard_area, &app.virtual_keyboard, app.keyboard_port_name.as_deref(), app.os_keyboard_active);
    }

    // Render footer
    render_footer(frame, layout.footer, app);

    // Render search bar overlay if in search mode
    if app.search_mode || app.log_search_mode {
        render_search_bar(frame, area, app);
    }
}

/// Render info-dense header with all stats
fn render_header(
    frame: &mut Frame,
    area: Rect,
    beat_info: &BeatInfo,
    summary: &SummaryStats,
    resources: &ResourceStats,
    time_offset: f64,
    vu_level: f32,
) {
    let status_color = if beat_info.running {
        Color::Green
    } else {
        Color::Yellow
    };
    let status_icon = if beat_info.running { "\u{25B6}" } else { "\u{23F8}" }; // ▶ ⏸

    // Calculate progress bar width
    let bar_width = area.width.saturating_sub(12) as usize;
    let progress_bar = beat_progress_bar_unicode(
        beat_info.beat_in_bar,
        beat_info.total_beats_in_bar,
        bar_width,
    );

    let header_lines = vec![
        // Line 1: Transport status, Bar, Beat, BPM, Time signature
        Line::from(vec![
            Span::styled(
                format!(" {} ", status_icon),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "VIBELANG",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  \u{2502}  Bar "), // │
            Span::styled(
                format!("{}", beat_info.bar_number),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Beat "),
            Span::styled(
                format!("{}/{}", beat_info.beat_number_in_bar, beat_info.total_beats_in_bar),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  \u{2502}  "),
            Span::styled(
                format!("{:.1} BPM", beat_info.bpm),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled(
                beat_info.time_signature.to_string(),
                Style::default().fg(Color::Cyan),
            ),
        ]),
        // Line 2: Full-width progress bar
        Line::from(vec![
            Span::styled(" Bar ", Style::default().fg(Color::DarkGray)),
            Span::styled(progress_bar, Style::default().fg(Color::Cyan)),
        ]),
        // Line 3: Patterns, Melodies, Sequences counts
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Patterns", Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(
                format!("{}\u{25B6}", summary.patterns_playing),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("/{}", summary.patterns_total),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("  \u{2502}  "),
            Span::styled("Melodies", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(
                format!("{}\u{25B6}", summary.melodies_playing),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("/{}", summary.melodies_total),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("  \u{2502}  "),
            Span::styled("Sequences", Style::default().fg(Color::LightCyan)),
            Span::raw(" "),
            Span::styled(
                format!("{}\u{25B6}", summary.sequences_playing),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("/{}", summary.sequences_total),
                Style::default().fg(Color::DarkGray),
            ),
        ]),
        // Line 4: Resources
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Groups", Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.groups),
                Style::default().fg(Color::White),
            ),
            Span::raw("  \u{2502}  "),
            Span::styled("Voices", Style::default().fg(Color::LightBlue)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.voices),
                Style::default().fg(Color::White),
            ),
            Span::raw("  \u{2502}  "),
            Span::styled("Effects", Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.effects),
                Style::default().fg(Color::White),
            ),
            Span::raw("  \u{2502}  "),
            Span::styled("Samples", Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.samples),
                Style::default().fg(Color::White),
            ),
            Span::raw("  \u{2502}  "),
            Span::styled("Bufs", Style::default().fg(Color::Blue)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.buffers_allocated),
                Style::default().fg(Color::White),
            ),
        ]),
        // Line 5: VU meter and time offset
        Line::from(vec![
            Span::raw(" "),
            Span::styled("VU ", Style::default().fg(Color::Green)),
            Span::styled(
                vu_meter_bar(vu_level, 12),
                Style::default().fg(vu_meter_color(vu_level)),
            ),
            if time_offset.abs() > 0.01 {
                Span::raw("  \u{2502}  ")
            } else {
                Span::raw("")
            },
            if time_offset.abs() > 0.01 {
                Span::styled(
                    format!("View offset {:+.1}b", time_offset),
                    Style::default().fg(Color::Yellow),
                )
            } else {
                Span::raw("")
            },
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let paragraph = Paragraph::new(header_lines).block(block);
    frame.render_widget(paragraph, area);
}

/// Render unified hierarchy view
fn render_unified_hierarchy(
    frame: &mut Frame,
    area: Rect,
    list_state: &mut ListState,
    hierarchy: &[HierarchyEntry],
    focused: bool,
    search_query: &str,
    flash_items: &std::collections::HashSet<String>,
) {
    let title = if !search_query.is_empty() {
        format!(" Hierarchy \u{2022} search: {} ", search_query)
    } else {
        " Hierarchy ".to_string()
    };

    let selection = list_state.selected().unwrap_or(0);

    let border_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(title.as_str())
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    let max_width = inner.width.saturating_sub(2) as usize;

    let mut items: Vec<ListItem> = Vec::new();

    for (idx, entry) in hierarchy.iter().enumerate() {
        let should_flash = flash_items.contains(&entry.id);
        let item = render_hierarchy_entry(entry, idx, selection, focused, max_width, should_flash);
        items.push(item);
    }

    if items.is_empty() {
        let paragraph = Paragraph::new("No groups or sequences defined")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let list = List::new(items)
        .highlight_style(Style::default());
    frame.render_stateful_widget(list, inner, list_state);
}

/// Fixed column widths for aligned display
const COL_NAME: usize = 16;
const COL_DETAIL: usize = 18;

/// Render a single hierarchy entry with aligned columns
fn render_hierarchy_entry(
    entry: &HierarchyEntry,
    idx: usize,
    selection: usize,
    focused: bool,
    max_width: usize,
    should_flash: bool,
) -> ListItem<'static> {
    let indent = "  ".repeat(entry.depth);
    let indent_len = indent.len();

    // Collapse/expand marker for collapsible items
    let collapse_marker = if entry.collapsible {
        if entry.collapsed { "\u{25B8}" } else { "\u{25BE}" } // ▸ ▾
    } else {
        " "
    };

    let type_marker = match entry.kind {
        HierarchyKind::Group => "\u{25CF}", // ●
        HierarchyKind::Voice => "\u{266A}", // ♪
        HierarchyKind::Pattern => if entry.active { "\u{25C6}" } else { "\u{25C7}" }, // ◆ ◇
        HierarchyKind::Melody => if entry.active { "\u{25C6}" } else { "\u{25C7}" }, // ◆ ◇
        HierarchyKind::Effect => "\u{25C8}", // ◈
        HierarchyKind::Sequence => if entry.active { "\u{25B6}" } else { "\u{25B7}" }, // ▶ ▷
        HierarchyKind::Section => "\u{2501}", // ━
    };

    // Build params string
    let params_str = if !entry.params.is_empty() {
        let param_list: Vec<String> = entry
            .params
            .iter()
            .take(6)
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect();
        param_list.join("  ")
    } else {
        String::new()
    };

    // Calculate available space after indent
    let available = max_width.saturating_sub(indent_len + 3);

    // Adjust column widths based on available space
    let name_col = COL_NAME.min(available / 3);
    let detail_col = COL_DETAIL.min(available / 3);
    let params_col = available.saturating_sub(name_col + detail_col + 2);

    let name = truncate_string(&entry.label, name_col);
    let detail = truncate_string(&entry.detail, detail_col);
    let params = truncate_string(&params_str, params_col);

    let marker_color = if entry.active {
        entry.color()
    } else {
        Color::DarkGray
    };

    let (name_color, detail_color) = if entry.active {
        match entry.kind {
            HierarchyKind::Pattern | HierarchyKind::Melody => (Color::White, Color::Green),
            _ => (entry.color(), Color::DarkGray),
        }
    } else {
        (entry.color(), Color::DarkGray)
    };

    // Build the line with proper formatting
    let mut spans = vec![
        Span::raw(indent),
        Span::styled(
            collapse_marker,
            Style::default().fg(if entry.collapsible { Color::White } else { Color::DarkGray }),
        ),
        Span::styled(type_marker, Style::default().fg(marker_color)),
        Span::raw(" "),
        Span::styled(
            format!("{:<width$}", name, width = name_col),
            Style::default()
                .fg(name_color)
                .add_modifier(if entry.active { Modifier::BOLD } else { Modifier::empty() }),
        ),
    ];

    if !detail.is_empty() || !params.is_empty() {
        spans.push(Span::styled(" \u{2502} ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(
            format!("{:<width$}", detail, width = detail_col),
            Style::default().fg(detail_color),
        ));
    }

    if !params.is_empty() {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(params, Style::default().fg(Color::DarkGray)));
    }

    let line = Line::from(spans);

    // Apply selection styling
    let style = if idx == selection && focused {
        Style::default().bg(Color::DarkGray)
    } else if should_flash {
        Style::default().bg(Color::Rgb(50, 50, 80))
    } else {
        Style::default()
    };

    ListItem::new(line).style(style)
}

/// Render log panel
fn render_log(frame: &mut Frame, area: Rect, app: &TuiApp, focused: bool) {
    let border_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let title = format!(
        " Log [{}] ",
        match app.min_log_level {
            Level::Error => "ERR",
            Level::Warn => "WRN",
            Level::Info => "INF",
            Level::Debug => "DBG",
            Level::Trace => "TRC",
        }
    );

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let filtered = app.filtered_log_entries();
    let items: Vec<ListItem> = filtered
        .iter()
        .map(|entry| {
            let level_color = match entry.level {
                Level::Error => Color::Red,
                Level::Warn => Color::Yellow,
                Level::Info => Color::Green,
                Level::Debug => Color::Blue,
                Level::Trace => Color::DarkGray,
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("[{:5}] ", entry.level),
                    Style::default().fg(level_color),
                ),
                Span::raw(&entry.message),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render maximized log panel
fn render_log_maximized(frame: &mut Frame, area: Rect, app: &TuiApp, focused: bool) {
    let border_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(" Log (maximized) ")
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let filtered = app.filtered_log_entries();
    let items: Vec<ListItem> = filtered
        .iter()
        .map(|entry| {
            let level_color = match entry.level {
                Level::Error => Color::Red,
                Level::Warn => Color::Yellow,
                Level::Info => Color::Green,
                Level::Debug => Color::Blue,
                Level::Trace => Color::DarkGray,
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("[{:5}] ", entry.level),
                    Style::default().fg(level_color),
                ),
                Span::raw(&entry.message),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, inner);
}

/// Render virtual keyboard
fn render_keyboard(
    frame: &mut Frame,
    area: Rect,
    keyboard: &VirtualKeyboard,
    port_name: Option<&str>,
    os_active: bool,
) {
    let port_status = if let Some(name) = port_name {
        format!(" \u{2192} {} ", name)
    } else {
        " (no JACK) ".to_string()
    };

    let os_indicator = if os_active { " [OS]" } else { "" };

    let block = Block::default()
        .title(format!(
            " Keyboard {} vel:{} oct:{}{}{}",
            port_status,
            keyboard.velocity(),
            keyboard.octave_name(),
            os_indicator,
            if keyboard.visible { "" } else { " (hidden)" }
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 || inner.width < 20 {
        return;
    }

    // Render two octaves of piano keys
    let lower_white = keyboard.config.lower_white_keys();
    let lower_black = keyboard.config.lower_black_keys();
    let upper_white = keyboard.config.upper_white_keys();
    let upper_black = keyboard.config.upper_black_keys();

    // Calculate key width based on available space
    let total_white_keys = lower_white.len() + upper_white.len();
    let key_width = (inner.width as usize / total_white_keys).max(3);

    // Build display lines
    let mut lines = Vec::new();

    // Upper black keys line
    let mut upper_black_line = String::new();
    let mut black_idx = 0;
    for (i, white_key) in upper_white.iter().enumerate() {
        // Check if there's a black key between this and the next white key
        let has_black = black_idx < upper_black.len()
            && upper_black[black_idx].note_offset < white_key.note_offset + 2
            && (i == 0 || upper_black[black_idx].note_offset > upper_white[i - 1].note_offset);

        if has_black && i > 0 {
            let is_pressed = keyboard.is_key_pressed(upper_black[black_idx]);
            let display = format!("{:^w$}", upper_black[black_idx].display_char, w = key_width - 1);
            if is_pressed {
                upper_black_line.push_str(&format!("\x1b[7m{}\x1b[0m ", display));
            } else {
                upper_black_line.push_str(&format!("{} ", display));
            }
            black_idx += 1;
        } else {
            upper_black_line.push_str(&" ".repeat(key_width));
        }
    }
    lines.push(Line::from(Span::raw(format!("  {}", upper_black_line))));

    // Upper white keys line
    let mut upper_white_line = Vec::new();
    for key in &upper_white {
        let is_pressed = keyboard.is_key_pressed(key);
        let style = if is_pressed {
            Style::default().bg(Color::Green).fg(Color::Black)
        } else {
            Style::default().fg(Color::White)
        };
        let display = format!("{:^w$}", key.display_char, w = key_width);
        upper_white_line.push(Span::styled(display, style));
    }
    lines.push(Line::from(vec![Span::raw("  ")].into_iter().chain(upper_white_line).collect::<Vec<_>>()));

    // Lower black keys line
    let mut lower_black_line = String::new();
    black_idx = 0;
    for (i, white_key) in lower_white.iter().enumerate() {
        let has_black = black_idx < lower_black.len()
            && lower_black[black_idx].note_offset < white_key.note_offset + 2
            && (i == 0 || lower_black[black_idx].note_offset > lower_white[i - 1].note_offset);

        if has_black && i > 0 {
            let is_pressed = keyboard.is_key_pressed(lower_black[black_idx]);
            let display = format!("{:^w$}", lower_black[black_idx].display_char, w = key_width - 1);
            if is_pressed {
                lower_black_line.push_str(&format!("\x1b[7m{}\x1b[0m ", display));
            } else {
                lower_black_line.push_str(&format!("{} ", display));
            }
            black_idx += 1;
        } else {
            lower_black_line.push_str(&" ".repeat(key_width));
        }
    }
    lines.push(Line::from(Span::raw(format!("  {}", lower_black_line))));

    // Lower white keys line
    let mut lower_white_line = Vec::new();
    for key in &lower_white {
        let is_pressed = keyboard.is_key_pressed(key);
        let style = if is_pressed {
            Style::default().bg(Color::Green).fg(Color::Black)
        } else {
            Style::default().fg(Color::White)
        };
        let display = format!("{:^w$}", key.display_char, w = key_width);
        lower_white_line.push(Span::styled(display, style));
    }
    lines.push(Line::from(vec![Span::raw("  ")].into_iter().chain(lower_white_line).collect::<Vec<_>>()));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, inner);
}

/// Render footer with keybindings
fn render_footer(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let keybindings = if app.keyboard_active() {
        "K:Hide Kbd  </>:Oct  Space:Play/Pause  Q:Quit  ?:Help"
    } else if app.in_input_mode() {
        "Enter:Confirm  Esc:Cancel"
    } else {
        "Space:Play/Pause  \u{2190}\u{2192}:Seek  Tab:Panel  K:Kbd  L:Log  /:Search  ?:Help  Q:Quit"
    };

    let paragraph = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled(keybindings, Style::default().fg(Color::DarkGray)),
    ]))
    .block(block);

    frame.render_widget(paragraph, area);
}

/// Render search bar overlay
fn render_search_bar(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let search_area = Rect {
        x: area.x + 2,
        y: area.y + area.height - 5,
        width: area.width.saturating_sub(4).min(50),
        height: 3,
    };

    let query = if app.search_mode {
        &app.search_query
    } else {
        &app.log_search_query
    };

    let title = if app.search_mode {
        "Search Hierarchy"
    } else {
        "Search Log"
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(search_area);
    frame.render_widget(block, search_area);

    let text = format!("{}_", query);
    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, inner);
}

/// Render help modal
fn render_help_modal(frame: &mut Frame, area: Rect) {
    let modal_width = 60.min(area.width - 4);
    let modal_height = 20.min(area.height - 4);
    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    // Clear background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        modal_area,
    );

    let block = Block::default()
        .title(" Help - Press ? or Esc to close ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let help_text = vec![
        Line::from(Span::styled("Transport", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  Space     Play/Pause transport"),
        Line::from("  \u{2190}/\u{2192}       Seek backward/forward 1 beat"),
        Line::from("  Ctrl+\u{2190}/\u{2192}  Seek backward/forward 1 bar"),
        Line::from("  0/Home    Jump to start"),
        Line::from(""),
        Line::from(Span::styled("Navigation", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  \u{2191}/\u{2193}/j/k  Move selection"),
        Line::from("  Tab/L     Switch panel focus"),
        Line::from("  Enter     Collapse/expand item"),
        Line::from("  [/]       Collapse/Expand all"),
        Line::from("  PgUp/Dn   Page navigation"),
        Line::from(""),
        Line::from(Span::styled("Other", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
        Line::from("  K         Toggle virtual keyboard"),
        Line::from("  /         Search hierarchy"),
        Line::from("  1-5       Set log level"),
        Line::from("  Q         Quit"),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, modal_area);
}

/// Render error modal
fn render_error_modal(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let modal_width = 60.min(area.width - 4);
    let modal_height = 10.min(area.height - 4);
    let modal_area = Rect {
        x: (area.width - modal_width) / 2,
        y: (area.height - modal_height) / 2,
        width: modal_width,
        height: modal_height,
    };

    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        modal_area,
    );

    let block = Block::default()
        .title(" Error - Press Enter or Esc to close ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Red));

    let error_text = app.error_message.as_deref().unwrap_or("Unknown error");
    let paragraph = Paragraph::new(error_text)
        .block(block)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(Color::Red));

    frame.render_widget(paragraph, modal_area);
}

/// Generate beat progress bar using unicode blocks
fn beat_progress_bar_unicode(beat_in_bar: f64, total_beats: i64, width: usize) -> String {
    if total_beats <= 0 || width == 0 {
        return String::new();
    }

    let progress = (beat_in_bar / total_beats as f64).clamp(0.0, 1.0);
    let filled = (progress * width as f64) as usize;
    let remainder = ((progress * width as f64) - filled as f64) * 8.0;

    let mut bar = String::new();

    // Filled blocks
    for _ in 0..filled {
        bar.push('\u{2588}'); // █
    }

    // Partial block
    if filled < width {
        let partial = match remainder as usize {
            0 => ' ',
            1 => '\u{258F}', // ▏
            2 => '\u{258E}', // ▎
            3 => '\u{258D}', // ▍
            4 => '\u{258C}', // ▌
            5 => '\u{258B}', // ▋
            6 => '\u{258A}', // ▊
            7 => '\u{2589}', // ▉
            _ => '\u{2588}', // █
        };
        bar.push(partial);
    }

    // Empty blocks
    let remaining = width.saturating_sub(filled + 1);
    for _ in 0..remaining {
        bar.push(' ');
    }

    bar
}

/// Generate VU meter bar
fn vu_meter_bar(level: f32, width: usize) -> String {
    let filled = (level.clamp(0.0, 1.0) * width as f32) as usize;
    let mut bar = String::new();

    for i in 0..width {
        if i < filled {
            bar.push('\u{2588}'); // █
        } else {
            bar.push('\u{2591}'); // ░
        }
    }

    bar
}

/// Get VU meter color based on level
fn vu_meter_color(level: f32) -> Color {
    if level > 0.9 {
        Color::Red
    } else if level > 0.7 {
        Color::Yellow
    } else {
        Color::Green
    }
}
