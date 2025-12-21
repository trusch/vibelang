/**
 * VibeLang Runtime API Types
 *
 * TypeScript interfaces generated from REST_API_SPEC.yaml
 * These types represent the data structures used by the VibeLang runtime API.
 */

// =============================================================================
// Transport
// =============================================================================

export interface TimeSignature {
    numerator: number;
    denominator: number;
}

export interface TransportState {
    bpm: number;
    time_signature: TimeSignature;
    running: boolean;
    current_beat: number;
    quantization_beats: number;
    /** Loop length in beats from longest active sequence (null if no sequences) */
    loop_beats?: number;
    /** Current beat position within the loop (current_beat % loop_beats) */
    loop_beat?: number;
    /** Server timestamp when this state was captured (ms since Unix epoch) */
    server_time_ms?: number;
}

export interface TransportUpdate {
    bpm?: number;
    time_signature?: TimeSignature;
    quantization_beats?: number;
}

// =============================================================================
// Source Location (for navigation to code)
// =============================================================================

export interface SourceLocation {
    file?: string;
    line?: number;
    column?: number;
}

// =============================================================================
// Groups
// =============================================================================

export interface Group {
    name: string;
    path: string;
    parent_path?: string;
    children: string[];
    node_id: number;
    audio_bus: number;
    link_synth_node_id?: number;
    muted: boolean;
    soloed: boolean;
    params: Record<string, number>;
    synth_node_ids?: number[];
    source_location?: SourceLocation;
}

export interface GroupCreate {
    name: string;
    parent_path?: string;
    params?: Record<string, number>;
}

export interface GroupUpdate {
    params?: Record<string, number>;
}

// =============================================================================
// Voices
// =============================================================================

export interface Voice {
    name: string;
    synth_name: string;
    polyphony: number;
    gain: number;
    group_path: string;
    group_name?: string;
    output_bus?: number;
    muted: boolean;
    soloed: boolean;
    params: Record<string, number>;
    sfz_instrument?: string;
    vst_instrument?: string;
    active_notes?: number[];
    sustained_notes?: number[];
    running: boolean;
    running_node_id?: number;
    source_location?: SourceLocation;
}

export interface VoiceCreate {
    name: string;
    synth_name?: string;
    polyphony?: number;
    gain?: number;
    group_path?: string;
    params?: Record<string, number>;
    sample?: string;
    sfz?: string;
}

export interface VoiceUpdate {
    synth_name?: string;
    polyphony?: number;
    gain?: number;
    params?: Record<string, number>;
}

// =============================================================================
// Patterns
// =============================================================================

export type LoopState = 'stopped' | 'queued' | 'playing' | 'queued_stop';

export interface LoopStatus {
    state: LoopState;
    start_beat?: number;
    stop_beat?: number;
}

export interface PatternEvent {
    beat: number;
    params?: Record<string, number>;
}

export interface Pattern {
    name: string;
    voice_name: string;
    group_path: string;
    loop_beats: number;
    events: PatternEvent[];
    params?: Record<string, number>;
    status: LoopStatus;
    is_looping: boolean;
    source_location?: SourceLocation;
    /** Original step pattern string (e.g., "x..x..x.|x.x.x.x.") for visual editing */
    step_pattern?: string;
}

export interface PatternCreate {
    name: string;
    voice_name: string;
    group_path?: string;
    loop_beats?: number;
    events?: PatternEvent[];
    pattern_string?: string;
    params?: Record<string, number>;
}

export interface PatternUpdate {
    events?: PatternEvent[];
    pattern_string?: string;
    loop_beats?: number;
    params?: Record<string, number>;
}

// =============================================================================
// Melodies
// =============================================================================

export interface MelodyEvent {
    beat: number;
    note: string;
    frequency?: number;
    duration?: number;
    velocity?: number;
    params?: Record<string, number>;
}

export interface Melody {
    name: string;
    voice_name: string;
    group_path: string;
    loop_beats: number;
    events: MelodyEvent[];
    params?: Record<string, number>;
    status: LoopStatus;
    is_looping: boolean;
    source_location?: SourceLocation;
    /** Notes pattern strings for visual editing (one per lane). */
    notes_patterns?: string[];
}

export interface MelodyCreate {
    name: string;
    voice_name: string;
    group_path?: string;
    loop_beats?: number;
    events?: MelodyEvent[];
    /** Single melody string (backward compatible). */
    melody_string?: string;
    /** Multiple lanes for polyphonic melodies. */
    lanes?: string[];
    params?: Record<string, number>;
}

