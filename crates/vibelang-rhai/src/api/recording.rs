//! Recording API for Rhai scripts.
//!
//! Provides audio recording capabilities with quantized start, count-in,
//! metronome support, and file saving.
//!
//! ## Example
//!
//! ```rhai
//! // Record 4 bars from the drums group with count-in
//! let take1 = record("take1")
//!     .from_group("drums")
//!     .bars(4)
//!     .count_in(2)
//!     .metronome(true)
//!     .to_file("recordings/drums_take1.wav")
//!     .apply();
//!
//! // The sample handle can be used immediately
//! // (audio arrives when recording completes)
//! let drums_voice = voice("playback").on(take1).apply();
//! ```

use rhai::{CustomType, Dynamic, Engine, EvalAltResult, NativeCallContext, Position, TypeBuilder};
use std::path::PathBuf;
use vibelang_core::traits::RecordingConfig as CoreRecordingConfig;

use super::sample::SampleHandle;
use crate::context;

// Suppress unused imports warning for rhai derive macro requirements
#[allow(unused_imports)]
use rhai::INT;

/// A RecordHandle represents a pending or active audio recording.
///
/// Use builder methods to configure the recording, then call `.apply()`
/// to start recording and get a sample handle.
#[derive(Clone, Debug, CustomType)]
pub struct RecordHandle {
    /// Unique identifier for this recording.
    pub id: String,
    /// Group path to record from.
    group_path: String,
    /// Length in bars (tempo-aware).
    length_bars: Option<f64>,
    /// Length in beats (tempo-aware).
    length_beats: Option<f64>,
    /// Length in seconds (fixed time).
    length_seconds: Option<f64>,
    /// Count-in bars before recording starts.
    count_in_bars: f64,
    /// Play metronome during count-in/recording.
    metronome: bool,
    /// File path to save recording.
    file_path: Option<String>,
    /// Start immediately (no quantization).
    start_immediately: bool,
    /// Number of channels (1 or 2).
    num_channels: i64,
}

impl RecordHandle {
    /// Create a new recording handle.
    pub fn new(_ctx: NativeCallContext, id: String) -> Self {
        Self {
            id,
            group_path: context::current_group_path(),
            length_bars: None,
            length_beats: None,
            length_seconds: None,
            count_in_bars: 0.0,
            metronome: false,
            file_path: None,
            start_immediately: false,
            num_channels: 2,
        }
    }

    // === Getters ===

    /// Get the recording ID.
    pub fn get_id(&mut self) -> String {
        self.id.clone()
    }

    /// Get the group path.
    pub fn get_group_path(&mut self) -> String {
        self.group_path.clone()
    }

    // === Length Configuration ===

    /// Set recording length in bars.
    pub fn bars(mut self, num_bars: f64) -> Self {
        self.length_bars = Some(num_bars);
        self.length_beats = None;
        self.length_seconds = None;
        self
    }

    /// Set recording length in bars (integer variant).
    pub fn bars_int(self, num_bars: i64) -> Self {
        self.bars(num_bars as f64)
    }

    /// Set recording length in beats.
    pub fn beats(mut self, num_beats: f64) -> Self {
        self.length_bars = None;
        self.length_beats = Some(num_beats);
        self.length_seconds = None;
        self
    }

    /// Set recording length in beats (integer variant).
    pub fn beats_int(self, num_beats: i64) -> Self {
        self.beats(num_beats as f64)
    }

    /// Set recording length in seconds (fixed time, ignores tempo).
    pub fn seconds(mut self, secs: f64) -> Self {
        self.length_bars = None;
        self.length_beats = None;
        self.length_seconds = Some(secs);
        self
    }

    /// Set recording length in seconds (integer variant).
    pub fn seconds_int(self, secs: i64) -> Self {
        self.seconds(secs as f64)
    }

    // === Group Configuration ===

    /// Record from a specific group instead of the current group.
    pub fn from_group(mut self, group_path: String) -> Self {
        self.group_path = context::resolve_group_reference(&group_path).unwrap_or(group_path);
        self
    }

    // === Recording Options ===

    /// Add count-in bars before recording starts.
    pub fn count_in(mut self, bars: f64) -> Self {
        self.count_in_bars = bars;
        self
    }

