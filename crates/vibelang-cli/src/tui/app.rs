//! TUI application state and logic adapted for vibelang-core

use crate::tui::keyboard::VirtualKeyboard;
use crate::tui::TuiEvent;
use log::Level;
use ratatui::style::Color;
use ratatui::widgets::ListState;
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};
use vibelang_core::{GroupId, State, VoiceId};

const MAX_LOG_ENTRIES: usize = 100;

/// Debounce duration for transport seeking (in milliseconds)
const SEEK_DEBOUNCE_MS: u64 = 250;

/// Main TUI application state
pub struct TuiApp {
    /// Log messages buffer
    pub log_buffer: VecDeque<LogEntry>,
    /// Current error message (if any)
    pub error_message: Option<String>,
    /// Show error modal
    pub show_error_modal: bool,
    /// Show help modal
    pub show_help_modal: bool,
    /// Cached runtime state for rendering
    pub runtime_state: Option<State>,
    /// Current focus target (Hierarchy or Log)
    pub focused_panel: PanelFocus,
    /// Hide inactive rows
    pub hide_inactive: bool,
    /// Selected hierarchy index
    pub hierarchy_selection: usize,
    /// List state for scrolling in hierarchy view
    pub hierarchy_list_state: ListState,
    /// Timeline offset in beats for scrubbing preview
    pub timeline_offset_beats: f64,
    /// Pending seek offset (accumulated from keypresses, applied after debounce)
    pub pending_seek_beats: f64,
    /// Time of last seek keypress (for debouncing)
    pub last_seek_time: Option<Instant>,
    /// Whether we're currently in scrub mode (actively seeking)
    pub is_scrubbing: bool,
    /// Set of collapsed item IDs (groups, sequences, etc.)
    pub collapsed_items: HashSet<String>,
    /// Minimum log level to display (for filtering)
    pub min_log_level: Level,
    /// Search mode active
    pub search_mode: bool,
    /// Current search query
    pub search_query: String,
    /// Page size for Page Up/Down navigation
    pub page_size: usize,
    /// Track previously active items for flash effect
    pub prev_active_items: HashSet<String>,
    /// Items that recently changed state (for flash effect)
    pub flash_items: HashSet<String>,
    /// Time when flash items were set
    pub flash_time: Option<Instant>,
    /// VU meter level (0.0 - 1.0)
    pub vu_level: f32,
    /// Maximize log panel (swap main/log areas)
    pub log_maximized: bool,
    /// Log search query
    pub log_search_query: String,
    /// Log search mode active
    pub log_search_mode: bool,
    /// Log scroll position
    pub log_scroll: usize,
    /// Virtual MIDI keyboard
    pub virtual_keyboard: VirtualKeyboard,
    /// JACK MIDI port name for the virtual keyboard (None if JACK not available)
    pub keyboard_port_name: Option<String>,
    /// Whether OS-level keyboard listener is active (for reliable key release)
    pub os_keyboard_active: bool,
    /// Last time we received a terminal event (for focus detection fallback)
    pub last_terminal_event: Option<Instant>,
    /// Whether the terminal explicitly has focus
    pub has_focus: bool,
    /// Whether focus events are supported by the terminal
    pub focus_events_supported: bool,
    /// Script file path
    pub script_path: Option<String>,
    /// Target voice for keyboard input (first voice if not set)
    pub keyboard_target_voice: Option<VoiceId>,
}

