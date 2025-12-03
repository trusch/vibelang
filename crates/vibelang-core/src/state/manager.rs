//! State manager for thread-safe state access.
//!
//! The [`StateManager`] provides synchronized access to the central
//! [`ScriptState`]. It uses an RwLock to allow multiple readers or
//! a single writer.

use std::sync::{Arc, RwLock};

use super::model::ScriptState;

/// Thread-safe manager for the central state.
///
/// The StateManager wraps the ScriptState in an Arc<RwLock> to provide
/// safe concurrent access from multiple threads.
#[derive(Clone)]
pub struct StateManager {
    state: Arc<RwLock<ScriptState>>,
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl StateManager {
    /// Create a new state manager with default state.
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(ScriptState::new())),
        }
    }

    /// Create a state manager with a specific initial state.
    pub fn with_state(state: ScriptState) -> Self {
        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }

    /// Read the state with a closure.
    ///
    /// This acquires a read lock for the duration of the closure.
    /// Multiple readers can hold the lock simultaneously.
    pub fn with_state_read<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&ScriptState) -> R,
    {
        let state = self.state.read().expect("State lock poisoned");
        f(&state)
    }

    /// Write to the state with a closure.
    ///
    /// This acquires an exclusive write lock for the duration of the closure.
    pub fn with_state_write<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ScriptState) -> R,
    {
        let mut state = self.state.write().expect("State lock poisoned");
        f(&mut state)
    }

    /// Get a clone of the current state.
    ///
    /// This is useful for taking snapshots but may be expensive for large states.
    pub fn snapshot(&self) -> ScriptState {
        self.with_state_read(|s| s.clone())
    }

    /// Get the current tempo.
    pub fn tempo(&self) -> f64 {
        self.with_state_read(|s| s.tempo)
    }

    /// Get the current beat position.
    pub fn current_beat(&self) -> f64 {
        self.with_state_read(|s| s.current_beat)
    }

    /// Check if the transport is running.
    pub fn is_transport_running(&self) -> bool {
        self.with_state_read(|s| s.transport_running)
    }

    /// Get the state version.
    pub fn version(&self) -> u64 {
        self.with_state_read(|s| s.version)
    }
}

impl std::fmt::Debug for StateManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StateManager")
            .field("version", &self.version())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_manager_creation() {
        let manager = StateManager::new();
        assert!((manager.tempo() - 120.0).abs() < 0.001);
        assert!(!manager.is_transport_running());
    }

    #[test]
    fn test_state_manager_write() {
        let manager = StateManager::new();
        manager.with_state_write(|s| {
            s.tempo = 140.0;
            s.bump_version();
        });
        assert!((manager.tempo() - 140.0).abs() < 0.001);
        assert_eq!(manager.version(), 1);
    }

    #[test]
    fn test_state_manager_snapshot() {
        let manager = StateManager::new();
        manager.with_state_write(|s| s.tempo = 90.0);
        let snapshot = manager.snapshot();
        assert!((snapshot.tempo - 90.0).abs() < 0.001);
    }

    #[test]
    fn test_state_manager_clone() {
        let manager1 = StateManager::new();
        let manager2 = manager1.clone();
        manager1.with_state_write(|s| s.tempo = 150.0);
        // Both managers share the same underlying state
        assert!((manager2.tempo() - 150.0).abs() < 0.001);
    }
}