    /// Enable/disable metronome during count-in and recording.
    pub fn metronome(mut self, enabled: bool) -> Self {
        self.metronome = enabled;
        self
    }

    /// Save recording to a file.
    pub fn to_file(mut self, path: String) -> Self {
        self.file_path = Some(path);
        self
    }

    /// Start immediately without quantization.
    pub fn immediate(mut self) -> Self {
        self.start_immediately = true;
        self
    }

    /// Set the number of channels (1 = mono, 2 = stereo).
    pub fn channels(mut self, num: i64) -> Self {
        self.num_channels = num.clamp(1, 2);
        self
    }

    // === Apply ===

    /// Start the recording and return a sample handle.
    ///
    /// The sample handle can be used immediately, though the audio
    /// won't be available until the recording completes.
    pub fn apply(self) -> SampleHandle {
        let recording_id = context::get_or_create_recording_id(&self.id);
        let group_id = context::get_or_create_group_id(&self.group_path);

        // Get tempo and time signature for beat calculations
        let (_tempo, time_sig) = (context::get_tempo(), context::get_time_signature());
        let beats_per_bar = time_sig.0 as f64 * (4.0 / time_sig.1 as f64);

        // Calculate length in beats
        let length_beats = self
            .length_bars
            .map(|bars| bars * beats_per_bar)
            .or(self.length_beats);

        // Calculate count-in in beats
        let count_in_beats = self.count_in_bars * beats_per_bar;

        // Save file path string for the sample handle (before moving into closure)
        let file_path_str = self.file_path.clone().unwrap_or_default();

        // Resolve file path
        let file_path = self.file_path.map(|p| {
            if let Some(current_file) = context::get_current_file() {
                if let Some(parent) = current_file.parent() {
                    let resolved = parent.join(&p);
                    return resolved;
                }
            }
            PathBuf::from(p)
        });

        // Create the recording config
        let config = CoreRecordingConfig {
            group: group_id,
            length_beats,
            length_seconds: self.length_seconds,
            start_beat: None, // Will be calculated by the runtime
            count_in_beats,
            metronome: self.metronome,
            file_path,
            num_channels: self.num_channels as u8,
        };

        // Store in script state for the runtime to pick up
        context::with_state(|state| {
            state.recordings.insert(recording_id, config);
        });

        // Calculate approximate buffer ID for the sample handle
        // The actual buffer will be allocated by the runtime
        let buffer_id = recording_id.raw() as i32;

        SampleHandle::new_pending(self.id, file_path_str, buffer_id, self.num_channels as i32)
    }
}

/// Create a new recording handle.
///
/// Usage: `record("take1").bars(4).apply()`
pub fn record(ctx: NativeCallContext, id: String) -> RecordHandle {
    RecordHandle::new(ctx, id)
}

/// Stop a recording by ID.
pub fn stop_recording(id: String) {
    let _recording_id = context::get_or_create_recording_id(&id);
    // The actual stop message would be sent via the runtime
    // For now, this just logs the intent
    log::info!("[RECORD] Stop requested for recording '{}'", id);
}

/// Cancel a pending or active recording by ID.
pub fn cancel_recording(id: String) {
    let _recording_id = context::get_or_create_recording_id(&id);
    // The actual cancel message would be sent via the runtime
    log::info!("[RECORD] Cancel requested for recording '{}'", id);
}

/// Register the recording API with a Rhai engine.
pub fn register(engine: &mut Engine) {
    // Register RecordHandle type
    engine.build_type::<RecordHandle>();

    // Constructor
    engine.register_fn("record", record);

    // Getters
    engine.register_fn("id", RecordHandle::get_id);
    engine.register_get("id", RecordHandle::get_id);
    engine.register_fn("group_path", RecordHandle::get_group_path);
    engine.register_get("group_path", RecordHandle::get_group_path);

    // Length configuration (builder methods)
    engine.register_fn("bars", RecordHandle::bars);
    engine.register_fn("bars", RecordHandle::bars_int);
    engine.register_fn("beats", RecordHandle::beats);
    engine.register_fn("beats", RecordHandle::beats_int);
    engine.register_fn("seconds", RecordHandle::seconds);
    engine.register_fn("seconds", RecordHandle::seconds_int);

    // Group configuration
    engine.register_fn("from_group", RecordHandle::from_group);

    // Recording options (builder methods)
    engine.register_fn("count_in", RecordHandle::count_in);
    engine.register_fn("metronome", RecordHandle::metronome);
    engine.register_fn("to_file", RecordHandle::to_file);
    engine.register_fn("immediate", RecordHandle::immediate);
    engine.register_fn("channels", RecordHandle::channels);

    // Apply
    engine.register_fn("apply", RecordHandle::apply);

    // Control functions
    engine.register_fn("stop_recording", stop_recording);
    engine.register_fn("cancel_recording", cancel_recording);
}