impl TuiApp {
    pub fn new() -> Self {
        let mut hierarchy_list_state = ListState::default();
        hierarchy_list_state.select(Some(0));
        Self {
            log_buffer: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            error_message: None,
            show_error_modal: false,
            show_help_modal: false,
            runtime_state: None,
            focused_panel: PanelFocus::Hierarchy,
            hierarchy_selection: 0,
            hierarchy_list_state,
            timeline_offset_beats: 0.0,
            hide_inactive: false,
            pending_seek_beats: 0.0,
            last_seek_time: None,
            is_scrubbing: false,
            collapsed_items: HashSet::new(),
            min_log_level: Level::Trace,
            search_mode: false,
            search_query: String::new(),
            page_size: 10,
            prev_active_items: HashSet::new(),
            flash_items: HashSet::new(),
            flash_time: None,
            vu_level: 0.0,
            log_maximized: false,
            log_search_query: String::new(),
            log_search_mode: false,
            log_scroll: 0,
            virtual_keyboard: VirtualKeyboard::default(),
            keyboard_port_name: None,
            os_keyboard_active: false,
            last_terminal_event: None,
            has_focus: true,
            focus_events_supported: false,
            script_path: None,
            keyboard_target_voice: None,
        }
    }

    /// Set the script file path
    pub fn set_script_path(&mut self, path: String) {
        self.script_path = Some(path);
    }

    /// Set whether OS keyboard listener is active
    pub fn set_os_keyboard_active(&mut self, active: bool) {
        self.os_keyboard_active = active;
    }

    /// Set whether focus events are supported
    pub fn set_focus_events_supported(&mut self, supported: bool) {
        self.focus_events_supported = supported;
    }

