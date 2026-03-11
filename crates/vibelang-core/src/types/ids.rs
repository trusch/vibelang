//! Type-safe ID wrappers.
//!
//! These newtypes prevent accidentally mixing up different kinds of IDs.
//! For example, a `NodeId` cannot be used where a `BufferId` is expected.
//!
//! # Example
//!
//! ```
//! use vibelang_core::types::ids::{NodeId, VoiceId};
//!
//! let node = NodeId::new(1000);
//! let voice = VoiceId::new(1);
//!
//! // These are different types and cannot be mixed
//! assert_ne!(std::mem::size_of_val(&node), 0);
//! ```

use std::fmt;

/// Macro to define a type-safe ID wrapper.
macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
        #[repr(transparent)]
        pub struct $name(pub u32);

        impl $name {
            /// Create a new ID with the given value.
            #[inline]
            pub const fn new(id: u32) -> Self {
                Self(id)
            }

            /// Get the raw ID value.
            #[inline]
            pub const fn raw(self) -> u32 {
                self.0
            }
        }

        impl From<u32> for $name {
            #[inline]
            fn from(id: u32) -> Self {
                Self(id)
            }
        }

        impl From<$name> for u32 {
            #[inline]
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

define_id!(
    /// ID for a synthesis node (synth or group) in the audio graph.
    NodeId
);

define_id!(
    /// ID for an audio buffer holding sample data.
    BufferId
);

define_id!(
    /// ID for an audio bus (for routing signals between nodes).
    BusId
);

define_id!(
    /// ID for a group in the audio hierarchy.
    ///
    /// Groups contain voices and effects, and can be nested.
    GroupId
);

define_id!(
    /// ID for a voice (sound-producing unit).
    ///
    /// Voices are the primary way to trigger sounds from patterns/melodies.
    VoiceId
);

define_id!(
    /// ID for a rhythmic pattern.
    ///
    /// Patterns trigger voices at specific beat positions.
    PatternId
);

define_id!(
    /// ID for a melodic sequence.
    ///
    /// Melodies are like patterns but include pitch information.
    MelodyId
);

define_id!(
    /// ID for a sequence (arrangement of clips).
    ///
    /// Sequences can contain patterns, melodies, fades, and nested sequences.
    SequenceId
);

define_id!(
    /// ID for an audio effect.
    ///
    /// Effects process audio passing through a group.
    EffectId
);

define_id!(
    /// ID for a loaded audio sample.
    SampleId
);

define_id!(
    /// ID for a loaded SFZ instrument.
    ///
    /// SFZ instruments contain multiple samples mapped across keys and velocities.
    SfzId
);

define_id!(
    /// ID for an audio recording session.
    ///
    /// Recordings capture audio from a group's bus into a buffer.
    RecordingId
);

define_id!(
    /// ID for a modulator (LFO, envelope, follower, etc.).
    ///
    /// Modulators output control-rate signals that can be routed to voice parameters.
    ModulatorId
);

define_id!(
    /// ID for a control bus.
    ///
    /// Control buses carry control-rate signals for modulation.
    ControlBusId
);

define_id!(
    /// ID for a fade (parameter automation).
    ///
    /// Fades smoothly transition parameters over time. Using stable IDs
    /// allows the reload diff system to detect unchanged fades and avoid
    /// re-firing them on every hot reload.
    FadeId
);

#[cfg(feature = "midi")]
define_id!(
    /// ID for a MIDI device.
    MidiDeviceId
);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_id_creation() {
        let node = NodeId::new(42);
        assert_eq!(node.raw(), 42);
        assert_eq!(u32::from(node), 42);
    }

    #[test]
    fn test_id_from_u32() {
        let node: NodeId = 100.into();
        assert_eq!(node.raw(), 100);
    }

    #[test]
    fn test_id_debug() {
        let node = NodeId::new(123);
        assert_eq!(format!("{:?}", node), "NodeId(123)");
    }

    #[test]
    fn test_id_display() {
        let node = NodeId::new(456);
        assert_eq!(format!("{}", node), "456");
    }

    #[test]
    fn test_different_id_types() {
        // Compile-time check: these are different types
        let _node: NodeId = NodeId::new(1);
        let _voice: VoiceId = VoiceId::new(1);
        // Cannot assign NodeId to VoiceId - this wouldn't compile:
        // let _voice: VoiceId = _node;
    }

    #[test]
    fn test_id_equality() {
        let a = GroupId::new(5);
        let b = GroupId::new(5);
        let c = GroupId::new(6);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_id_hash() {
        let mut set = HashSet::new();
        set.insert(PatternId::new(1));
        set.insert(PatternId::new(2));
        set.insert(PatternId::new(1)); // Duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&PatternId::new(1)));
        assert!(set.contains(&PatternId::new(2)));
    }

    #[test]
    fn test_id_default() {
        let id = VoiceId::default();
        assert_eq!(id.raw(), 0);
    }

    #[test]
    fn test_id_clone() {
        let original = EffectId::new(99);
        let cloned = original;
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_all_id_types() {
        // Ensure all ID types can be created
        let _ = NodeId::new(1);
        let _ = BufferId::new(2);
        let _ = BusId::new(3);
        let _ = GroupId::new(4);
        let _ = VoiceId::new(5);
        let _ = PatternId::new(6);
        let _ = MelodyId::new(7);
        let _ = SequenceId::new(8);
        let _ = EffectId::new(9);
        let _ = SampleId::new(10);
        let _ = SfzId::new(11);
        let _ = RecordingId::new(12);
    }
}