use vibelang_core::candidate::{
    AuthoringDeclaration, Cancellation, CandidateError, CanonicalF64, Composition,
    DeclarationOwner, DeclarationPayload, DesiredLifecycle, ErasedRef, GroupKind, GroupScope,
    LifecycleAction, LifecycleMetadata, RecordingAuthoring, RecordingKind,
    RecordingLengthAuthoring, SampleKind, StartMode, TerminalEffect, TypedAddress,
};
use vibelang_core::types::Beat;

use super::sample::SampleRef;
use crate::foundation::{self, BuilderBase, FoundationError, Observation, RefBase};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordRef {
    base: RefBase,
}

impl RecordRef {
    pub(crate) fn new(base: RefBase) -> Result<Self, FoundationError> {
        base.typed::<RecordingKind>()?;
        Ok(Self { base })
    }

    #[must_use]
    pub fn base(&self) -> &RefBase {
        &self.base
    }

    fn action(self, action: LifecycleAction, role: &str) -> Result<Self, FoundationError> {
        let (effect, cancellation) = match &action {
            LifecycleAction::Start(_) => (TerminalEffect::Start, Cancellation::BeforePlanning),
            LifecycleAction::Stop => (TerminalEffect::Stop, Cancellation::NotCancellable),
            LifecycleAction::Remove => (TerminalEffect::Cancel, Cancellation::RemoveDeclaration),
            LifecycleAction::Cancel => (TerminalEffect::Cancel, Cancellation::BeforePlanning),
            _ => {
                return Err(CandidateError::InvalidLifecycle(
                    "unsupported RecordRef lifecycle action".into(),
                )
                .into())
            }
        };
        let source = foundation::operation_source(&self.base, role)?;
        let base = foundation::commit_action(
            self.base,
            LifecycleMetadata::reference(effect, cancellation),
            action,
            source,
        )?;
        Ok(Self { base })
    }

    pub fn start(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Start(StartMode::Normal), "start")
    }

    pub fn start_now(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Start(StartMode::Immediate), "start-now")
    }

    /// Finalize the active run and keep its normal result.
    pub fn stop(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Stop, "stop")
    }

    /// Abort the pending or active run; it produces no normal result.
    pub fn cancel(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Cancel, "cancel")
    }

    pub fn remove(self) -> Result<Self, FoundationError> {
        self.action(LifecycleAction::Remove, "remove")
    }

    pub fn status(&self) -> Result<Observation, FoundationError> {
        foundation::observe(&self.base)
    }

    /// Typed completion handle: the logical Sample address a completed
    /// run binds its take to. No physical buffer ID is exposed at any
    /// point — lowering resolves the applied generation during planning,
    /// and the SampleRef becomes usable only once the recording outcome
    /// and resource binding are applied (observable via `status`).
    pub fn sample(&self) -> Result<SampleRef, FoundationError> {
        let address = self.base.address();
        let sample_address = TypedAddress::<SampleKind>::new(
            address.project().clone(),
            address.module().clone(),
            address.group_scope().clone(),
            address.key().clone(),
        );
        SampleRef::new(RefBase::new(ErasedRef::new(
            self.base.identity().clone(),
            sample_address.erase(),
        )))
    }
}

#[derive(Clone, Debug, PartialEq)]
enum RecordLengthV2 {
    Beats(i64),
    Seconds(f64),
}