    /// Set explicit focus state
    pub fn set_has_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
    }

    /// Mark that we received a terminal event
    pub fn mark_terminal_event(&mut self) {
        self.last_terminal_event = Some(Instant::now());
    }

    /// Toggle collapse state of the selected item
    pub fn toggle_collapse(&mut self) {
        let entries = self.hierarchy_entries();
        if let Some(entry) = entries.get(self.hierarchy_selection) {
            if entry.collapsible {
                if self.collapsed_items.contains(&entry.id) {
                    self.collapsed_items.remove(&entry.id);
                } else {
                    self.collapsed_items.insert(entry.id.clone());
                }
            }
        }
    }

    /// Update state from the runtime
    pub fn update_state(&mut self, state: State) {
        self.runtime_state = Some(state);
        self.sync_selection_bounds();
    }

    /// Process a TUI event
    pub fn process_event(&mut self, event: TuiEvent) {
        match event {
            TuiEvent::Log { level, message } => {
                self.add_log(level, message);
            }
            TuiEvent::Error(msg) => {
                self.error_message = Some(msg.clone());
                self.add_log(Level::Error, format!("ERROR: {}", msg));
            }
        }
    }

    /// Add a log message
    pub fn add_log(&mut self, level: Level, message: String) {
        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        self.log_buffer.push_back(LogEntry {
            timestamp,
            level,
            message,
        });
        if self.log_buffer.len() > MAX_LOG_ENTRIES {
            self.log_buffer.pop_front();
        }
    }

    /// Close error modal
    pub fn close_error_modal(&mut self) {
        self.show_error_modal = false;
    }

    pub fn toggle_hide_inactive(&mut self) {
        self.hide_inactive = !self.hide_inactive;
        self.sync_selection_bounds();
    }

    /// Toggle focus between hierarchy and log
    pub fn toggle_focus(&mut self) {
        self.focused_panel = match self.focused_panel {
            PanelFocus::Hierarchy => PanelFocus::Log,
            PanelFocus::Log => PanelFocus::Hierarchy,
        };
    }

    pub fn move_selection_up(&mut self) {
        if self.focused_panel == PanelFocus::Hierarchy {
            self.hierarchy_selection = self.hierarchy_selection.saturating_sub(1);
            self.hierarchy_list_state
                .select(Some(self.hierarchy_selection));
        }
    }

    pub fn move_selection_down(&mut self) {
        if self.focused_panel == PanelFocus::Hierarchy {
            let len = self.hierarchy_entries().len();
            if len == 0 {
                self.hierarchy_selection = 0;
            } else if self.hierarchy_selection + 1 < len {
                self.hierarchy_selection += 1;
            }
            self.hierarchy_list_state
                .select(Some(self.hierarchy_selection));
        }
    }

    /// Move selection up by page
    pub fn move_selection_page_up(&mut self) {
        if self.focused_panel == PanelFocus::Hierarchy {
            self.hierarchy_selection = self.hierarchy_selection.saturating_sub(self.page_size);
            self.hierarchy_list_state
                .select(Some(self.hierarchy_selection));
        } else if self.focused_panel == PanelFocus::Log {
            self.log_scroll = self.log_scroll.saturating_sub(self.page_size);
        }
    }

    /// Move selection down by page
    pub fn move_selection_page_down(&mut self) {
        if self.focused_panel == PanelFocus::Hierarchy {
            let len = self.hierarchy_entries().len();
            if len > 0 {
                self.hierarchy_selection = (self.hierarchy_selection + self.page_size).min(len - 1);
            }
            self.hierarchy_list_state
                .select(Some(self.hierarchy_selection));
        } else if self.focused_panel == PanelFocus::Log {
            let filtered_len = self.filtered_log_entries().len();
            if filtered_len > 0 {
                self.log_scroll =
                    (self.log_scroll + self.page_size).min(filtered_len.saturating_sub(1));
            }
        }
    }

    /// Toggle help modal
    pub fn toggle_help_modal(&mut self) {
        self.show_help_modal = !self.show_help_modal;
    }

    /// Expand all collapsible items
    pub fn expand_all(&mut self) {
        self.collapsed_items.clear();
    }

    /// Collapse all collapsible items
    pub fn collapse_all(&mut self) {
        let entries = self.hierarchy_entries();
        for entry in entries {
            if entry.collapsible {
                self.collapsed_items.insert(entry.id.clone());
            }
        }
        self.sync_selection_bounds();
    }

    /// Set specific log level by number (1-5)
    pub fn set_log_level(&mut self, level: u8) {
        self.min_log_level = match level {
            1 => Level::Error,
            2 => Level::Warn,
            3 => Level::Info,
            4 => Level::Debug,
            _ => Level::Trace,
        };
    }

    /// Enter search mode for hierarchy
    pub fn enter_search_mode(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
    }

    /// Exit search mode
    pub fn exit_search_mode(&mut self) {
        self.search_mode = false;
        self.log_search_mode = false;
    }

    /// Add character to search query
    pub fn search_push_char(&mut self, c: char) {
        if self.search_mode {
            self.search_query.push(c);
        } else if self.log_search_mode {
            self.log_search_query.push(c);
        }
    }

    /// Remove character from search query
    pub fn search_pop_char(&mut self) {
        if self.search_mode {
            self.search_query.pop();
        } else if self.log_search_mode {
            self.log_search_query.pop();
        }
    }

    /// Get filtered hierarchy entries based on search query
    pub fn filtered_hierarchy_entries(&self) -> Vec<HierarchyEntry> {
        let entries = self.hierarchy_entries();
        if self.search_query.is_empty() {
            return entries;
        }
        let query = self.search_query.to_lowercase();
        entries
            .into_iter()
            .filter(|e| {
                e.label.to_lowercase().contains(&query) || e.detail.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Toggle log maximized view
    pub fn toggle_log_maximized(&mut self) {
        self.log_maximized = !self.log_maximized;
    }

    /// Enter log search mode
    pub fn enter_log_search_mode(&mut self) {
        self.log_search_mode = true;
        self.log_search_query.clear();
        self.focused_panel = PanelFocus::Log;
    }

    /// Get filtered log entries
    pub fn filtered_log_entries(&self) -> Vec<&LogEntry> {
        self.log_buffer
            .iter()
            .filter(|entry| {
                let level_ok = match self.min_log_level {
                    Level::Error => entry.level == Level::Error,
                    Level::Warn => entry.level == Level::Error || entry.level == Level::Warn,
                    Level::Info => entry.level != Level::Debug && entry.level != Level::Trace,
                    Level::Debug => entry.level != Level::Trace,
                    Level::Trace => true,
                };
                let search_ok = self.log_search_query.is_empty()
                    || entry
                        .message
                        .to_lowercase()
                        .contains(&self.log_search_query.to_lowercase());
                level_ok && search_ok
            })
            .collect()
    }

    /// Update flash tracking based on state changes
    pub fn update_flash_tracking(&mut self) {
        if let Some(flash_time) = self.flash_time {
            if flash_time.elapsed() > Duration::from_millis(500) {
                self.flash_items.clear();
                self.flash_time = None;
            }
        }

        let entries = self.hierarchy_entries();
        let current_active: HashSet<String> = entries
            .iter()
            .filter(|e| e.active)
            .map(|e| e.id.clone())
            .collect();

        let newly_active: HashSet<String> = current_active
            .difference(&self.prev_active_items)
            .cloned()
            .collect();
        let newly_inactive: HashSet<String> = self
            .prev_active_items
            .difference(&current_active)
            .cloned()
            .collect();

        if !newly_active.is_empty() || !newly_inactive.is_empty() {
            self.flash_items = newly_active.union(&newly_inactive).cloned().collect();
            self.flash_time = Some(Instant::now());
        }

        self.prev_active_items = current_active;
    }

    /// Check if currently in any search/input mode
    pub fn in_input_mode(&self) -> bool {
        self.search_mode || self.log_search_mode
    }

    /// Check if the virtual keyboard is active and capturing keys
    pub fn keyboard_active(&self) -> bool {
        self.virtual_keyboard.visible
    }

    /// Get the effective keyboard target voice
    /// Returns the explicitly set voice, or falls back to the first available voice
    pub fn get_keyboard_target_voice(&self) -> Option<VoiceId> {
        if let Some(voice_id) = self.keyboard_target_voice {
            // Verify the voice still exists
            if let Some(state) = &self.runtime_state {
                if state.voices.contains_key(&voice_id) {
                    return Some(voice_id);
                }
            }
        }
        // Fall back to first voice
        if let Some(state) = &self.runtime_state {
            state.voices.keys().next().copied()
        } else {
            None
        }
    }

    /// Set the keyboard target voice from the currently selected hierarchy item
    pub fn set_keyboard_target_from_selection(&mut self) {
        let entries = self.hierarchy_entries();
        if let Some(entry) = entries.get(self.hierarchy_selection) {
            if entry.kind == HierarchyKind::Voice {
                // Parse voice ID from entry.id (format: "voice:{id}")
                if let Some(id_str) = entry.id.strip_prefix("voice:") {
                    if let Ok(raw_id) = id_str.parse::<u32>() {
                        self.keyboard_target_voice = Some(VoiceId::new(raw_id));
                    }
                }
            }
        }
    }

    /// Get the name of the keyboard target voice for display
    pub fn keyboard_target_voice_name(&self) -> String {
        if let Some(voice_id) = self.get_keyboard_target_voice() {
            if let Some(state) = &self.runtime_state {
                if let Some(voice_state) = state.voices.get(&voice_id) {
                    if !voice_state.config.name.is_empty() {
                        return voice_state.config.name.clone();
                    }
                }
            }
            voice_id.to_string()
        } else {
            "none".to_string()
        }
    }

    /// Add to the pending seek offset (debounced - actual seek happens later)
    pub fn add_pending_seek(&mut self, delta: f64) {
        self.pending_seek_beats += delta;
        self.last_seek_time = Some(Instant::now());
        self.is_scrubbing = true;
        self.timeline_offset_beats += delta;
    }

    /// Check if the debounce period has passed and return the pending seek offset
    pub fn check_seek_debounce(&mut self) -> Option<f64> {
        if !self.is_scrubbing {
            return None;
        }

        if let Some(last_time) = self.last_seek_time {
            if last_time.elapsed() >= Duration::from_millis(SEEK_DEBOUNCE_MS) {
                let offset = self.pending_seek_beats;
                self.pending_seek_beats = 0.0;
                self.last_seek_time = None;
                self.is_scrubbing = false;
                self.timeline_offset_beats = 0.0;
                return Some(offset);
            }
        }
        None
    }

    /// Cancel pending seek and reset state
    pub fn cancel_pending_seek(&mut self) {
        self.pending_seek_beats = 0.0;
        self.last_seek_time = None;
        self.is_scrubbing = false;
        self.timeline_offset_beats = 0.0;
    }

    /// Build hierarchy entries from vibelang-core state
    pub fn hierarchy_entries(&self) -> Vec<HierarchyEntry> {
        if let Some(state) = &self.runtime_state {
            build_hierarchy_entries(state, &self.collapsed_items)
        } else {
            vec![]
        }
    }

    /// Get summary stats from state
    pub fn summary_stats(&self) -> SummaryStats {
        if let Some(state) = &self.runtime_state {
            SummaryStats::from_state(state)
        } else {
            SummaryStats::default()
        }
    }

    /// Get resource stats from state
    pub fn resource_stats(&self) -> ResourceStats {
        if let Some(state) = &self.runtime_state {
            ResourceStats::from_state(state)
        } else {
            ResourceStats::default()
        }
    }

    /// Get beat position information
    pub fn get_beat_info(&self) -> BeatInfo {
        if let Some(state) = &self.runtime_state {
            let beats_per_bar = state.time_sig.beats_per_bar();
            let current_beat = state.current_beat.to_f64();
            let current_bar = (current_beat / beats_per_bar).floor() as i64;
            let beat_in_bar = current_beat % beats_per_bar;
            let beat_number_in_bar = (beat_in_bar.floor() as i64) + 1;
            let total_beats_in_bar = state.time_sig.numerator as i64;

            BeatInfo {
                bar_number: current_bar + 1,
                beat_in_bar,
                beat_number_in_bar,
                total_beats_in_bar,
                bpm: state.tempo,
                time_signature: format!(
                    "{}/{}",
                    state.time_sig.numerator, state.time_sig.denominator
                ),
                running: state.playing,
            }
        } else {
            BeatInfo::default()
        }
    }

    fn sync_selection_bounds(&mut self) {
        let hierarchy_len = self.hierarchy_entries().len();
        if hierarchy_len == 0 {
            self.hierarchy_selection = 0;
        } else if self.hierarchy_selection >= hierarchy_len {
            self.hierarchy_selection = hierarchy_len.saturating_sub(1);
        }
        self.hierarchy_list_state
            .select(Some(self.hierarchy_selection));
    }
}

impl Default for TuiApp {
    fn default() -> Self {
        Self::new()
    }
}

/// Beat position information for display
#[derive(Default)]
pub struct BeatInfo {
    pub bar_number: i64,
    pub beat_in_bar: f64,
    pub beat_number_in_bar: i64,
    pub total_beats_in_bar: i64,
    pub bpm: f64,
    pub time_signature: String,
    pub running: bool,
}

/// Panel focus - only Hierarchy and Log
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelFocus {
    Hierarchy,
    Log,
}

#[derive(Clone, Default)]
pub struct SummaryStats {
    pub patterns_playing: usize,
    pub patterns_total: usize,
    pub melodies_playing: usize,
    pub melodies_total: usize,
    pub sequences_playing: usize,
    pub sequences_total: usize,
}

impl SummaryStats {
    pub fn from_state(state: &State) -> Self {
        let patterns_playing = state.patterns.values().filter(|p| p.playing).count();
        let melodies_playing = state.melodies.values().filter(|m| m.playing).count();
        let sequences_playing = state.sequences.values().filter(|s| s.playing).count();

        Self {
            patterns_playing,
            patterns_total: state.patterns.len(),
            melodies_playing,
            melodies_total: state.melodies.len(),
            sequences_playing,
            sequences_total: state.sequences.len(),
        }
    }
}

#[derive(Clone, Default)]
pub struct ResourceStats {
    pub groups: usize,
    pub voices: usize,
    pub effects: usize,
    pub samples: usize,
    pub buffers_allocated: u32,
}

impl ResourceStats {
    pub fn from_state(state: &State) -> Self {
        Self {
            groups: state.groups.len(),
            voices: state.voices.len(),
            effects: state.effects.len(),
            samples: state.samples.len(),
            buffers_allocated: state.buffer_ids.allocated_count(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: Level,
    pub message: String,
}

#[derive(Clone)]
pub struct HierarchyEntry {
    pub id: String,
    pub depth: usize,
    pub label: String,
    pub detail: String,
    pub params: Vec<(String, String)>,
    pub kind: HierarchyKind,
    pub active: bool,
    pub collapsible: bool,
    pub collapsed: bool,
}

impl HierarchyEntry {
    pub fn color(&self) -> Color {
        match self.kind {
            HierarchyKind::Group => Color::Cyan,
            HierarchyKind::Voice => Color::LightBlue,
            HierarchyKind::Pattern => Color::Green,
            HierarchyKind::Melody => Color::Magenta,
            HierarchyKind::Effect => Color::Yellow,
            HierarchyKind::Sequence => Color::LightCyan,
            HierarchyKind::Section => Color::Gray,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HierarchyKind {
    Group,
    Voice,
    Pattern,
    Melody,
    Effect,
    Sequence,
    Section,
}

/// Build hierarchy entries from vibelang-core State
fn build_hierarchy_entries(state: &State, collapsed: &HashSet<String>) -> Vec<HierarchyEntry> {
    let mut entries = Vec::new();

    // Group groups by parent
    let mut root_groups: Vec<GroupId> = Vec::new();
    let mut child_groups: std::collections::HashMap<GroupId, Vec<GroupId>> =
        std::collections::HashMap::new();

    for (id, group_state) in &state.groups {
        if let Some(parent) = group_state.parent {
            child_groups.entry(parent).or_default().push(*id);
        } else {
            root_groups.push(*id);
        }
    }

    // Sort root groups by ID
    root_groups.sort_by_key(|id| id.raw());

    // Build entries recursively
    for group_id in root_groups {
        collect_group_entries(&group_id, 0, &mut entries, state, &child_groups, collapsed);
    }

    // Add sequences section if there are any
    if !state.sequences.is_empty() {
        let seq_collapsed = collapsed.contains("section:Sequences");
        entries.push(HierarchyEntry {
            id: "section:Sequences".to_string(),
            depth: 0,
            label: "Sequences".to_string(),
            detail: format!("{} total", state.sequences.len()),
            params: Vec::new(),
            kind: HierarchyKind::Section,
            active: false,
            collapsible: true,
            collapsed: seq_collapsed,
        });

        if !seq_collapsed {
            let mut seq_ids: Vec<_> = state.sequences.keys().collect();
            seq_ids.sort_by_key(|id| id.raw());

            for seq_id in seq_ids {
                if let Some(seq_state) = state.sequences.get(seq_id) {
                    let status = if seq_state.paused {
                        "paused"
                    } else if seq_state.playing {
                        "\u{25B6}" // ▶
                    } else {
                        "\u{23F8}" // ⏸
                    };

                    entries.push(HierarchyEntry {
                        id: format!("seq:{}", seq_id),
                        depth: 1,
                        label: seq_id.to_string(),
                        detail: format!("{:.1}b {}", seq_state.config.length.to_f64(), status),
                        params: Vec::new(),
                        kind: HierarchyKind::Sequence,
                        active: seq_state.playing && !seq_state.paused,
                        collapsible: false,
                        collapsed: false,
                    });
                }
            }
        }
    }

    if entries.is_empty() {
        entries.push(HierarchyEntry {
            id: "section:Empty".to_string(),
            depth: 0,
            label: "No groups defined".to_string(),
            detail: String::new(),
            params: Vec::new(),
            kind: HierarchyKind::Section,
            active: false,
            collapsible: false,
            collapsed: false,
        });
    }

    entries
}

fn collect_group_entries(
    group_id: &GroupId,
    depth: usize,
    entries: &mut Vec<HierarchyEntry>,
    state: &State,
    child_groups: &std::collections::HashMap<GroupId, Vec<GroupId>>,
    collapsed: &HashSet<String>,
) {
    if let Some(group_state) = state.groups.get(group_id) {
        let entry_id = format!("group:{}", group_id);
        let is_collapsed = collapsed.contains(&entry_id);

        // Build group detail
        let mut detail_parts = Vec::new();
        if group_state.muted {
            detail_parts.push("muted".to_string());
        }
        if group_state.soloed {
            detail_parts.push("solo".to_string());
        }

        // Build params
        let params: Vec<(String, String)> = group_state
            .params
            .iter()
            .map(|(k, v)| (k.clone(), format_param_value(*v)))
            .collect();

        entries.push(HierarchyEntry {
            id: entry_id.clone(),
            depth,
            label: group_id.to_string(),
            detail: detail_parts.join(" \u{2022} "), // •
            params,
            kind: HierarchyKind::Group,
            active: !group_state.muted,
            collapsible: true,
            collapsed: is_collapsed,
        });

        if !is_collapsed {
            // Add voices in this group
            let mut voice_ids: Vec<_> = state
                .voices
                .iter()
                .filter(|(_, v)| v.config.group == *group_id)
                .map(|(id, _)| id)
                .collect();
            voice_ids.sort_by_key(|id| id.raw());

            for voice_id in voice_ids {
                if let Some(voice_state) = state.voices.get(voice_id) {
                    let mut detail_parts = Vec::new();
                    detail_parts.push(voice_state.config.synthdef.clone());
                    if voice_state.config.muted {
                        detail_parts.push("muted".to_string());
                    }
                    if voice_state.config.polyphony > 1 {
                        detail_parts.push(format!("poly:{}", voice_state.config.polyphony));
                    }

                    let params: Vec<(String, String)> = voice_state
                        .config
                        .params
                        .iter()
                        .map(|(k, v)| (k.clone(), format_param_value(*v)))
                        .collect();

                    entries.push(HierarchyEntry {
                        id: format!("voice:{}", voice_id),
                        depth: depth + 1,
                        label: if voice_state.config.name.is_empty() {
                            voice_id.to_string()
                        } else {
                            voice_state.config.name.clone()
                        },
                        detail: detail_parts.join(" \u{2022} "),
                        params,
                        kind: HierarchyKind::Voice,
                        active: !voice_state.config.muted,
                        collapsible: false,
                        collapsed: false,
                    });
                }
            }

            // Add patterns targeting voices in this group
            let mut pattern_ids: Vec<_> = state
                .patterns
                .iter()
                .filter(|(_, p)| {
                    p.content
                        .voice
                        .as_ref()
                        .and_then(|vid| state.voices.get(vid))
                        .map(|v| v.config.group == *group_id)
                        .unwrap_or(false)
                })
                .map(|(id, _)| id)
                .collect();
            pattern_ids.sort_by_key(|id| id.raw());

            for pattern_id in pattern_ids {
                if let Some(pattern_state) = state.patterns.get(pattern_id) {
                    let status = if pattern_state.playing {
                        "\u{25B6}"
                    } else {
                        "\u{23F8}"
                    };
                    let voice_name = pattern_state
                        .content
                        .voice
                        .and_then(|vid| {
                            state.voices.get(&vid).map(|v| {
                                if v.config.name.is_empty() {
                                    vid.to_string()
                                } else {
                                    v.config.name.clone()
                                }
                            })
                        })
                        .unwrap_or_else(|| "?".to_string());

                    let label = if pattern_state.content.name.is_empty() {
                        pattern_id.to_string()
                    } else {
                        pattern_state.content.name.clone()
                    };

                    entries.push(HierarchyEntry {
                        id: format!("pattern:{}", pattern_id),
                        depth: depth + 1,
                        label,
                        detail: format!("{} \u{2192}{}", status, voice_name),
                        params: Vec::new(),
                        kind: HierarchyKind::Pattern,
                        active: pattern_state.playing,
                        collapsible: false,
                        collapsed: false,
                    });
                }
            }

            // Add melodies targeting voices in this group
            let mut melody_ids: Vec<_> = state
                .melodies
                .iter()
                .filter(|(_, m)| {
                    m.content
                        .voice
                        .as_ref()
                        .and_then(|vid| state.voices.get(vid))
                        .map(|v| v.config.group == *group_id)
                        .unwrap_or(false)
                })
                .map(|(id, _)| id)
                .collect();
            melody_ids.sort_by_key(|id| id.raw());

            for melody_id in melody_ids {
                if let Some(melody_state) = state.melodies.get(melody_id) {
                    let status = if melody_state.playing {
                        "\u{25B6}"
                    } else {
                        "\u{23F8}"
                    };
                    let voice_name = melody_state
                        .content
                        .voice
                        .and_then(|vid| {
                            state.voices.get(&vid).map(|v| {
                                if v.config.name.is_empty() {
                                    vid.to_string()
                                } else {
                                    v.config.name.clone()
                                }
                            })
                        })
                        .unwrap_or_else(|| "?".to_string());

                    let label = if melody_state.content.name.is_empty() {
                        melody_id.to_string()
                    } else {
                        melody_state.content.name.clone()
                    };

                    entries.push(HierarchyEntry {
                        id: format!("melody:{}", melody_id),
                        depth: depth + 1,
                        label,
                        detail: format!("{} \u{2192}{}", status, voice_name),
                        params: Vec::new(),
                        kind: HierarchyKind::Melody,
                        active: melody_state.playing,
                        collapsible: false,
                        collapsed: false,
                    });
                }
            }

            // Add effects in this group
            let mut effect_ids: Vec<_> = state
                .effects
                .iter()
                .filter(|(_, e)| e.group == *group_id)
                .map(|(id, _)| id)
                .collect();
            effect_ids.sort_by_key(|id| id.raw());

            for effect_id in effect_ids {
                if let Some(effect_state) = state.effects.get(effect_id) {
                    let params: Vec<(String, String)> = effect_state
                        .params
                        .iter()
                        .map(|(k, v)| (k.clone(), format_param_value(*v)))
                        .collect();

                    entries.push(HierarchyEntry {
                        id: format!("effect:{}", effect_id),
                        depth: depth + 1,
                        label: effect_id.to_string(),
                        detail: effect_state.synthdef.clone(),
                        params,
                        kind: HierarchyKind::Effect,
                        active: true,
                        collapsible: false,
                        collapsed: false,
                    });
                }
            }

            // Recursively add child groups
            if let Some(children) = child_groups.get(group_id) {
                let mut sorted_children = children.clone();
                sorted_children.sort_by_key(|id| id.raw());
                for child_id in sorted_children {
                    collect_group_entries(
                        &child_id,
                        depth + 1,
                        entries,
                        state,
                        child_groups,
                        collapsed,
                    );
                }
            }
        }
    }
}

/// Format a parameter value for display
fn format_param_value(v: f32) -> String {
    if v.abs() < 0.0001 {
        "0".to_string()
    } else if v.abs() >= 1000.0 {
        format!("{:.0}", v)
    } else if v.abs() >= 10.0 {
        format!("{:.1}", v)
    } else if v.abs() >= 1.0 {
        format!("{:.2}", v)
    } else {
        format!("{:.3}", v)
    }
}
