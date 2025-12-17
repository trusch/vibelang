//! Beat-based event scheduler.
//!
//! The scheduler collects events that are due within a lookahead window
//! and returns them for execution. It maintains per-loop state to prevent
//! interference between loops with different start times or periods.

use crate::events::{BeatEvent, Pattern};
use crate::timing::{BeatTime, TransportClock};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

/// Identifies what kind of loop is being scheduled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoopKind {
    /// A rhythmic pattern.
    Pattern,
    /// A melodic pattern.
    Melody,
    /// A nested sequence.
    Sequence,
}

/// Snapshot of a running loop for scheduler processing.
#[derive(Clone, Debug)]
pub struct LoopSnapshot {
    /// Unique name of this loop.
    pub name: String,
    /// The pattern data.
    pub pattern: Pattern,
    /// Beat when this loop started playing.
    pub start_beat: f64,
    /// What kind of loop this is.
    pub kind: LoopKind,
    /// Group path for tagging events.
    pub group_path: Option<String>,
    /// Voice name for tagging events.
    pub voice_name: Option<String>,
}

/// Beat-based event scheduler with per-loop tracking.
///
/// The scheduler maintains independent tracking for each loop to prevent
/// loops with different start times or periods from interfering with
/// each other.
pub struct EventScheduler {
    /// Last scheduled beat per loop (prevents duplicate scheduling).
    loop_last_scheduled: HashMap<String, BeatTime>,
    /// Beat position to use as default for new loops (prevents event burst on restart).
    /// Set to current beat minus epsilon on reset to allow scheduling from beat 0.
    default_last_scheduled: BeatTime,
}

impl Default for EventScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl EventScheduler {
    /// Create a new event scheduler.
    pub fn new() -> Self {
        Self {
            loop_last_scheduled: HashMap::new(),
            // Default to -1.0 for initial state, will be updated on reset
            default_last_scheduled: BeatTime::from_float(-1.0),
        }
    }

    /// Reset all loop tracking state.
    ///
    /// Call this when the transport is stopped or rewound.
    /// The `target_beat` parameter sets the starting point for scheduling -
    /// events at or after this beat will be scheduled on the next tick.
    pub fn reset_to_beat(&mut self, target_beat: f64) {
        self.loop_last_scheduled.clear();
        // Set default to just before target beat so events at target_beat are scheduled,
        // but we won't burst through events before target_beat
        self.default_last_scheduled = BeatTime::from_float(target_beat - 0.001);
    }

    /// Reset all loop tracking state to beat 0.
    ///
    /// Call this when the transport is stopped or rewound to the beginning.
    pub fn reset(&mut self) {
        self.reset_to_beat(0.0);
    }

    /// Reset tracking for a specific loop.
    pub fn reset_loop(&mut self, name: &str) {
        self.loop_last_scheduled.remove(name);
    }

    /// Sync all last_scheduled positions to a specific beat without clearing loop tracking.
    /// Use this for pause/resume to prevent event bursts.
    pub fn sync_to_beat(&mut self, beat: f64) {
        let beat_time = BeatTime::from_float(beat);
        for last_scheduled in self.loop_last_scheduled.values_mut() {
            *last_scheduled = beat_time;
        }
        self.default_last_scheduled = beat_time;
    }