/// Pure recording builder. Each recording admits exactly one active run
/// per candidate; `apply` registers a dormant declaration and
/// `start`/`start_now` request the run. The v1 `.immediate()` flag was
/// never effective and has no v2 spelling — use the `start_now`
/// terminal. The v1 `.bars(n)` spelling has no pure v2 equivalent (bar
/// length depends on runtime time-signature state); migrate to
/// `.beats(n * beats_per_bar)`.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordBuilder {
    base: BuilderBase,
    source: Option<RefBase>,
    length: Option<RecordLengthV2>,
    count_in_ticks: i64,
    metronome: bool,
    destination: Option<String>,
    channels: u8,
}

impl RecordBuilder {
    #[must_use]
    pub fn new(base: BuilderBase) -> Self {
        Self {
            base,
            source: None,
            length: None,
            count_in_ticks: 0,
            metronome: false,
            destination: None,
            channels: 2,
        }
    }

    /// Record from a typed group. The source is mandatory: a terminal
    /// without one is a structured failure, never a silent default.
    pub fn from(mut self, source: RefBase) -> Result<Self, FoundationError> {
        source.typed::<GroupKind>()?;
        self.source = Some(source);
        Ok(self)
    }

    /// Effective forwarding alias for the v1 `.from_group(path)`
    /// spelling; the argument is now a typed group Ref, never a raw
    /// path resolved against ambient script state.
    pub fn from_group(self, source: RefBase) -> Result<Self, FoundationError> {
        self.from(source)
    }

    pub fn beats(mut self, beats: f64) -> Result<Self, FoundationError> {
        if !beats.is_finite() || beats <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Recording length in beats must be finite and positive".into(),
            )
            .into());
        }
        self.length = Some(RecordLengthV2::Beats(Beat::from_f64(beats).raw()));
        Ok(self)
    }

    pub fn seconds(mut self, seconds: f64) -> Result<Self, FoundationError> {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Recording length in seconds must be finite and positive".into(),
            )
            .into());
        }
        self.length = Some(RecordLengthV2::Seconds(seconds));
        Ok(self)
    }

    /// Count-in before the run starts, in beats. The v1 spelling counted
    /// bars against runtime time-signature state, which a pure builder
    /// cannot consult; migrate by multiplying bars by beats-per-bar.
    pub fn count_in(mut self, beats: f64) -> Result<Self, FoundationError> {
        if !beats.is_finite() || beats < 0.0 {
            return Err(CandidateError::InvalidAuthoring(
                "Recording count-in must be finite and non-negative".into(),
            )
            .into());
        }
        self.count_in_ticks = Beat::from_f64(beats).raw();
        Ok(self)
    }

    #[must_use]
    pub fn metronome(mut self, enabled: bool) -> Self {
        self.metronome = enabled;
        self
    }

    pub fn to_file(mut self, path: String) -> Result<Self, FoundationError> {
        if path.is_empty() || path.trim() != path || path.bytes().any(|byte| byte < 0x20) {
            return Err(CandidateError::InvalidAuthoring(
                "Recording destination must be a non-empty path without surrounding whitespace or control bytes"
                    .into(),
            )
            .into());
        }
        self.destination = Some(path);
        Ok(self)
    }

    pub fn channels(mut self, channels: i64) -> Result<Self, FoundationError> {
        if !(1..=2).contains(&channels) {
            return Err(CandidateError::InvalidAuthoring(
                "Recording channels must be 1 or 2".into(),
            )
            .into());
        }
        self.channels = channels as u8;
        Ok(self)
    }

    fn terminal(self, lifecycle: DesiredLifecycle) -> Result<RecordRef, FoundationError> {
        let source = self.source.ok_or_else(|| {
            CandidateError::InvalidAuthoring(
                "RecordBuilder needs a typed source group before a terminal: declare .from(group)"
                    .into(),
            )
        })?;
        let length = self
            .length
            .map(|length| {
                Ok::<_, CandidateError>(match length {
                    RecordLengthV2::Beats(ticks) => RecordingLengthAuthoring::Beats(ticks),
                    RecordLengthV2::Seconds(seconds) => {
                        RecordingLengthAuthoring::Seconds(CanonicalF64::new(seconds)?)
                    }
                })
            })
            .transpose()?;
        let declaration = RecordingAuthoring {
            source: source.typed::<GroupKind>()?,
            length,
            count_in_ticks: self.count_in_ticks,
            metronome: self.metronome,
            destination: self.destination,
            channels: self.channels,
            lifecycle,
        };
        let payload = DeclarationPayload::authoring(AuthoringDeclaration::Recording(declaration))?;
        let references = vec![(source, self.base.source().clone())];
        let owner = DeclarationOwner::Structural(self.base.source().syntax_key().clone());
        let metadata = match lifecycle {
            DesiredLifecycle::Dormant => LifecycleMetadata::register(Composition::Standalone),
            DesiredLifecycle::Start(_) => LifecycleMetadata::start(Composition::Standalone),
        };
        let (fragment, reference) = self.base.fragment(owner, metadata, payload, references)?;
        foundation::commit_fragment(fragment)?;
        RecordRef::new(reference)
    }

    /// Register the recording dormant; no run is requested.
    pub fn apply(self) -> Result<RecordRef, FoundationError> {
        self.terminal(DesiredLifecycle::Dormant)
    }

    /// Register and request one run at the declared normal quantization.
    pub fn start(self) -> Result<RecordRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Normal))
    }

    /// Register and request one immediate, unquantized run.
    pub fn start_now(self) -> Result<RecordRef, FoundationError> {
        self.terminal(DesiredLifecycle::Start(StartMode::Immediate))
    }
}

