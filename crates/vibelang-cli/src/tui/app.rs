//! TUI application state and logic

use vibelang_core::sequences::ClipSource;
use vibelang_core::state::{
    EffectState, GroupState, LoopStatus, MelodyState, PatternState, ScriptState, VoiceState,
};
use crate::tui::TuiEvent;
use log::Level;
use ratatui::style::Color;
use ratatui::widgets::ListState;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

const MAX_LOG_ENTRIES: usize = 100;

/// Debounce duration for transport seeking (in milliseconds)
const SEEK_DEBOUNCE_MS: u64 = 250;

/// Main TUI application state - simplified for unified hierarchy view
pub struct TuiApp {
    /// Log messages buffer
    pub log_buffer: VecDeque<LogEntry>,
    /// Current error message (if any)
    pub error_message: Option<String>,
    /// Show error modal
    pub show_error_modal: bool,
    /// Show help modal
    pub show_help_modal: bool,
    /// Cached state for rendering
    pub state: Option<ScriptState>,
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
            state: None,
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
        }
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

    /// Check if an item is collapsed
    pub fn is_collapsed(&self, id: &str) -> bool {
        self.collapsed_items.contains(id)
    }

    /// Update state from the state manager
    pub fn update_state(&mut self, state: ScriptState) {
        self.state = Some(state);
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
            TuiEvent::ClearError => {
                self.error_message = None;
                self.show_error_modal = false;
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

    /// Toggle error modal
    pub fn toggle_error_modal(&mut self) {
        if self.error_message.is_some() {
            self.show_error_modal = !self.show_error_modal;
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

    pub fn focus_hierarchy(&mut self) {
        self.focused_panel = PanelFocus::Hierarchy;
    }

    pub fn focus_log(&mut self) {
        self.focused_panel = PanelFocus::Log;
    }

    pub fn move_selection_up(&mut self) {
        if self.focused_panel == PanelFocus::Hierarchy {
            self.hierarchy_selection = self.hierarchy_selection.saturating_sub(1);
            self.hierarchy_list_state.select(Some(self.hierarchy_selection));
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
            self.hierarchy_list_state.select(Some(self.hierarchy_selection));
        }
    }

    /// Move selection up by page
    pub fn move_selection_page_up(&mut self) {
        if self.focused_panel == PanelFocus::Hierarchy {
            self.hierarchy_selection = self.hierarchy_selection.saturating_sub(self.page_size);
            self.hierarchy_list_state.select(Some(self.hierarchy_selection));
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
            self.hierarchy_list_state.select(Some(self.hierarchy_selection));
        } else if self.focused_panel == PanelFocus::Log {
            let filtered_len = self.filtered_log_entries().len();
            if filtered_len > 0 {
                self.log_scroll = (self.log_scroll + self.page_size).min(filtered_len.saturating_sub(1));
            }
        }
    }

    /// Toggle help modal
    pub fn toggle_help_modal(&mut self) {
        self.show_help_modal = !self.show_help_modal;
    }

    /// Jump to first active pattern/melody
    pub fn jump_to_active(&mut self) {
        let entries = self.hierarchy_entries();
        for (idx, entry) in entries.iter().enumerate() {
            if entry.active && matches!(entry.kind, HierarchyKind::Pattern | HierarchyKind::Melody) {
                self.hierarchy_selection = idx;
                self.hierarchy_list_state.select(Some(idx));
                break;
            }
        }
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

    /// Cycle minimum log level
    pub fn cycle_log_level(&mut self) {
        self.min_log_level = match self.min_log_level {
            Level::Error => Level::Warn,
            Level::Warn => Level::Info,
            Level::Info => Level::Debug,
            Level::Debug => Level::Trace,
            Level::Trace => Level::Error,
        };
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
            .filter(|e| e.label.to_lowercase().contains(&query) || e.detail.to_lowercase().contains(&query))
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
                // Filter by level
                let level_ok = match self.min_log_level {
                    Level::Error => entry.level == Level::Error,
                    Level::Warn => entry.level == Level::Error || entry.level == Level::Warn,
                    Level::Info => entry.level != Level::Debug && entry.level != Level::Trace,
                    Level::Debug => entry.level != Level::Trace,
                    Level::Trace => true,
                };
                // Filter by search query
                let search_ok = self.log_search_query.is_empty()
                    || entry.message.to_lowercase().contains(&self.log_search_query.to_lowercase());
                level_ok && search_ok
            })
            .collect()
    }

    /// Update flash tracking based on state changes
    pub fn update_flash_tracking(&mut self) {
        // Clear old flashes after 500ms
        if let Some(flash_time) = self.flash_time {
            if flash_time.elapsed() > Duration::from_millis(500) {
                self.flash_items.clear();
                self.flash_time = None;
            }
        }

        // Get current active items
        let entries = self.hierarchy_entries();
        let current_active: HashSet<String> = entries
            .iter()
            .filter(|e| e.active)
            .map(|e| e.id.clone())
            .collect();

        // Find items that changed state
        let newly_active: HashSet<String> = current_active.difference(&self.prev_active_items).cloned().collect();
        let newly_inactive: HashSet<String> = self.prev_active_items.difference(&current_active).cloned().collect();

        if !newly_active.is_empty() || !newly_inactive.is_empty() {
            self.flash_items = newly_active.union(&newly_inactive).cloned().collect();
            self.flash_time = Some(Instant::now());
        }

        self.prev_active_items = current_active;
    }

    /// Check if an item should flash
    pub fn should_flash(&self, id: &str) -> bool {
        self.flash_items.contains(id)
    }

    /// Update VU meter level from state
    pub fn update_vu_level(&mut self) {
        if let Some(state) = &self.state {
            // Estimate level from max voice gain
            let max_gain = state.voices.values()
                .map(|v| v.gain as f32)
                .fold(0.0f32, |a, b| a.max(b));
            // Smooth the VU meter
            self.vu_level = self.vu_level * 0.7 + max_gain.min(1.0) * 0.3;
        }
    }

    /// Check if currently in any search/input mode
    pub fn in_input_mode(&self) -> bool {
        self.search_mode || self.log_search_mode
    }

    /// Add to the pending seek offset (debounced - actual seek happens later)
    pub fn add_pending_seek(&mut self, delta: f64) {
        self.pending_seek_beats += delta;
        self.last_seek_time = Some(Instant::now());
        self.is_scrubbing = true;
        // Update timeline offset for visual preview
        self.timeline_offset_beats += delta;
    }

    /// Check if the debounce period has passed and return the pending seek offset
    /// Returns Some(offset) if ready to apply, None if still waiting
    pub fn check_seek_debounce(&mut self) -> Option<f64> {
        if !self.is_scrubbing {
            return None;
        }

        if let Some(last_time) = self.last_seek_time {
            if last_time.elapsed() >= Duration::from_millis(SEEK_DEBOUNCE_MS) {
                let offset = self.pending_seek_beats;
                // Reset scrubbing state
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

    /// Legacy method for immediate time scroll (used for UI preview)
    pub fn scroll_time(&mut self, delta: f64) {
        self.timeline_offset_beats += delta;
    }

    pub fn reset_time_scroll(&mut self) {
        self.timeline_offset_beats = 0.0;
    }

    pub fn sequence_entries(&self) -> Vec<SequenceDisplay> {
        if let Some(state) = &self.state {
            let view_beat = state.current_beat + self.timeline_offset_beats;
            let mut entries: Vec<_> = state
                .sequences
                .iter()
                .map(|(name, sequence)| {
                    let active = state.active_sequences.get(name);
                    let loop_beats = sequence.loop_beats.max(0.001);

                    // Always use transport-relative timing for consistent visualization
                    // All sequences sync to the global transport beat
                    let elapsed_beats = view_beat.max(0.0);

                    // Position within current loop (wrapped)
                    let position = elapsed_beats.rem_euclid(loop_beats);

                    let clips = sequence
                        .clips
                        .iter()
                        .map(SequenceClipDisplay::from_clip)
                        .collect();

                    SequenceDisplay {
                        name: name.to_string(),
                        loop_beats,
                        clips,
                        position,
                        elapsed_beats,
                        playing: active.is_some(),
                        paused: active.map(|s| s.paused).unwrap_or(false),
                    }
                })
                .collect();

            entries.sort_by(|a, b| a.name.cmp(&b.name));
            entries
        } else {
            vec![]
        }
    }

    pub fn hierarchy_entries(&self) -> Vec<HierarchyEntry> {
        if let Some(state) = &self.state {
            build_hierarchy_entries(state, &self.collapsed_items)
        } else {
            vec![]
        }
    }

    pub fn summary_stats(&self) -> SummaryStats {
        if let Some(state) = &self.state {
            SummaryStats::from_state(state)
        } else {
            SummaryStats::default()
        }
    }

    pub fn queue_metrics(&self) -> QueueMetrics {
        if let Some(state) = &self.state {
            QueueMetrics::from_state(state)
        } else {
            QueueMetrics::default()
        }
    }

    pub fn resource_stats(&self) -> ResourceStats {
        if let Some(state) = &self.state {
            ResourceStats::from_state(state)
        } else {
            ResourceStats::default()
        }
    }

    fn sync_selection_bounds(&mut self) {
        let hierarchy_len = self.hierarchy_entries().len();
        if hierarchy_len == 0 {
            self.hierarchy_selection = 0;
        } else if self.hierarchy_selection >= hierarchy_len {
            self.hierarchy_selection = hierarchy_len.saturating_sub(1);
        }
        self.hierarchy_list_state.select(Some(self.hierarchy_selection));
    }

    /// Get beat position information
    pub fn get_beat_info(&self) -> BeatInfo {
        if let Some(state) = &self.state {
            let beats_per_bar = state.time_signature.beats_per_bar();
            let current_bar = (state.current_beat / beats_per_bar).floor() as i64;
            let beat_in_bar = state.current_beat % beats_per_bar;

            // Calculate which beat number we're on (1, 2, 3, 4 etc)
            let beat_number_in_bar = (beat_in_bar.floor() as i64) + 1;
            let total_beats_in_bar = state.time_signature.numerator as i64;

            // Create visual beat indicator
            let num_beats = state.time_signature.numerator as usize;
            let current_beat_index = beat_in_bar.floor() as usize;
            let beat_fraction = beat_in_bar.fract();

            let mut beat_indicator = String::new();
            for i in 0..num_beats {
                if i == current_beat_index {
                    // Show fraction of current beat
                    if beat_fraction < 0.25 {
                        beat_indicator.push('█');
                    } else if beat_fraction < 0.5 {
                        beat_indicator.push('▓');
                    } else if beat_fraction < 0.75 {
                        beat_indicator.push('▒');
                    } else {
                        beat_indicator.push('░');
                    }
                } else if i < current_beat_index {
                    beat_indicator.push('▪');
                } else {
                    beat_indicator.push('·');
                }
                beat_indicator.push(' ');
            }

            BeatInfo {
                current_beat: state.current_beat,
                bar_number: current_bar + 1,
                beat_in_bar,
                beat_number_in_bar,
                total_beats_in_bar,
                bpm: state.tempo,
                time_signature: format!(
                    "{}/{}",
                    state.time_signature.numerator, state.time_signature.denominator
                ),
                running: state.transport_running,
                beat_indicator,
            }
        } else {
            BeatInfo::default()
        }
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
    pub current_beat: f64,
    pub bar_number: i64,
    pub beat_in_bar: f64,
    pub beat_number_in_bar: i64, // 1, 2, 3, 4 etc
    pub total_beats_in_bar: i64,
    pub bpm: f64,
    pub time_signature: String,
    pub running: bool,
    pub beat_indicator: String,
}

/// Simplified panel focus - only Hierarchy and Log
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelFocus {
    Hierarchy,
    Log,
}

#[derive(Clone, Default)]
pub struct SummaryStats {
    pub patterns_playing: usize,
    pub patterns_queued: usize,
    pub patterns_total: usize,
    pub melodies_playing: usize,
    pub melodies_queued: usize,
    pub melodies_total: usize,
    pub avg_voice_gain: f32,
    pub max_voice_gain: f32,
}

impl SummaryStats {
    pub fn from_state(state: &ScriptState) -> Self {
        let mut stats = SummaryStats::default();
        stats.patterns_total = state.patterns.len();
        stats.melodies_total = state.melodies.len();

        if !state.voices.is_empty() {
            let mut sum = 0.0f32;
            let mut max_gain = 0.0f32;
            for voice in state.voices.values() {
                let gain = voice.gain as f32;
                sum += gain;
                if gain > max_gain {
                    max_gain = gain;
                }
            }
            stats.avg_voice_gain = sum / state.voices.len() as f32;
            stats.max_voice_gain = max_gain;
        }

        for pattern in state.patterns.values() {
            match pattern.status {
                LoopStatus::Playing { .. } | LoopStatus::QueuedStop { .. } => {
                    stats.patterns_playing += 1;
                }
                LoopStatus::Queued { .. } => stats.patterns_queued += 1,
                LoopStatus::Stopped => {}
            }
        }

        for melody in state.melodies.values() {
            match melody.status {
                LoopStatus::Playing { .. } | LoopStatus::QueuedStop { .. } => {
                    stats.melodies_playing += 1;
                }
                LoopStatus::Queued { .. } => stats.melodies_queued += 1,
                LoopStatus::Stopped => {}
            }
        }

        stats
    }
}

#[derive(Clone, Default)]
pub struct QueueMetrics {
    pub active_sequences: usize,
    pub upcoming_event_beat: Option<f64>,
    pub playing_patterns: usize,
    pub queued_patterns: usize,
    pub playing_melodies: usize,
    pub queued_melodies: usize,
}

impl QueueMetrics {
    pub fn from_state(state: &ScriptState) -> Self {
        let mut metrics = QueueMetrics::default();
        metrics.active_sequences = state.active_sequences.len();
        metrics.upcoming_event_beat = state
            .scheduled_events
            .iter()
            .map(|evt| evt.beat)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        for pattern in state.patterns.values() {
            match pattern.status {
                LoopStatus::Playing { .. } | LoopStatus::QueuedStop { .. } => {
                    metrics.playing_patterns += 1;
                }
                LoopStatus::Queued { .. } => metrics.queued_patterns += 1,
                LoopStatus::Stopped => {}
            }
        }

        for melody in state.melodies.values() {
            match melody.status {
                LoopStatus::Playing { .. } | LoopStatus::QueuedStop { .. } => {
                    metrics.playing_melodies += 1;
                }
                LoopStatus::Queued { .. } => metrics.queued_melodies += 1,
                LoopStatus::Stopped => {}
            }
        }

        metrics
    }
}

#[derive(Clone, Default)]
pub struct ResourceStats {
    pub active_synths: usize,
    pub groups: usize,
    pub voices: usize,
    pub effects: usize,
    pub buffers_used: i32,
    pub buses_used: i32,
    pub samples: usize,
}

impl ResourceStats {
    pub fn from_state(state: &ScriptState) -> Self {
        Self {
            active_synths: state.active_synths.len(),
            groups: state.groups.len(),
            voices: state.voices.len(),
            effects: state.effects.len(),
            buffers_used: state.next_buffer_id - 100,
            buses_used: state.next_audio_bus - 64,
            samples: state.samples.len(),
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
pub struct SequenceDisplay {
    pub name: String,
    pub loop_beats: f64,
    pub clips: Vec<SequenceClipDisplay>,
    pub position: f64,           // Position within current loop (0 to loop_beats)
    pub elapsed_beats: f64,      // Total elapsed beats for multi-loop display
    pub playing: bool,
    pub paused: bool,
}

impl SequenceDisplay {
    pub fn status(&self) -> &'static str {
        if self.paused {
            "paused"
        } else if self.playing {
            "playing"
        } else {
            "idle"
        }
    }

    pub fn status_color(&self) -> Color {
        if self.paused {
            Color::Yellow
        } else if self.playing {
            Color::Green
        } else {
            Color::DarkGray
        }
    }

    pub fn position_ratio(&self) -> f64 {
        if self.loop_beats <= f64::EPSILON {
            0.0
        } else {
            (self.position / self.loop_beats).clamp(0.0, 1.0)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClipKind {
    Pattern,
    Melody,
    Fade,
    Sequence,
}

impl ClipKind {
    pub fn symbol(self) -> char {
        match self {
            ClipKind::Pattern => 'P',
            ClipKind::Melody => 'M',
            ClipKind::Fade => 'F',
            ClipKind::Sequence => 'S',
        }
    }

    pub fn color(self) -> Color {
        match self {
            ClipKind::Pattern => Color::Cyan,
            ClipKind::Melody => Color::Magenta,
            ClipKind::Fade => Color::Yellow,
            ClipKind::Sequence => Color::Green,
        }
    }

    pub fn order(self) -> u8 {
        match self {
            ClipKind::Pattern => 0,
            ClipKind::Melody => 1,
            ClipKind::Fade => 2,
            ClipKind::Sequence => 3,
        }
    }
}

#[derive(Clone)]
pub struct SequenceClipDisplay {
    pub start: f64,
    pub end: f64,
    pub label: String,
    pub kind: ClipKind,
}

impl SequenceClipDisplay {
    pub fn from_clip(clip: &vibelang_core::sequences::SequenceClip) -> Self {
        let (label, kind) = match &clip.source {
            ClipSource::Pattern(name) => (name.clone(), ClipKind::Pattern),
            ClipSource::Melody(name) => (name.clone(), ClipKind::Melody),
            ClipSource::Fade(name) => (name.clone(), ClipKind::Fade),
            ClipSource::Sequence(name) => (name.clone(), ClipKind::Sequence),
        };

        Self {
            start: clip.start,
            end: clip.end,
            label,
            kind,
        }
    }
}

#[derive(Clone)]
pub struct HierarchyEntry {
    pub id: String,
    pub depth: usize,
    pub label: String,
    pub detail: String,
    pub params: Vec<(String, String)>, // (name, value) pairs for display
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

    fn group(group: &GroupState, depth: usize, collapsed: bool) -> Self {
        let mut detail = Vec::new();
        if group.muted {
            detail.push("muted".to_string());
        }
        if group.soloed {
            detail.push("solo".to_string());
        }
        if !group.synth_node_ids.is_empty() {
            detail.push(format!("{} synths", group.synth_node_ids.len()));
        }

        // Format params for display
        let params: Vec<(String, String)> = group
            .params
            .iter()
            .map(|(k, v)| (k.clone(), format_param_value(*v)))
            .collect();

        Self {
            id: format!("group:{}", group.path),
            depth,
            label: group.name.clone(),
            detail: detail.join(" • "),
            params,
            kind: HierarchyKind::Group,
            active: !group.muted,
            collapsible: true,
            collapsed,
        }
    }

    fn section(label: &str, depth: usize, collapsed: bool) -> Self {
        Self {
            id: format!("section:{}", label),
            depth,
            label: label.to_string(),
            detail: String::new(),
            params: Vec::new(),
            kind: HierarchyKind::Section,
            active: false,
            collapsible: true,
            collapsed,
        }
    }

    fn sequence(name: &str, loop_beats: f64, active: bool, paused: bool, depth: usize, collapsed: bool, clip_count: usize) -> Self {
        let mut detail = format!("{:.1}b", loop_beats);
        if clip_count > 0 {
            detail.push_str(&format!(" • {} clips", clip_count));
        }
        if paused {
            detail.push_str(" • paused");
        } else if active {
            detail.push_str(" • ▶");
        }
        Self {
            id: format!("seq:{}", name),
            depth,
            label: name.to_string(),
            detail,
            params: Vec::new(),
            kind: HierarchyKind::Sequence,
            active,
            collapsible: true,
            collapsed,
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

#[derive(Clone)]
struct HierarchyItem {
    id: String,
    kind: HierarchyKind,
    label: String,
    detail: String,
    params: Vec<(String, String)>,
    active: bool,
    collapsible: bool,
}

impl HierarchyItem {
    fn into_entry(self, depth: usize, collapsed: bool) -> HierarchyEntry {
        HierarchyEntry {
            id: self.id,
            depth,
            label: self.label,
            detail: self.detail,
            params: self.params,
            kind: self.kind,
            active: self.active,
            collapsible: self.collapsible,
            collapsed,
        }
    }

    fn order(&self) -> u8 {
        match self.kind {
            HierarchyKind::Voice => 0,
            HierarchyKind::Pattern => 1,
            HierarchyKind::Melody => 2,
            HierarchyKind::Effect => 3,
            HierarchyKind::Group => 4,
            HierarchyKind::Sequence => 5,
            HierarchyKind::Section => 6,
        }
    }
}

fn build_hierarchy_entries(state: &ScriptState, collapsed: &HashSet<String>) -> Vec<HierarchyEntry> {
    let mut entries = Vec::new();

    let mut group_children: BTreeMap<Option<String>, Vec<String>> = BTreeMap::new();
    for group in state.groups.values() {
        group_children
            .entry(group.parent_path.clone())
            .or_default()
            .push(group.path.clone());
    }
    for children in group_children.values_mut() {
        children.sort();
    }

    let mut grouped_items: BTreeMap<String, Vec<HierarchyItem>> = BTreeMap::new();

    for voice in state.voices.values() {
        grouped_items
            .entry(voice.group_path.clone())
            .or_default()
            .push(hierarchy_item_for_voice(voice));
    }

    for pattern in state.patterns.values() {
        grouped_items
            .entry(pattern.group_path.clone())
            .or_default()
            .push(hierarchy_item_for_pattern(pattern));
    }

    for melody in state.melodies.values() {
        grouped_items
            .entry(melody.group_path.clone())
            .or_default()
            .push(hierarchy_item_for_melody(melody));
    }

    for effect in state.effects.values() {
        grouped_items
            .entry(effect.group_path.clone())
            .or_default()
            .push(hierarchy_item_for_effect(effect));
    }

    if let Some(root_groups) = group_children.get(&None) {
        for group_path in root_groups {
            collect_group_entries(
                group_path,
                0,
                &mut entries,
                state,
                &group_children,
                &grouped_items,
                collapsed,
            );
        }
    }

    if entries.is_empty() {
        entries.push(HierarchyEntry::section("No groups defined", 0, false));
    }

    if !state.sequences.is_empty() {
        let seq_collapsed = collapsed.contains("section:Sequences");
        entries.push(HierarchyEntry::section("Sequences", 0, seq_collapsed));

        if !seq_collapsed {
            let mut names: Vec<_> = state.sequences.keys().cloned().collect();
            names.sort();
            for name in names {
                let seq_def = state.sequences.get(&name);
                let loop_beats = seq_def.map(|s| s.loop_beats).unwrap_or(0.0);
                let clip_count = seq_def.map(|s| s.clips.len()).unwrap_or(0);
                let active = state.active_sequences.get(&name);
                let this_collapsed = collapsed.contains(&format!("seq:{}", name));

                entries.push(HierarchyEntry::sequence(
                    &name,
                    loop_beats,
                    active.is_some(),
                    active.map(|s| s.paused).unwrap_or(false),
                    1,
                    this_collapsed,
                    clip_count,
                ));

                // Show clip details if not collapsed
                if !this_collapsed {
                    if let Some(def) = seq_def {
                        for clip in &def.clips {
                            let (kind, kind_label) = match &clip.source {
                                ClipSource::Pattern(n) => (HierarchyKind::Pattern, format!("pat:{}", n)),
                                ClipSource::Melody(n) => (HierarchyKind::Melody, format!("mel:{}", n)),
                                ClipSource::Fade(n) => (HierarchyKind::Effect, format!("fade:{}", n)),
                                ClipSource::Sequence(n) => (HierarchyKind::Sequence, format!("seq:{}", n)),
                            };
                            entries.push(HierarchyEntry {
                                id: format!("clip:{}:{}", name, kind_label),
                                depth: 2,
                                label: kind_label,
                                detail: format!("{:.1}-{:.1}b", clip.start, clip.end),
                                params: Vec::new(),
                                kind,
                                active: active.is_some(),
                                collapsible: false,
                                collapsed: false,
                            });
                        }
                    }
                }
            }
        }
    }

    entries
}

fn collect_group_entries(
    path: &str,
    depth: usize,
    entries: &mut Vec<HierarchyEntry>,
    state: &ScriptState,
    children: &BTreeMap<Option<String>, Vec<String>>,
    items: &BTreeMap<String, Vec<HierarchyItem>>,
    collapsed: &HashSet<String>,
) {
    if let Some(group) = state.groups.get(path) {
        let group_id = format!("group:{}", path);
        let is_collapsed = collapsed.contains(&group_id);
        entries.push(HierarchyEntry::group(group, depth, is_collapsed));

        // Only show children if not collapsed
        if !is_collapsed {
            if let Some(group_items) = items.get(path) {
                let mut sorted_items = group_items.clone();
                sorted_items.sort_by(|a, b| a.order().cmp(&b.order()).then(a.label.cmp(&b.label)));
                for item in sorted_items {
                    let item_collapsed = collapsed.contains(&item.id);
                    entries.push(item.into_entry(depth + 1, item_collapsed));
                }
            }

            if let Some(child_paths) = children.get(&Some(path.to_string())) {
                for child in child_paths {
                    collect_group_entries(child, depth + 1, entries, state, children, items, collapsed);
                }
            }
        }
    }
}

fn hierarchy_item_for_voice(voice: &VoiceState) -> HierarchyItem {
    let mut detail = Vec::new();
    if let Some(synth) = &voice.synth_name {
        detail.push(synth.clone());
    }
    if voice.muted {
        detail.push("muted".to_string());
    }
    if voice.soloed {
        detail.push("solo".to_string());
    }

    // Build params list - combine gain with amp for unified display
    let mut params: Vec<(String, String)> = Vec::new();

    // Calculate effective amp: if user set amp via set_param, use that; otherwise use gain
    let effective_amp = if let Some(&amp_val) = voice.params.get("amp") {
        amp_val as f64
    } else {
        voice.gain
    };

    // Only show amp if not default (1.0)
    if (effective_amp - 1.0).abs() > 0.001 {
        params.push(("amp".to_string(), format_param_value(effective_amp as f32)));
    }

    if voice.polyphony > 1 {
        params.push(("poly".to_string(), voice.polyphony.to_string()));
    }

    // Add other params (excluding amp since we already handled it)
    for (k, v) in &voice.params {
        if k != "amp" {
            params.push((k.clone(), format_param_value(*v)));
        }
    }

    HierarchyItem {
        id: format!("voice:{}", voice.name),
        kind: HierarchyKind::Voice,
        label: voice.name.clone(),
        detail: detail.join(" • "),
        params,
        active: !voice.muted,
        collapsible: false,
    }
}

fn hierarchy_item_for_pattern(pattern: &PatternState) -> HierarchyItem {
    let mut detail_parts = Vec::new();
    match &pattern.status {
        LoopStatus::Playing { .. } => detail_parts.push("▶".to_string()),
        LoopStatus::Queued { start_beat } => detail_parts.push(format!("⏳@{:.0}", start_beat)),
        LoopStatus::QueuedStop { stop_beat, .. } => detail_parts.push(format!("⏹@{:.0}", stop_beat)),
        LoopStatus::Stopped => detail_parts.push("⏸".to_string()),
    };
    if let Some(voice) = &pattern.voice_name {
        detail_parts.push(format!("→{}", voice));
    }

    // Include pattern params
    let params: Vec<(String, String)> = pattern
        .params
        .iter()
        .map(|(k, v)| (k.clone(), format_param_value(*v)))
        .collect();

    HierarchyItem {
        id: format!("pattern:{}", pattern.name),
        kind: HierarchyKind::Pattern,
        label: pattern.name.clone(),
        detail: detail_parts.join(" "),
        params,
        active: pattern.status.is_playing(),
        collapsible: false,
    }
}

fn hierarchy_item_for_melody(melody: &MelodyState) -> HierarchyItem {
    let mut detail_parts = Vec::new();
    match &melody.status {
        LoopStatus::Playing { .. } => detail_parts.push("▶".to_string()),
        LoopStatus::Queued { start_beat } => detail_parts.push(format!("⏳@{:.0}", start_beat)),
        LoopStatus::QueuedStop { stop_beat, .. } => detail_parts.push(format!("⏹@{:.0}", stop_beat)),
        LoopStatus::Stopped => detail_parts.push("⏸".to_string()),
    };
    if let Some(voice) = &melody.voice_name {
        detail_parts.push(format!("→{}", voice));
    }

    // Include melody params
    let params: Vec<(String, String)> = melody
        .params
        .iter()
        .map(|(k, v)| (k.clone(), format_param_value(*v)))
        .collect();

    HierarchyItem {
        id: format!("melody:{}", melody.name),
        kind: HierarchyKind::Melody,
        label: melody.name.clone(),
        detail: detail_parts.join(" "),
        params,
        active: melody.status.is_playing(),
        collapsible: false,
    }
}

fn hierarchy_item_for_effect(effect: &EffectState) -> HierarchyItem {
    // Show effect params
    let params: Vec<(String, String)> = effect
        .params
        .iter()
        .map(|(k, v)| (k.clone(), format_param_value(*v)))
        .collect();

    HierarchyItem {
        id: format!("effect:{}", effect.id),
        kind: HierarchyKind::Effect,
        label: effect.id.clone(),
        detail: effect.synthdef_name.clone(),
        params,
        active: true,
        collapsible: false,
    }
}