export interface MelodyUpdate {
    events?: MelodyEvent[];
    /** Single melody string (backward compatible). */
    melody_string?: string;
    /** Multiple lanes for polyphonic melodies. */
    lanes?: string[];
    loop_beats?: number;
    params?: Record<string, number>;
}

// =============================================================================
// Sequences
// =============================================================================

export type ClipType = 'pattern' | 'melody' | 'fade' | 'sequence';

export interface SequenceClip {
    type: ClipType;
    name: string;
    start_beat: number;
    end_beat?: number;
    duration_beats?: number;
    once?: boolean;
}

export interface Sequence {
    name: string;
    loop_beats: number;
    clips: SequenceClip[];
    play_once?: boolean;
    active?: boolean;
    source_location?: SourceLocation;
}

export interface SequenceCreate {
    name: string;
    loop_beats?: number;
    clips?: SequenceClip[];
}

export interface SequenceUpdate {
    loop_beats?: number;
    clips?: SequenceClip[];
}

// =============================================================================
// Effects
// =============================================================================

export interface Effect {
    id: string;
    synthdef_name: string;
    group_path: string;
    node_id?: number;
    bus_in?: number;
    bus_out?: number;
    params: Record<string, number>;
    position?: number;
    vst_plugin?: string;
    source_location?: SourceLocation;
}

export interface EffectCreate {
    id?: string;
    synthdef_name: string;
    group_path: string;
    params?: Record<string, number>;
    position?: number;
}

export interface EffectUpdate {
    params?: Record<string, number>;
}

// =============================================================================
// Samples
// =============================================================================

export interface SampleSlice {
    name: string;
    start_frame: number;
    end_frame: number;
}

export interface Sample {
    id: string;
    path: string;
    buffer_id: number;
    num_channels: number;
    num_frames: number;
    sample_rate: number;
    synthdef_name: string;
    slices?: SampleSlice[];
}

export interface SampleLoad {
    id?: string;
    path: string;
}

// =============================================================================
// SynthDefs
// =============================================================================

export interface SynthDefParam {
    name: string;
    default_value: number;
    min_value?: number;
    max_value?: number;
}

export type SynthDefSource = 'builtin' | 'user' | 'stdlib';

export interface SynthDef {
    name: string;
    params: SynthDefParam[];
    source?: SynthDefSource;
}

// =============================================================================
// Fades
// =============================================================================

export type FadeTargetType = 'group' | 'voice' | 'effect';

export interface ActiveFade {
    id: string;
    name?: string;
    target_type: FadeTargetType;
    target_name: string;
    param_name: string;
    start_value: number;
    target_value: number;
    current_value?: number;
    duration_beats: number;
    start_beat?: number;
    progress: number;
}

export interface FadeCreate {
    target_type: FadeTargetType;
    target_name: string;
    param_name: string;
    start_value?: number;
    target_value: number;
    duration_beats: number;
}

// =============================================================================
// Live State
// =============================================================================

export interface ActiveSynth {
    node_id: number;
    synthdef_name: string;
    voice_name?: string;
    group_path?: string;
    created_at_beat?: number;
}

export interface ActiveSequence {
    name: string;
    start_beat: number;
    current_position: number;
    loop_beats: number;
    iteration?: number;
    play_once?: boolean;
}

export interface LiveState {
    transport: TransportState;
    active_synths: ActiveSynth[];
    active_sequences: ActiveSequence[];
    active_fades: ActiveFade[];
    active_notes?: Record<string, number[]>;
    patterns_status?: Record<string, LoopStatus>;
    melodies_status?: Record<string, LoopStatus>;
}

// =============================================================================
// Metering
// =============================================================================

export interface MeterLevel {
    peak_left: number;
    peak_right: number;
    rms_left: number;
    rms_right: number;
}

export type MeterLevels = Record<string, MeterLevel>;

// =============================================================================
// Full Session State (aggregated for the extension)
// =============================================================================

export interface SessionState {
    connected: boolean;
    transport: TransportState;
    groups: Group[];
    voices: Voice[];
    patterns: Pattern[];
    melodies: Melody[];
    sequences: Sequence[];
    effects: Effect[];
    samples: Sample[];
    synthdefs: SynthDef[];
    live: LiveState;
}

// =============================================================================
// Entity union type for selection
// =============================================================================

export type EntityType = 'group' | 'voice' | 'pattern' | 'melody' | 'sequence' | 'effect' | 'sample';

export interface EntitySelection {
    type: EntityType;
    id: string;
}

// =============================================================================
// Tree item types for Session Explorer
// =============================================================================

export interface GroupTreeItem {
    type: 'group';
    group: Group;
    voices: Voice[];
    patterns: Pattern[];
    melodies: Melody[];
    effects: Effect[];
    children: GroupTreeItem[];
}