pub(crate) fn record_builder_v2(name: String) -> Result<RecordBuilder, Box<EvalAltResult>> {
    Ok(RecordBuilder::new(
        foundation::authoring_builder::<RecordingKind>(&name, GroupScope::root())
            .map_err(|error| record_v2_error(error, Position::NONE))?,
    ))
}

pub(crate) fn record_ref_v2(name: String) -> Result<RecordRef, Box<EvalAltResult>> {
    RecordRef::new(
        foundation::authoring_ref::<RecordingKind>(&name, GroupScope::root())
            .map_err(|error| record_v2_error(error, Position::NONE))?,
    )
    .map_err(|error| record_v2_error(error, Position::NONE))
}

/// Effective replacement for the v1 log-only `stop_recording(id)` stub:
/// commits a real Stop operation against the typed recording address.
pub(crate) fn stop_recording_v2(name: String) -> Result<RecordRef, Box<EvalAltResult>> {
    record_ref_v2(name)?
        .stop()
        .map_err(|error| record_v2_error(error, Position::NONE))
}

/// Effective replacement for the v1 log-only `cancel_recording(id)`
/// stub: commits a real Cancel operation against the typed recording
/// address.
pub(crate) fn cancel_recording_v2(name: String) -> Result<RecordRef, Box<EvalAltResult>> {
    record_ref_v2(name)?
        .cancel()
        .map_err(|error| record_v2_error(error, Position::NONE))
}

fn record_v2_error(error: FoundationError, position: Position) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        error.to_string().into(),
        position,
    ))
}

pub(crate) fn install_v2(engine: &mut Engine) {
    fn strict<T>(result: Result<T, FoundationError>) -> Result<T, Box<EvalAltResult>> {
        result.map_err(|error| record_v2_error(error, Position::NONE))
    }

    engine
        .register_type_with_name::<RecordBuilder>("RecordBuilder")
        .register_type_with_name::<RecordRef>("RecordRef")
        .register_fn("record", record_builder_v2)
        .register_fn("record_ref", record_ref_v2)
        .register_fn("stop_recording", stop_recording_v2)
        .register_fn("cancel_recording", cancel_recording_v2)
        .register_fn("from", |builder: RecordBuilder, source: RefBase| {
            strict(builder.from(source))
        })
        .register_fn("from_group", |builder: RecordBuilder, source: RefBase| {
            strict(builder.from_group(source))
        })
        .register_fn(
            "from",
            |builder: RecordBuilder, source: super::group::GroupRef| {
                strict(builder.from(source.base().clone()))
            },
        )
        .register_fn(
            "from_group",
            |builder: RecordBuilder, source: super::group::GroupRef| {
                strict(builder.from_group(source.base().clone()))
            },
        )
        .register_fn("beats", |builder: RecordBuilder, beats: f64| {
            strict(builder.beats(beats))
        })
        .register_fn("beats", |builder: RecordBuilder, beats: i64| {
            strict(builder.beats(beats as f64))
        })
        .register_fn("seconds", |builder: RecordBuilder, seconds: f64| {
            strict(builder.seconds(seconds))
        })
        .register_fn("seconds", |builder: RecordBuilder, seconds: i64| {
            strict(builder.seconds(seconds as f64))
        })
        .register_fn("count_in", |builder: RecordBuilder, beats: f64| {
            strict(builder.count_in(beats))
        })
        .register_fn("count_in", |builder: RecordBuilder, beats: i64| {
            strict(builder.count_in(beats as f64))
        })
        .register_fn("metronome", RecordBuilder::metronome)
        .register_fn("to_file", |builder: RecordBuilder, path: String| {
            strict(builder.to_file(path))
        })
        .register_fn("channels", |builder: RecordBuilder, channels: i64| {
            strict(builder.channels(channels))
        })
        .register_fn("apply", |builder: RecordBuilder| strict(builder.apply()))
        .register_fn("start", |builder: RecordBuilder| strict(builder.start()))
        .register_fn("start_now", |builder: RecordBuilder| {
            strict(builder.start_now())
        })
        .register_fn("start", |reference: RecordRef| strict(reference.start()))
        .register_fn("start_now", |reference: RecordRef| {
            strict(reference.start_now())
        })
        .register_fn("stop", |reference: RecordRef| strict(reference.stop()))
        .register_fn("cancel", |reference: RecordRef| strict(reference.cancel()))
        .register_fn("remove", |reference: RecordRef| strict(reference.remove()))
        .register_fn("status", |reference: RecordRef| strict(reference.status()))
        .register_fn("sample", |reference: RecordRef| strict(reference.sample()));
}

