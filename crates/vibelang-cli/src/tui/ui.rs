//! UI rendering logic for the TUI - Unified Hierarchy View

use crate::tui::app::{
    BeatInfo, ExportMode, HierarchyEntry, HierarchyKind, LogEntry, PanelFocus, QueueMetrics,
    ResourceStats, SequenceDisplay, SummaryStats, TuiApp,
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

/// Render the entire UI - simplified structure
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

    if app.midi_export.visible {
        render_midi_export_panel(frame, app, area);
        return;
    }

    let sequences = app.sequence_entries();
    let hierarchy = if app.search_query.is_empty() {
        app.hierarchy_entries()
    } else {
        app.filtered_hierarchy_entries()
    };
    let summary_stats = app.summary_stats();
    let queue_metrics = app.queue_metrics();
    let resource_stats = app.resource_stats();
    let beat_info = app.get_beat_info();

    // Render header with all stats and VU meter
    render_header(
        frame,
        layout.header,
        &beat_info,
        &summary_stats,
        &queue_metrics,
        &resource_stats,
        app.timeline_offset_beats,
        app.vu_level,
    );

    // Handle maximized log view
    if app.log_maximized {
        // Log takes the main area, hierarchy takes log area
        render_log_maximized(frame, layout.main, app, app.focused_panel == PanelFocus::Log);

        let focused = app.focused_panel == PanelFocus::Hierarchy;
        render_unified_hierarchy(
            frame,
            layout.log,
            &mut app.hierarchy_list_state,
            &hierarchy,
            &sequences,
            focused,
            app.hide_inactive,
            &app.search_query,
            &app.flash_items,
        );
    } else {
        // Normal layout
        let focused = app.focused_panel == PanelFocus::Hierarchy;
        render_unified_hierarchy(
            frame,
            layout.main,
            &mut app.hierarchy_list_state,
            &hierarchy,
            &sequences,
            focused,
            app.hide_inactive,
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
    queue: &QueueMetrics,
    resources: &ResourceStats,
    time_offset: f64,
    vu_level: f32,
) {
    let status_color = if beat_info.running {
        Color::Green
    } else {
        Color::Yellow
    };
    let status_icon = if beat_info.running { "▶" } else { "⏸" };

    // Calculate progress bar width (full width minus borders and label)
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
            Span::raw("  │  Bar "),
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
            Span::raw("  │  "),
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
                format!("{}▶", summary.patterns_playing),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("/{}", summary.patterns_total),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("  │  "),
            Span::styled("Melodies", Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(
                format!("{}▶", summary.melodies_playing),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("/{}", summary.melodies_total),
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw("  │  "),
            Span::styled("Sequences", Style::default().fg(Color::LightCyan)),
            Span::raw(" "),
            Span::styled(
                format!("{}", queue.active_sequences),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" active"),
            Span::raw("  │  "),
            Span::styled("Voices", Style::default().fg(Color::LightBlue)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.voices),
                Style::default().fg(Color::White),
            ),
        ]),
        // Line 4: Resources
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Synths", Style::default().fg(Color::Cyan)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.active_synths),
                Style::default().fg(Color::White),
            ),
            Span::raw("  │  "),
            Span::styled("Effects", Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.effects),
                Style::default().fg(Color::White),
            ),
            Span::raw("  │  "),
            Span::styled("Groups", Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.groups),
                Style::default().fg(Color::White),
            ),
            Span::raw("  │  "),
            Span::styled("Buffers", Style::default().fg(Color::Blue)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.buffers_used),
                Style::default().fg(Color::White),
            ),
            Span::raw("  │  "),
            Span::styled("Buses", Style::default().fg(Color::Blue)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.buses_used),
                Style::default().fg(Color::White),
            ),
            Span::raw("  │  "),
            Span::styled("Samples", Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(
                format!("{}", resources.samples),
                Style::default().fg(Color::White),
            ),
        ]),
        // Line 5: Gain, VU meter, and time offset
        Line::from(vec![
            Span::raw(" "),
            Span::styled("Gain", Style::default().fg(Color::Magenta)),
            Span::raw(" avg "),
            Span::styled(
                format!("{:.2}", summary.avg_voice_gain),
                Style::default().fg(Color::White),
            ),
            Span::raw(" max "),
            Span::styled(
                format!("{:.2}", summary.max_voice_gain),
                Style::default().fg(Color::White),
            ),
            Span::raw("  │  "),
            Span::styled("VU ", Style::default().fg(Color::Green)),
            Span::styled(
                vu_meter_bar(vu_level, 12),
                Style::default().fg(vu_meter_color(vu_level)),
            ),
            if time_offset.abs() > 0.01 {
                Span::raw("  │  ")
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

/// Render unified hierarchy view with integrated sequences
fn render_unified_hierarchy(
    frame: &mut Frame,
    area: Rect,
    list_state: &mut ListState,
    hierarchy: &[HierarchyEntry],
    sequences: &[SequenceDisplay],
    focused: bool,
    hide_inactive: bool,
    search_query: &str,
    flash_items: &std::collections::HashSet<String>,
) {
    let title = if !search_query.is_empty() {
        format!(" Hierarchy • search: {} ", search_query)
    } else if hide_inactive {
        " Hierarchy • filtered ".to_string()
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

    // Find the longest sequence for normalization
    let max_loop_beats = sequences
        .iter()
        .map(|s| s.loop_beats)
        .fold(0.0_f64, |a, b| a.max(b))
        .max(1.0);

    // Build combined list: hierarchy entries + sequences with progress bars
    let mut items: Vec<ListItem> = Vec::new();

    // Add hierarchy entries
    for (idx, entry) in hierarchy.iter().enumerate() {
        let should_flash = flash_items.contains(&entry.id);
        let item = render_hierarchy_entry(entry, idx, selection, focused, max_width, should_flash);
        items.push(item);
    }

    // Find sequence entries and replace them with enhanced normalized versions
    let hierarchy_count = hierarchy.len();
    for (i, item) in items.iter_mut().enumerate() {
        if i < hierarchy_count {
            let entry = &hierarchy[i];
            if entry.kind == HierarchyKind::Sequence {
                if let Some(seq) = sequences.iter().find(|s| s.name == entry.label) {
                    *item = render_sequence_entry_normalized(
                        seq,
                        i,
                        selection,
                        focused,
                        max_width,
                        max_loop_beats,
                    );
                }
            }
        }
    }

    if items.is_empty() {
        let paragraph = Paragraph::new("No groups or sequences defined")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }

    let list = List::new(items)
        .highlight_style(Style::default()); // Selection highlighting is done in render_hierarchy_entry
    frame.render_stateful_widget(list, inner, list_state);
}

/// Fixed column widths for aligned display
const COL_NAME: usize = 16;      // Name/identifier column
const COL_DETAIL: usize = 18;    // Detail/status column
#[allow(dead_code)]
const COL_PARAMS: usize = 35;    // Parameters column (reserved for future use)

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
        if entry.collapsed { "▸" } else { "▾" }
    } else {
        " "
    };

    let type_marker = match entry.kind {
        HierarchyKind::Group => "●",
        HierarchyKind::Voice => "♪",
        HierarchyKind::Pattern => if entry.active { "◆" } else { "◇" },
        HierarchyKind::Melody => if entry.active { "◆" } else { "◇" },
        HierarchyKind::Effect => "◈",
        HierarchyKind::Sequence => if entry.active { "▶" } else { "▷" },
        HierarchyKind::Section => "━",
    };

    // Build params string
    let params_str = if !entry.params.is_empty() {
        let param_list: Vec<String> = entry
            .params
            .iter()
            .take(6)
            .map(|(k, v)| format!("{}:{}", k, v))
            .collect();
        param_list.join("  ") // Double space between params
    } else {
        String::new()
    };

    // Calculate available space after indent
    let available = max_width.saturating_sub(indent_len + 3); // 3 for markers

    // Adjust column widths based on available space
    let name_col = COL_NAME.min(available / 3);
    let detail_col = COL_DETAIL.min(available / 3);
    let params_col = available.saturating_sub(name_col + detail_col + 2);

    let name = truncate_string(&entry.label, name_col);
    let detail = truncate_string(&entry.detail, detail_col);

    let marker_color = if entry.active {
        entry.color()
    } else {
        Color::DarkGray
    };

    // For active patterns/melodies, use brighter colors
    let (name_color, detail_color) = if entry.active {
        match entry.kind {
            HierarchyKind::Pattern | HierarchyKind::Melody => (Color::White, Color::Green),
            _ => (entry.color(), Color::DarkGray),
        }
    } else {
        (entry.color(), Color::DarkGray)
    };

    let line = Line::from(vec![
        Span::raw(indent),
        Span::styled(
            collapse_marker,
            Style::default().fg(if entry.collapsible { Color::White } else { Color::DarkGray }),
        ),
        Span::styled(type_marker, Style::default().fg(marker_color)),
        Span::raw(" "),
        // Name column - left aligned, fixed width (white + bold when active pattern/melody)
        Span::styled(
            format!("{:<width$}", name, width = name_col),
            Style::default()
                .fg(name_color)
                .add_modifier(if entry.active { Modifier::BOLD } else { Modifier::empty() }),
        ),
        // Separator
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        // Detail column - green when playing, grey otherwise
        Span::styled(
            format!("{:<width$}", detail, width = detail_col),
            Style::default().fg(detail_color),
        ),
        // Separator (only if params exist)
        if !params_str.is_empty() {
            Span::styled(" │ ", Style::default().fg(Color::DarkGray))
        } else {
            Span::raw("")
        },
        // Params column
        if !params_str.is_empty() {
            Span::styled(
                truncate_string(&params_str, params_col),
                Style::default().fg(Color::Rgb(180, 180, 120)),
            )
        } else {
            Span::raw("")
        },
    ]);

    let style = if focused && idx == selection {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else if should_flash {
        // Flash effect - bright background for recently changed items
        Style::default()
            .bg(Color::Rgb(60, 60, 30))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    ListItem::new(line).style(style)
}

/// Render a sequence entry with normalized progress bar showing clips
fn render_sequence_entry_normalized(
    seq: &SequenceDisplay,
    idx: usize,
    selection: usize,
    focused: bool,
    max_width: usize,
    max_loop_beats: f64,
) -> ListItem<'static> {
    let indent = "  ";

    // Status icons
    let (marker, marker_color) = if seq.paused {
        ("⏸", Color::Yellow)
    } else if seq.playing {
        ("▶", Color::Green)
    } else {
        ("○", Color::DarkGray)
    };

    // Use same column widths as hierarchy for alignment
    let name_col = COL_NAME.min(max_width / 4);
    let pos_col = 12; // Position info column
    let bar_width = max_width.saturating_sub(name_col + pos_col + 12).max(20);

    let name = truncate_string(&seq.name, name_col);
    let pos_text = format!("{:>5.1} / {:<5.1}b", seq.position, seq.loop_beats);

    // Build the normalized progress bar with clips
    let bar = sequence_progress_bar_normalized(seq, bar_width, max_loop_beats);

    let mut spans = vec![
        Span::raw(indent.to_string()),
        Span::styled(
            marker,
            Style::default().fg(marker_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        // Name column - aligned with hierarchy
        Span::styled(
            format!("{:<width$}", name, width = name_col),
            Style::default()
                .fg(if seq.playing { Color::LightCyan } else { Color::DarkGray })
                .add_modifier(if seq.playing { Modifier::BOLD } else { Modifier::empty() }),
        ),
        // Separator
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
    ];

    // Progress bar
    spans.extend(bar);

    // Separator and position
    spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
    spans.push(Span::styled(
        pos_text,
        Style::default().fg(Color::DarkGray),
    ));

    let line = Line::from(spans);

    let style = if focused && idx == selection {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    ListItem::new(line).style(style)
}

/// Create normalized progress bar with clip indicators
/// Shorter sequences loop multiple times to align with longest
fn sequence_progress_bar_normalized(
    seq: &SequenceDisplay,
    width: usize,
    max_loop_beats: f64,
) -> Vec<Span<'static>> {
    if width < 5 || seq.loop_beats <= 0.0 {
        return vec![Span::styled("─".repeat(width), Style::default().fg(Color::DarkGray))];
    }

    // How many times this sequence loops within the max duration
    let loop_count = (max_loop_beats / seq.loop_beats).ceil() as usize;
    let width_per_loop = width / loop_count.max(1);

    // Characters
    const EMPTY: char = '─';
    const CLIP_MARK: char = '▪';  // Small square for clip positions
    const LOOP_SEP: char = '│';   // Loop boundary separator
    const PLAYHEAD: char = '●';

    // Determine colors - playhead is always visible (white), only track dims when not playing
    let (track_color, _clip_color, head_color) = if seq.paused {
        (Color::DarkGray, Color::Yellow, Color::Yellow)
    } else if seq.playing {
        (Color::DarkGray, Color::Cyan, Color::White)
    } else {
        // Even when not "active", show visible playhead since all sequences sync to transport
        (Color::DarkGray, Color::Gray, Color::White)
    };

    // Build cell array: (char, color, is_playhead)
    let mut cells: Vec<(char, Color, bool)> = vec![(EMPTY, track_color, false); width];

    // Mark loop boundaries
    for loop_idx in 1..loop_count {
        let sep_pos = loop_idx * width_per_loop;
        if sep_pos < width {
            cells[sep_pos] = (LOOP_SEP, Color::DarkGray, false);
        }
    }

    // Mark clip positions within each loop
    for clip in &seq.clips {
        let clip_start_ratio = clip.start / seq.loop_beats;
        let clip_end_ratio = clip.end / seq.loop_beats;

        // Draw clip in each loop iteration
        for loop_idx in 0..loop_count {
            let loop_offset = loop_idx * width_per_loop;

            // Start marker
            let start_pos = loop_offset + (clip_start_ratio * width_per_loop as f64) as usize;
            if start_pos < width && cells[start_pos].0 != LOOP_SEP {
                cells[start_pos] = (CLIP_MARK, clip.kind.color(), false);
            }

            // End marker (if different from start)
            let end_pos = loop_offset + (clip_end_ratio * width_per_loop as f64) as usize;
            if end_pos < width && end_pos != start_pos && cells[end_pos].0 != LOOP_SEP {
                cells[end_pos] = (CLIP_MARK, clip.kind.color(), false);
            }
        }
    }

    // Calculate playhead position using elapsed_beats (total time, not wrapped)
    // This allows the playhead to progress through all visual loop iterations
    let elapsed_in_normalized = seq.elapsed_beats.rem_euclid(max_loop_beats);
    let playhead_ratio = elapsed_in_normalized / max_loop_beats;
    let playhead_pos = (playhead_ratio * width as f64).round() as usize;
    let playhead_pos = playhead_pos.min(width.saturating_sub(1));

    // Mark playhead position
    cells[playhead_pos] = (PLAYHEAD, head_color, true);

    // Convert to spans, grouping by color
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current_color = cells[0].1;
    let mut current_bold = cells[0].2;
    let mut buffer = String::new();

    for (ch, color, is_head) in cells {
        if color == current_color && is_head == current_bold {
            buffer.push(ch);
        } else {
            if !buffer.is_empty() {
                let style = if current_bold {
                    Style::default().fg(current_color).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(current_color)
                };
                spans.push(Span::styled(buffer.clone(), style));
                buffer.clear();
            }
            buffer.push(ch);
            current_color = color;
            current_bold = is_head;
        }
    }

    if !buffer.is_empty() {
        let style = if current_bold {
            Style::default().fg(current_color).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(current_color)
        };
        spans.push(Span::styled(buffer, style));
    }

    spans
}

/// Clean Unicode beat progress bar for header
fn beat_progress_bar_unicode(beat_in_bar: f64, total_beats: i64, width: usize) -> String {
    if width < 5 {
        return "···".to_string();
    }

    let beats = total_beats.max(1) as usize;

    // Beautiful bar characters
    const FILLED: &str = "▓";
    const PARTIAL: &str = "▒";
    const EMPTY: &str = "░";

    // Calculate width per beat
    let beat_width = width / beats;
    let mut out = String::with_capacity(width);

    for i in 0..beats {
        let beat_start = i as f64;

        for j in 0..beat_width {
            let cell_start = beat_start + (j as f64 / beat_width as f64);
            let cell_end = cell_start + (1.0 / beat_width as f64);

            if beat_in_bar >= cell_end {
                out.push_str(FILLED);
            } else if beat_in_bar >= cell_start {
                out.push_str(PARTIAL);
            } else {
                out.push_str(EMPTY);
            }
        }
    }

    out
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

    let level_indicator = match app.min_log_level {
        Level::Error => "ERR",
        Level::Warn => "≥WARN",
        Level::Info => "≥INFO",
        Level::Debug => "≥DBG",
        Level::Trace => "ALL",
    };

    let title = if !app.log_search_query.is_empty() {
        format!(" Log [{}] search: {} ", level_indicator, app.log_search_query)
    } else {
        format!(" Log [{}] ", level_indicator)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let max_lines = inner.height as usize;
    let filtered: Vec<&LogEntry> = app.filtered_log_entries();
    let total = filtered.len();
    let start = total.saturating_sub(max_lines);
    let entries: Vec<&LogEntry> = filtered.into_iter().skip(start).collect();

    let lines: Vec<Line> = entries
        .into_iter()
        .map(|entry| {
            let level_style = log_level_style(entry.level);
            let message_style = if !app.log_search_query.is_empty()
                && entry.message.to_lowercase().contains(&app.log_search_query.to_lowercase()) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(vec![
                Span::styled(
                    format!("[{}] ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{:<5}", log_level_label(entry.level)), level_style),
                Span::raw(" "),
                Span::styled(entry.message.clone(), message_style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

/// Render footer with keybinds
fn render_footer(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let mut spans = vec![
        Span::styled("?", Style::default().fg(Color::White)),
        Span::styled(" help", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("space", Style::default().fg(Color::White)),
        Span::styled(" play", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled("/", Style::default().fg(Color::White)),
        Span::styled(" find", Style::default().fg(Color::DarkGray)),
        Span::raw("  "),
        Span::styled(
            if app.keyboard_active() { "K" } else { "K" },
            Style::default().fg(if app.keyboard_active() { Color::Cyan } else { Color::White }),
        ),
        Span::styled(
            if app.keyboard_active() { " 🎹" } else { " piano" },
            Style::default().fg(if app.keyboard_active() { Color::Cyan } else { Color::DarkGray }),
        ),
        Span::raw("  "),
        Span::styled("L", Style::default().fg(Color::White)),
        Span::styled(
            if app.log_maximized { " mini" } else { " max" },
            Style::default().fg(Color::DarkGray),
        ),
    ];

    if app.is_scrubbing {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("🔇 {:+.1}b", app.pending_seek_beats),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    } else if app.timeline_offset_beats.abs() > 0.01 {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{:+.1}b", app.timeline_offset_beats),
            Style::default().fg(Color::Yellow),
        ));
    }

    if app.error_message.is_some() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "e error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    let paragraph = Paragraph::new(Line::from(spans))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

/// Render error modal
fn render_error_modal(frame: &mut Frame, app: &TuiApp, area: Rect) {
    if let Some(error) = &app.error_message {
        let modal_width = area.width.saturating_sub(10).min(100);
        let modal_height = area.height.saturating_sub(10).min(30);

        let modal_x = (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = Rect {
            x: modal_x,
            y: modal_y,
            width: modal_width,
            height: modal_height,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(" ERROR ")
            .style(Style::default().bg(Color::Black));

        let text = Paragraph::new(error.as_str())
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::Red));

        frame.render_widget(text, modal_area);

        let help_area = Rect {
            x: modal_x,
            y: modal_y + modal_height,
            width: modal_width,
            height: 1,
        };

        let help = Paragraph::new("Press ESC or 'e' to close")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Gray));

        frame.render_widget(help, help_area);
    }
}

fn log_level_label(level: Level) -> &'static str {
    match level {
        Level::Error => "ERR",
        Level::Warn => "WARN",
        Level::Info => "INFO",
        Level::Debug => "DBG",
        Level::Trace => "TRC",
    }
}

fn log_level_style(level: Level) -> Style {
    match level {
        Level::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        Level::Warn => Style::default().fg(Color::Yellow),
        Level::Info => Style::default().fg(Color::Cyan),
        Level::Debug => Style::default().fg(Color::Green),
        Level::Trace => Style::default().fg(Color::Magenta),
    }
}

/// Render help modal with all keyboard shortcuts
fn render_help_modal(frame: &mut Frame, area: Rect) {
    let modal_width = area.width.saturating_sub(10).min(70);
    let modal_height = area.height.saturating_sub(6).min(32);

    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect {
        x: modal_x,
        y: modal_y,
        width: modal_width,
        height: modal_height,
    };

    let help_text = vec![
        Line::from(vec![
            Span::styled("  Navigation", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  ↑/↓ k/j     ", Style::default().fg(Color::White)),
            Span::styled("Move selection up/down", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  PgUp/PgDn   ", Style::default().fg(Color::White)),
            Span::styled("Page up/down", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  Home/0      ", Style::default().fg(Color::White)),
            Span::styled("Jump to start", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  a           ", Style::default().fg(Color::White)),
            Span::styled("Jump to first active pattern/melody", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Playback", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Space       ", Style::default().fg(Color::White)),
            Span::styled("Play/pause transport", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  ←/→         ", Style::default().fg(Color::White)),
            Span::styled("Seek backward/forward 1 beat", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+←/→    ", Style::default().fg(Color::White)),
            Span::styled("Seek backward/forward 1 bar", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  View", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Enter       ", Style::default().fg(Color::White)),
            Span::styled("Expand/collapse selected item", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  [           ", Style::default().fg(Color::White)),
            Span::styled("Collapse all groups", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  ]           ", Style::default().fg(Color::White)),
            Span::styled("Expand all groups", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  f           ", Style::default().fg(Color::White)),
            Span::styled("Toggle filter inactive items", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  L           ", Style::default().fg(Color::White)),
            Span::styled("Toggle log panel maximized", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  Tab/l       ", Style::default().fg(Color::White)),
            Span::styled("Switch focus between hierarchy and log", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Search & Filter", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  /           ", Style::default().fg(Color::White)),
            Span::styled("Search hierarchy", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  Ctrl+/      ", Style::default().fg(Color::White)),
            Span::styled("Search logs", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  1-5         ", Style::default().fg(Color::White)),
            Span::styled("Set log level (1=Error, 5=Trace)", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  Esc         ", Style::default().fg(Color::White)),
            Span::styled("Cancel search / close modal", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Virtual Keyboard", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  K (capital) ", Style::default().fg(Color::White)),
            Span::styled("Toggle virtual MIDI keyboard", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  < >         ", Style::default().fg(Color::White)),
            Span::styled("Octave down/up (when keyboard active)", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  Lower oct   ", Style::default().fg(Color::White)),
            Span::styled("Y-M row (white), SFGJKL (black): A2-C4", Style::default().fg(Color::Gray)),
        ]),
        Line::from(vec![
            Span::styled("  Upper oct   ", Style::default().fg(Color::White)),
            Span::styled("QWERTZU row (white), 12456 (black): D4-C5", Style::default().fg(Color::Gray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  q/Ctrl+c    ", Style::default().fg(Color::White)),
            Span::styled("Quit", Style::default().fg(Color::Gray)),
        ]),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Keyboard Shortcuts ")
        .style(Style::default().bg(Color::Black));

    let text = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });

    // Clear background
    frame.render_widget(ratatui::widgets::Clear, modal_area);
    frame.render_widget(text, modal_area);

    let help_area = Rect {
        x: modal_x,
        y: modal_y + modal_height,
        width: modal_width,
        height: 1,
    };

    let help = Paragraph::new("Press ? or Esc to close")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    frame.render_widget(help, help_area);
}

/// Render MIDI export panel for pattern/melody export
fn render_midi_export_panel(frame: &mut Frame, app: &TuiApp, area: Rect) {
    let modal_width = area.width.saturating_sub(10).min(80);
    let modal_height = area.height.saturating_sub(4).min(30);

    let modal_x = (area.width.saturating_sub(modal_width)) / 2;
    let modal_y = (area.height.saturating_sub(modal_height)) / 2;

    let modal_area = Rect {
        x: modal_x,
        y: modal_y,
        width: modal_width,
        height: modal_height,
    };

    // Build the content
    let mut lines: Vec<Line> = Vec::new();

    // Mode selector
    let melody_style = if matches!(app.midi_export.export_mode, ExportMode::Melody) {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };
    let pattern_style = if matches!(app.midi_export.export_mode, ExportMode::Pattern) {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    lines.push(Line::from(vec![
        Span::styled("  Mode: ", Style::default().fg(Color::White)),
        Span::styled(" Melody ", melody_style),
        Span::raw("  "),
        Span::styled(" Pattern ", pattern_style),
        Span::styled("   (Tab)", Style::default().fg(Color::DarkGray)),
    ]));

    // Settings on same line
    lines.push(Line::from(vec![
        Span::styled("  Bars: ", Style::default().fg(Color::White)),
        Span::styled(
            format!("{}", app.midi_export.bar_count),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" (+/-)", Style::default().fg(Color::DarkGray)),
        Span::styled("   Quant: ", Style::default().fg(Color::White)),
        Span::styled(
            format!("1/{}", app.midi_export.quantization),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" (←/→)", Style::default().fg(Color::DarkGray)),
    ]));

    lines.push(Line::from(""));

    // Voice selection section
    lines.push(Line::from(vec![
        Span::styled("  Voices", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!(
                " ({}/{})",
                app.midi_export.selected_voices.len(),
                app.midi_export.available_voices.len()
            ),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled("   (↑/↓ navigate, Space select, a=all, n=none)", Style::default().fg(Color::DarkGray)),
    ]));

    if app.midi_export.available_voices.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("(no recorded voices)", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
        ]));
    } else {
        // Show up to 6 voices
        let max_voices = 6;
        let start_idx = if app.midi_export.voice_cursor >= max_voices {
            app.midi_export.voice_cursor - max_voices + 1
        } else {
            0
        };
        let end_idx = (start_idx + max_voices).min(app.midi_export.available_voices.len());

        for (i, voice) in app.midi_export.available_voices.iter().enumerate().skip(start_idx).take(end_idx - start_idx) {
            let is_cursor = i == app.midi_export.voice_cursor;
            let is_selected = app.midi_export.selected_voices.contains(&i);

            let checkbox = if is_selected { "[✓]" } else { "[ ]" };
            let cursor_marker = if is_cursor { ">" } else { " " };

            let style = if is_cursor {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Gray)
            };

            lines.push(Line::from(vec![
                Span::styled(format!("   {}", cursor_marker), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{} {}", checkbox, voice), style),
            ]));
        }

        // Show scroll indicator if needed
        if app.midi_export.available_voices.len() > max_voices {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("    ... ({} more)", app.midi_export.available_voices.len() - max_voices),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Preview section
    lines.push(Line::from(vec![
        Span::styled("  Output", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));

    // Show preview or placeholder
    if app.midi_export.preview.starts_with('(') {
        // Placeholder message
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(&app.midi_export.preview, Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)),
        ]));
    } else {
        // Split preview into lines
        let max_preview_width = (modal_width as usize).saturating_sub(8);
        let preview_lines: Vec<&str> = app.midi_export.preview.lines().collect();

        // Show up to 6 lines of preview
        for line in preview_lines.iter().take(6) {
            let display_line = if line.len() > max_preview_width {
                format!("{}...", &line[..max_preview_width - 3])
            } else {
                line.to_string()
            };
            lines.push(Line::from(vec![
                Span::styled("    ", Style::default()),
                Span::styled(display_line, Style::default().fg(Color::Green)),
            ]));
        }
        if preview_lines.len() > 6 {
            lines.push(Line::from(vec![
                Span::styled("    ", Style::default()),
                Span::styled(format!("... ({} more lines)", preview_lines.len() - 6), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    lines.push(Line::from(""));

    // Status message
    if let Some((msg, _)) = &app.midi_export.status_message {
        let style = if msg.contains('✓') {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(msg.clone(), style),
        ]));
    } else {
        lines.push(Line::from(""));
    }

    // Help line
    lines.push(Line::from(vec![
        Span::styled(
            "  Enter/y: Copy | c: Clear | Esc/q: Close",
            Style::default().fg(Color::DarkGray),
        ),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" MIDI Export ")
        .style(Style::default().bg(Color::Black));

    let text = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    // Clear background and render
    frame.render_widget(ratatui::widgets::Clear, modal_area);
    frame.render_widget(text, modal_area);
}

/// Render maximized log panel (takes main area)
fn render_log_maximized(frame: &mut Frame, area: Rect, app: &TuiApp, focused: bool) {
    let border_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let level_indicator = match app.min_log_level {
        Level::Error => "ERR",
        Level::Warn => "≥WARN",
        Level::Info => "≥INFO",
        Level::Debug => "≥DBG",
        Level::Trace => "ALL",
    };

    let title = if !app.log_search_query.is_empty() {
        format!(" Log [{}] search: {} (maximized) ", level_indicator, app.log_search_query)
    } else {
        format!(" Log [{}] (maximized) ", level_indicator)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let max_lines = inner.height as usize;
    let filtered: Vec<&LogEntry> = app.filtered_log_entries();
    let total = filtered.len();
    let start = total.saturating_sub(max_lines);
    let entries: Vec<&LogEntry> = filtered.into_iter().skip(start).collect();

    let lines: Vec<Line> = entries
        .into_iter()
        .map(|entry| {
            let level_style = log_level_style(entry.level);
            let message_style = if !app.log_search_query.is_empty()
                && entry.message.to_lowercase().contains(&app.log_search_query.to_lowercase()) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(vec![
                Span::styled(
                    format!("[{}] ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!("{:<5}", log_level_label(entry.level)), level_style),
                Span::raw(" "),
                Span::styled(entry.message.clone(), message_style),
            ])
        })
        .collect();

    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
}

/// Render search bar at bottom of screen
fn render_search_bar(frame: &mut Frame, area: Rect, app: &TuiApp) {
    let search_area = Rect {
        x: area.x + 2,
        y: area.height.saturating_sub(3),
        width: area.width.saturating_sub(4).min(60),
        height: 3,
    };

    let (label, query) = if app.search_mode {
        ("Search hierarchy: ", &app.search_query)
    } else {
        ("Search logs: ", &app.log_search_query)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::Black));

    let text = Line::from(vec![
        Span::styled(label, Style::default().fg(Color::Yellow)),
        Span::styled(query.clone(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
    ]);

    let paragraph = Paragraph::new(text).block(block);

    // Clear background
    frame.render_widget(ratatui::widgets::Clear, search_area);
    frame.render_widget(paragraph, search_area);
}

/// Generate VU meter bar
fn vu_meter_bar(level: f32, width: usize) -> String {
    let filled = ((level * width as f32).round() as usize).min(width);
    let empty = width.saturating_sub(filled);

    let mut bar = String::new();
    for i in 0..filled {
        let ratio = i as f32 / width as f32;
        if ratio < 0.6 {
            bar.push('█');
        } else if ratio < 0.8 {
            bar.push('▓');
        } else {
            bar.push('▒');
        }
    }
    bar.push_str(&"░".repeat(empty));
    bar
}

/// Get VU meter color based on level
fn vu_meter_color(level: f32) -> Color {
    if level < 0.6 {
        Color::Green
    } else if level < 0.8 {
        Color::Yellow
    } else {
        Color::Red
    }
}

/// Render the virtual MIDI keyboard with piano-style visualization
pub fn render_keyboard(frame: &mut Frame, area: Rect, keyboard: &VirtualKeyboard, port_name: Option<&str>, os_keyboard_active: bool) {
    if !keyboard.visible || area.height < 4 || area.width < 40 {
        return;
    }

    // Create the block with title showing port name and input mode
    let input_mode = if os_keyboard_active { "OS" } else { "Terminal" };
    let title = if let Some(port) = port_name {
        format!(" 🎹 Piano [{}] ({}) → {} ", keyboard.octave_name(), input_mode, port)
    } else {
        format!(" 🎹 Piano [{}] (JACK not available) ", keyboard.octave_name())
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

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

    // Extended piano layout - all keys in one continuous row (A2 to C5):
    //    ┌S┐        ┌F┐ ┌G┐        ┌J┐ ┌K┐ ┌L┐    ┌1┐ ┌2┐     ┌4┐ ┌5┐ ┌6┐
    //    └─┘        └─┘ └─┘        └─┘ └─┘ └─┘    └─┘ └─┘     └─┘ └─┘ └─┘
    //   │  Y  │  X  │  C  │  V  │  B  │  N  │  M  │  ,  │  .  │  -  │  Q  │  W  │  E  │  R  │  T  │  Z  │  U  │
    //   │ A2  │ B2  │ C3  │ D3  │ E3  │ F3  │ G3  │ A3  │ B3  │ C4  │ D4  │ E4  │ F4  │ G4  │ A4  │ B4  │ C5  │

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
    // The index refers to which white key the black key appears AFTER
    // White key indices: Y=0, X=1, C=2, V=3, B=4, N=5, M=6, ,=7, .=8, -=9, Q=10, W=11, E=12, R=13, T=14, Z=15, U=16
    let black_keys: [(usize, char, i8); 11] = [
        (0, 'S', -2),   // A#2 - between Y(A2) and X(B2)
        (2, 'F', 1),    // C#3 - between C(C3) and V(D3)
        (3, 'G', 3),    // D#3 - between V(D3) and B(E3)
        (5, 'J', 6),    // F#3 - between N(F3) and M(G3)
        (6, 'K', 8),    // G#3 - between M(G3) and ,(A3)
        (7, 'L', 10),   // A#3 - between ,(A3) and .(B3)
        // Upper octave: -(C4)=9, Q(D4)=10, W(E4)=11, E(F4)=12, R(G4)=13, T(A4)=14, Z(B4)=15, U(C5)=16
        (9, '1', 13),   // C#4 - between -(C4) and Q(D4)
        (10, '2', 15),  // D#4 - between Q(D4) and W(E4)
        // No black between E4-F4 (W and E)
        (12, '4', 18),  // F#4 - between E(F4) and R(G4)
        (13, '5', 20),  // G#4 - between R(G4) and T(A4)
        (14, '6', 22),  // A#4 - between T(A4) and Z(B4)
        // No black between B4-C5 (Z and U)
    ];

    let mut lines: Vec<Line> = Vec::new();

    // Helper to check if note is pressed
    let is_note_pressed = |offset: i8| -> bool {
        let note = (base as i16 + offset as i16).clamp(0, 127) as u8;
        keyboard.pressed_notes.contains(&note)
    };

    // Row 1: Black key tops (┌X┐)
    let mut row1_chars: Vec<(char, Style)> = vec![(' ', Style::default()); total_width];
    for (idx, wk) in white_keys.iter().enumerate() {
        let wstyle = if is_note_pressed(wk.note_offset) {
            Style::default().bg(Color::Cyan)
        } else {
            Style::default().bg(Color::White)
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
            row1_chars[pos] = ('│', Style::default().fg(Color::Black).bg(Color::White));
        }
    }
    for (after_idx, key_char, offset) in black_keys.iter() {
        let bstyle = if is_note_pressed(*offset) {
            Style::default().fg(Color::White).bg(Color::Magenta).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray).bg(Color::Rgb(30, 30, 30))
        };
        let boundary = (*after_idx + 1) * key_width;
        let start = boundary.saturating_sub(1);
        if start + 3 <= row1_chars.len() {
            row1_chars[start] = ('┌', bstyle);
            row1_chars[start + 1] = (*key_char, bstyle);
            row1_chars[start + 2] = ('┐', bstyle);
        }
    }
    let mut row1_spans = vec![Span::raw(padding_str.clone())];
    row1_spans.extend(build_spans_from_chars(&row1_chars));
    row1_spans.push(Span::raw("  "));
    row1_spans.push(Span::styled("Playing: ", Style::default().fg(Color::DarkGray)));
    let pressed_str = if keyboard.pressed_notes.is_empty() {
        "—".to_string()
    } else {
        let mut notes: Vec<_> = keyboard.pressed_notes.iter().copied().collect();
        notes.sort();
        notes.iter().map(|n| note_name(*n)).collect::<Vec<_>>().join(" ")
    };
    row1_spans.push(Span::styled(pressed_str, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)));
    lines.push(Line::from(row1_spans));

    // Row 2: Black key bottoms (└─┘)
    let mut row2_chars: Vec<(char, Style)> = vec![(' ', Style::default()); total_width];
    for (idx, wk) in white_keys.iter().enumerate() {
        let wstyle = if is_note_pressed(wk.note_offset) {
            Style::default().bg(Color::Cyan)
        } else {
            Style::default().bg(Color::White)
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
            row2_chars[pos] = ('│', Style::default().fg(Color::Black).bg(Color::White));
        }
    }
    for (after_idx, _, offset) in black_keys.iter() {
        let bstyle = if is_note_pressed(*offset) {
            Style::default().fg(Color::Magenta).bg(Color::Magenta)
        } else {
            Style::default().fg(Color::Rgb(30, 30, 30)).bg(Color::Rgb(30, 30, 30))
        };
        let boundary = (*after_idx + 1) * key_width;
        let start = boundary.saturating_sub(1);
        if start + 3 <= row2_chars.len() {
            row2_chars[start] = ('└', bstyle);
            row2_chars[start + 1] = ('─', bstyle);
            row2_chars[start + 2] = ('┘', bstyle);
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
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Black).bg(Color::White)
        };
        if idx == 0 {
            row3_spans.push(Span::styled(format!("  {}   ", wk.display_char), wstyle));
        } else {
            row3_spans.push(Span::styled("│", Style::default().fg(Color::Black).bg(Color::White)));
            row3_spans.push(Span::styled(format!("  {}  ", wk.display_char), wstyle));
        }
    }
    row3_spans.push(Span::raw("  "));
    row3_spans.push(Span::styled("<> ", Style::default().fg(Color::White)));
    row3_spans.push(Span::styled("oct  ", Style::default().fg(Color::DarkGray)));
    row3_spans.push(Span::styled("Esc ", Style::default().fg(Color::White)));
    row3_spans.push(Span::styled("hide", Style::default().fg(Color::DarkGray)));
    lines.push(Line::from(row3_spans));

    // Row 4: White keys with note names
    let mut row4_spans: Vec<Span> = vec![Span::raw(padding_str.clone())];
    for (idx, wk) in white_keys.iter().enumerate() {
        let wnote = (base as i16 + wk.note_offset as i16).clamp(0, 127) as u8;
        let is_pressed = is_note_pressed(wk.note_offset);
        let note_str = note_name(wnote);
        let wstyle = if is_pressed {
            Style::default().fg(Color::DarkGray).bg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray).bg(Color::White)
        };
        if idx == 0 {
            row4_spans.push(Span::styled(format!(" {:^3}  ", note_str), wstyle));
        } else {
            row4_spans.push(Span::styled("│", Style::default().fg(Color::Black).bg(Color::White)));
            row4_spans.push(Span::styled(format!(" {:^3} ", note_str), wstyle));
        }
    }
    lines.push(Line::from(row4_spans));

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

/// Compact keyboard view for narrow terminals
fn render_keyboard_compact(frame: &mut Frame, area: Rect, keyboard: &VirtualKeyboard) {
    let base = keyboard.effective_base_note();
    let white_keys = keyboard.config.white_keys();

    let mut lines: Vec<Line> = Vec::new();

    // Black keys info: (after_white_index, key_char, note_offset)
    // Piano has black keys: A#, C#, D#, F#, G#, A# (no black between B-C and E-F)
    let black_keys: [(usize, char, i8); 6] = [
        (0, 'S', -2),  // A#2 - between Y(A2) and X(B2)
        (2, 'F', 1),   // C#3 - between C(C3) and V(D3)
        (3, 'G', 3),   // D#3 - between V(D3) and B(E3)
        (5, 'J', 6),   // F#3 - between N(F3) and M(G3)
        (6, 'K', 8),   // G#3 - between M(G3) and ,(A3)
        (7, 'L', 10),  // A#3 - between ,(A3) and .(B3)
    ];

    let key_width: usize = 3;
    let total_width = white_keys.len() * key_width;

    // Build black keys row using character buffer for precise positioning
    let mut black_chars: Vec<(char, Style)> = vec![(' ', Style::default()); total_width];

    // Place black keys at boundaries (offset by half a white key width)
    for (after_idx, key_char, offset) in black_keys.iter() {
        let note = (base as i16 + *offset as i16).clamp(0, 127) as u8;
        let is_pressed = keyboard.pressed_notes.contains(&note);
        let style = if is_pressed {
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).bg(Color::Rgb(40, 40, 40))
        };
        // Position at boundary between white keys (after_idx * width + width/2)
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
        white_spans.push(Span::styled(format!(" {} ", wk.display_char), style));
    }
    lines.push(Line::from(white_spans));

    // Status line with notes being played
    let pressed: String = if keyboard.pressed_notes.is_empty() {
        "-".to_string()
    } else {
        keyboard.pressed_notes.iter().map(|n| note_name(*n)).collect::<Vec<_>>().join(" ")
    };
    lines.push(Line::from(vec![
        Span::styled(" ▶ ", Style::default().fg(Color::Cyan)),
        Span::styled(pressed, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("    ", Style::default()),
        Span::styled("<>", Style::default().fg(Color::White)),
        Span::styled(":oct ", Style::default().fg(Color::DarkGray)),
        Span::styled("Esc", Style::default().fg(Color::White)),
        Span::styled(":hide", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}