    /// Collect all events that are due within the lookahead window.
    ///
    /// # Arguments
    /// * `clock` - The transport clock
    /// * `now` - Current wall-clock time
    /// * `loops` - Active loops to process
    /// * `scheduled_events` - One-shot scheduled events
    /// * `lookahead_ms` - Lookahead window in milliseconds
    ///
    /// # Returns
    /// A sorted list of (beat_time, events) pairs.
    pub fn collect_due_events(
        &mut self,
        clock: &TransportClock,
        now: Instant,
        loops: &[LoopSnapshot],
        scheduled_events: &[(BeatEvent, BeatTime)],
        lookahead_ms: u64,
    ) -> Vec<(BeatTime, Vec<BeatEvent>)> {
        let current = clock.beat_at(now);
        let window_end =
            BeatTime::from_float(current.to_float() + clock.lookahead_beats(lookahead_ms));

        let mut events_by_beat: BTreeMap<BeatTime, Vec<BeatEvent>> = BTreeMap::new();

        // Process each loop independently
        for snapshot in loops {
            let pattern = &snapshot.pattern;
            if pattern.loop_length_beats <= f64::EPSILON {
                continue;
            }

            // Get the last scheduled beat for this specific loop
            // Use default_last_scheduled instead of -1.0 to prevent event burst on restart
            let loop_last_scheduled = self
                .loop_last_scheduled
                .get(&snapshot.name)
                .copied()
                .unwrap_or(self.default_last_scheduled);

            let mut max_beat_for_this_loop = loop_last_scheduled;

            // Process each event in the pattern
            let fade_events_count = pattern.events.iter().filter(|e| e.fade.is_some()).count();
            if fade_events_count > 0 {
                log::trace!("[SCHEDULER] Processing {} events ({} fades) for '{}'",
                    pattern.events.len(), fade_events_count, snapshot.name);
            }
            for event in &pattern.events {
                let mut event = event.clone();

                // Tag the event with loop metadata
                match snapshot.kind {
                    LoopKind::Pattern => {
                        if event.pattern_name.is_none() {
                            event.pattern_name = Some(snapshot.name.clone());
                        }
                    }
                    LoopKind::Melody => {
                        if event.melody_name.is_none() {
                            event.melody_name = Some(snapshot.name.clone());
                        }
                    }
                    LoopKind::Sequence => { /* keep existing metadata */ }
                }
                if event.group_path.is_none() {
                    event.group_path = snapshot.group_path.clone();
                }
                if event.voice_name.is_none() {
                    event.voice_name = snapshot.voice_name.clone();
                }

                // Calculate absolute beat positions for this event
                let event_beat_in_pattern = event.beat;
                let first_occurrence =
                    snapshot.start_beat + pattern.phase_offset + event_beat_in_pattern;
                let loop_length = pattern.loop_length_beats;

                // Find the first iteration that might be in our window
                let iterations_since_start = ((current.to_float() - first_occurrence) / loop_length)
                    .floor()
                    .max(0.0);

                let mut iteration = iterations_since_start;

                // Generate event occurrences until we exceed the window
                // Limit iterations to prevent infinite loops
                for _ in 0..2048 {
                    let absolute_beat = first_occurrence + (iteration * loop_length);

                    if absolute_beat > window_end.to_float() + 1e-9 {
                        break;
                    }

                    let beat_time = BeatTime::from_float(absolute_beat);

                    // Only schedule if not already scheduled
                    if beat_time > loop_last_scheduled && beat_time <= window_end {
                        if event.fade.is_some() {
                            log::trace!("[SCHEDULER] Scheduling fade '{}' at beat {}",
                                event.fade.as_ref().unwrap().name, absolute_beat);
                        }
                        events_by_beat
                            .entry(beat_time)
                            .or_default()
                            .push(event.clone());

                        if beat_time > max_beat_for_this_loop {
                            max_beat_for_this_loop = beat_time;
                        }
                    } else if event.fade.is_some() {
                        log::trace!("[SCHEDULER] Fade '{}' at beat {} rejected (last_sched={}, window_end={})",
                            event.fade.as_ref().unwrap().name, absolute_beat,
                            loop_last_scheduled.to_float(), window_end.to_float());
                    }

                    iteration += 1.0;
                }
            }

            // Update tracking for this loop
            if max_beat_for_this_loop > loop_last_scheduled {
                self.loop_last_scheduled
                    .insert(snapshot.name.clone(), max_beat_for_this_loop);
            }
        }

        // Add one-shot scheduled events
        for (event, beat) in scheduled_events {
            if *beat > current && *beat <= window_end {
                events_by_beat.entry(*beat).or_default().push(event.clone());
            }
        }

        events_by_beat.into_iter().collect()
    }

    /// Get the number of loops currently tracked.
    pub fn tracked_loop_count(&self) -> usize {
        self.loop_last_scheduled.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_pattern() -> Pattern {
        Pattern {
            name: "test".to_string(),
            events: vec![
                BeatEvent::new(0.0, "kick"),
                BeatEvent::new(1.0, "kick"),
                BeatEvent::new(2.0, "kick"),
                BeatEvent::new(3.0, "kick"),
            ],
            loop_length_beats: 4.0,
            phase_offset: 0.0,
        }
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = EventScheduler::new();
        assert_eq!(scheduler.tracked_loop_count(), 0);
    }

    #[test]
    fn test_scheduler_reset() {
        let mut scheduler = EventScheduler::new();
        scheduler
            .loop_last_scheduled
            .insert("test".to_string(), BeatTime::from_float(4.0));
        assert_eq!(scheduler.tracked_loop_count(), 1);
        scheduler.reset();
        assert_eq!(scheduler.tracked_loop_count(), 0);
    }

    #[test]
    fn test_loop_snapshot() {
        let snapshot = LoopSnapshot {
            name: "kick_pattern".to_string(),
            pattern: make_test_pattern(),
            start_beat: 0.0,
            kind: LoopKind::Pattern,
            group_path: Some("main.drums".to_string()),
            voice_name: Some("kick".to_string()),
        };
        assert_eq!(snapshot.name, "kick_pattern");
        assert_eq!(snapshot.kind, LoopKind::Pattern);
    }
}