#[cfg(test)]
mod v2_tests {
    use super::*;
    use crate::api::group::GroupBuilder;
    use vibelang_core::candidate::EntityKind;

    fn v2_identity() -> vibelang_core::candidate::EvaluationIdentity {
        vibelang_core::candidate::EvaluationIdentity::new(
            vibelang_core::candidate::LanguageContract::v2(
                vibelang_core::candidate::ContractDigest::from_bytes(b"record-v2-test"),
            ),
            vibelang_core::candidate::EngineInstanceId::new(),
            vibelang_core::mutation::RuntimeEpoch::new(),
        )
    }

    fn builder(name: &str) -> RecordBuilder {
        RecordBuilder::new(
            foundation::authoring_builder::<RecordingKind>(name, GroupScope::root()).unwrap(),
        )
    }

    fn declare_group(key: &str) -> RefBase {
        GroupBuilder::new(
            foundation::authoring_builder::<GroupKind>(key, GroupScope::root()).unwrap(),
        )
        .apply()
        .unwrap()
        .base()
        .clone()
    }

    #[test]
    fn v2_record_configuration_is_pure_and_strict() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        assert!(matches!(
            builder("take").beats(0.0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("take").beats(f64::NAN),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("take").seconds(0.0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("take").count_in(-1.0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("take").channels(0),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("take").channels(3),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(matches!(
            builder("take").to_file(" takes/one.wav".into()),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(_)
            ))
        ));
        assert!(
            builder("take")
                .from(foundation::authoring_ref::<SampleKind>("kick", GroupScope::root()).unwrap())
                .is_err(),
            "a non-group source must be rejected at the typed binding"
        );
        assert!(matches!(
            builder("take").apply(),
            Err(FoundationError::Candidate(
                CandidateError::InvalidAuthoring(message)
            )) if message.contains("source group")
        ));

        let candidate = foundation::finish_evaluation().unwrap();
        assert!(
            candidate.declarations().is_empty(),
            "rejected configuration must leave no candidate residue"
        );
    }

    #[test]
    fn v2_record_terminals_are_typed_and_lifecycle_distinct() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let drums = declare_group("drums");
        let dormant = builder("take")
            .from(drums.clone())
            .unwrap()
            .beats(16.0)
            .unwrap()
            .apply()
            .unwrap();
        assert_eq!(dormant.base().kind(), EntityKind::Recording);
        assert!(matches!(
            dormant.status(),
            Err(FoundationError::ObservationUnavailable)
        ));
        assert!(matches!(
            builder("take").from(drums.clone()).unwrap().apply(),
            Err(FoundationError::Candidate(
                CandidateError::DuplicateDeclaration { .. }
            ))
        ));
        let started = builder("take2")
            .from(drums)
            .unwrap()
            .seconds(30.0)
            .unwrap()
            .start()
            .unwrap();
        assert_eq!(started.base().kind(), EntityKind::Recording);

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 3);
        let effects = candidate
            .declarations()
            .iter()
            .filter(|declaration| declaration.address().kind() == EntityKind::Recording)
            .map(|declaration| {
                (
                    declaration.address().key().as_str().to_string(),
                    declaration.lifecycle().terminal_effect,
                )
            })
            .collect::<Vec<_>>();
        assert!(effects.contains(&("take".into(), TerminalEffect::Register)));
        assert!(effects.contains(&("take2".into(), TerminalEffect::Start)));
    }

    #[test]
    fn v2_record_admits_exactly_one_active_run() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let drums = declare_group("drums");
        let take = builder("take")
            .from(drums)
            .unwrap()
            .beats(16.0)
            .unwrap()
            .start()
            .unwrap();
        take.clone().start().unwrap();
        assert!(
            matches!(
                foundation::finish_evaluation(),
                Err(FoundationError::Candidate(
                    CandidateError::InvalidAuthoring(message)
                )) if message.contains("one active run")
            ),
            "a started declaration plus a start operation is two runs"
        );

        foundation::begin_evaluation(v2_identity()).unwrap();
        let drums = declare_group("drums");
        let take = builder("take").from(drums).unwrap().apply().unwrap();
        let take = take.start().unwrap();
        take.clone().stop().unwrap();
        take.cancel().unwrap();
        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(
            candidate.operations().len(),
            3,
            "stop and cancel must not count against the single active run"
        );
    }

    #[test]
    fn v2_record_completion_is_a_typed_sample_ref_without_physical_ids() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let drums = declare_group("drums");
        let take = builder("take")
            .from(drums)
            .unwrap()
            .beats(16.0)
            .unwrap()
            .to_file("takes/one.wav".into())
            .unwrap()
            .start()
            .unwrap();
        foundation::finish_evaluation().unwrap();

        let sample = take.sample().unwrap();
        assert_eq!(sample.base().kind(), EntityKind::Sample);
        assert_eq!(
            sample.base().address().key(),
            take.base().address().key(),
            "the completion handle must name the recording's own logical address"
        );
        assert_eq!(
            sample.base().address().module(),
            take.base().address().module()
        );
        assert_eq!(sample.base().identity(), take.base().identity());
        sample.base().typed::<SampleKind>().unwrap();
    }

    #[test]
    fn v2_record_rhai_surface_and_stop_cancel_replacements_author_from_script() {
        foundation::abort_evaluation();
        foundation::begin_evaluation(v2_identity()).unwrap();
        let mut engine = Engine::new();
        crate::foundation::register(&mut engine);
        install_v2(&mut engine);
        engine.register_fn(
            "group_ref",
            |name: String| -> Result<RefBase, Box<EvalAltResult>> {
                foundation::authoring_ref::<GroupKind>(&name, GroupScope::root())
                    .map_err(|error| record_v2_error(error, Position::NONE))
            },
        );
        declare_group("drums");
        let reference = engine
            .eval::<RecordRef>(
                r#"record("take1")
                    .from(group_ref("drums"))
                    .beats(16.0)
                    .count_in(4)
                    .metronome(true)
                    .to_file("takes/one.wav")
                    .channels(2)
                    .apply()"#,
            )
            .unwrap();
        assert_eq!(reference.base().kind(), EntityKind::Recording);
        engine
            .eval::<RecordRef>(r#"record_ref("take1").start()"#)
            .unwrap();
        engine
            .eval::<RecordRef>(r#"stop_recording("take1")"#)
            .unwrap();
        engine
            .eval::<RecordRef>(r#"cancel_recording("take1")"#)
            .unwrap();
        assert!(
            engine
                .eval::<RecordRef>(r#"record("bad").apply()"#)
                .is_err(),
            "a terminal without a source group must fail, not no-op"
        );
        assert!(engine
            .eval::<RecordBuilder>(r#"record("bad").beats(0.0)"#)
            .is_err());

        let candidate = foundation::finish_evaluation().unwrap();
        assert_eq!(candidate.declarations().len(), 2);
        assert_eq!(
            candidate.operations().len(),
            3,
            "start, stop, and cancel must each be real committed operations"
        );
    }
}
