/**
 * Type-safe message protocol for webview communication.
 *
 * All webview panels should use these types instead of loose { command: string } objects.
 * This provides compile-time type checking and autocomplete for message handling.
 */

// =============================================================================
// Common Messages (used by multiple panels)
// =============================================================================

export interface GoToSourceMessage {
    command: 'goToSource';
    file: string;
    line: number;
    column?: number;
}

export interface SelectEntityMessage {
    command: 'select';
    type: 'group' | 'voice' | 'pattern' | 'melody' | 'sequence' | 'effect';
    id: string;
}

// =============================================================================
// Mixer Panel Messages
// =============================================================================

export interface MixerSetAmpMessage {
    command: 'setAmp';
    path: string;
    value: number;
}

export interface MixerSetPanMessage {
    command: 'setPan';
    path: string;
    value: number;
}

export interface MixerMuteMessage {
    command: 'mute';
    path: string;
}

export interface MixerSoloMessage {
    command: 'solo';
    path: string;
}

export interface MixerSelectMessage {
    command: 'select';
    path: string;
}

export type MixerMessage =
    | MixerSetAmpMessage
    | MixerSetPanMessage
    | MixerMuteMessage
    | MixerSoloMessage
    | MixerSelectMessage;

// =============================================================================
// Inspector Panel Messages
// =============================================================================

export interface InspectorInteractionStartMessage {
    command: 'interactionStart';
}

export interface InspectorInteractionEndMessage {
    command: 'interactionEnd';
}

export interface InspectorSetParamMessage {
    command: 'setParam';
    entityType: 'group' | 'voice' | 'effect';
    entityId: string;
    param: string;
    value: number;
}

export interface InspectorMuteMessage {
    command: 'mute';
    entityType: 'group' | 'voice';
    entityId: string;
}

export interface InspectorSoloMessage {
    command: 'solo';
    entityType: 'group';
    entityId: string;
}

export interface InspectorStartMessage {
    command: 'start';
    entityType: 'pattern' | 'melody' | 'sequence';
    entityId: string;
}

export interface InspectorStopMessage {
    command: 'stop';
    entityType: 'pattern' | 'melody' | 'sequence';
    entityId: string;
}

export type InspectorMessage =
    | InspectorInteractionStartMessage
    | InspectorInteractionEndMessage
    | InspectorSetParamMessage
    | InspectorMuteMessage
    | InspectorSoloMessage
    | InspectorStartMessage
    | InspectorStopMessage
    | GoToSourceMessage;

// =============================================================================
// Arrangement Timeline Messages
// =============================================================================

export interface TimelineToggleTransportMessage {
    command: 'toggleTransport';
}

export interface TimelineStopTransportMessage {
    command: 'stopTransport';
}

export interface TimelineSeekMessage {
    command: 'seek';
    beat: number;
}

export interface TimelineStartPatternMessage {
    command: 'startPattern';
    name: string;
}

export interface TimelineStopPatternMessage {
    command: 'stopPattern';
    name: string;
}

export interface TimelineStartMelodyMessage {
    command: 'startMelody';
    name: string;
}

export interface TimelineStopMelodyMessage {
    command: 'stopMelody';
    name: string;
}

export interface TimelineStartSequenceMessage {
    command: 'startSequence';
    name: string;
}

export interface TimelineStopSequenceMessage {
    command: 'stopSequence';
    name: string;
}

export interface TimelineMuteTrackMessage {
    command: 'muteTrack';
    trackType: 'group' | 'sequence';
    trackId: string;
}

export interface TimelineSoloTrackMessage {
    command: 'soloTrack';
    trackType: 'group' | 'sequence';
    trackId: string;
}

export interface TimelineAddAutomationLaneMessage {
    command: 'addAutomationLane';
    target: {
        entityType: 'group' | 'voice' | 'effect';
        entityId: string;
        param: string;
    };
}

export interface TimelineRemoveAutomationLaneMessage {
    command: 'removeAutomationLane';
    laneId: string;
}

export interface TimelineToggleAutomationLaneMessage {
    command: 'toggleAutomationLane';
    laneId: string;
}

export interface TimelineAddAutomationPointMessage {
    command: 'addAutomationPoint';
    laneId: string;
    beat: number;
    value: number;
}

export interface TimelineUpdateAutomationPointMessage {
    command: 'updateAutomationPoint';
    laneId: string;
    pointIndex: number;
    beat?: number;
    value?: number;
}

export interface TimelineRemoveAutomationPointMessage {
    command: 'removeAutomationPoint';
    laneId: string;
    pointIndex: number;
}

export interface TimelineSetAutomationCurveTypeMessage {
    command: 'setAutomationCurveType';
    laneId: string;
    pointIndex: number;
    curveType: 'linear' | 'exponential' | 'smooth' | 'step';
}

export interface TimelineGenerateAutomationCodeMessage {
    command: 'generateAutomationCode';
    laneId: string;
}

export interface TimelineClearAutomationLaneMessage {
    command: 'clearAutomationLane';
    laneId: string;
}

export type ArrangementTimelineMessage =
    | GoToSourceMessage
    | TimelineToggleTransportMessage
    | TimelineStopTransportMessage
    | TimelineSeekMessage
    | TimelineStartPatternMessage
    | TimelineStopPatternMessage
    | TimelineStartMelodyMessage
    | TimelineStopMelodyMessage
    | TimelineStartSequenceMessage
    | TimelineStopSequenceMessage
    | TimelineMuteTrackMessage
    | TimelineSoloTrackMessage
    | TimelineAddAutomationLaneMessage
    | TimelineRemoveAutomationLaneMessage
    | TimelineToggleAutomationLaneMessage
    | TimelineAddAutomationPointMessage
    | TimelineUpdateAutomationPointMessage
    | TimelineRemoveAutomationPointMessage
    | TimelineSetAutomationCurveTypeMessage
    | TimelineGenerateAutomationCodeMessage
    | TimelineClearAutomationLaneMessage;

// =============================================================================
// Sample Browser Messages
// =============================================================================

export interface SampleBrowserLoadMessage {
    command: 'loadSample';
}

export interface SampleBrowserPreviewMessage {
    command: 'previewSample';
    sampleId: string;
}

export interface SampleBrowserStopPreviewMessage {
    command: 'stopPreview';
}

export interface SampleBrowserInsertSampleCodeMessage {
    command: 'insertSampleCode';
    sampleId: string;
    samplePath: string;
}

export interface SampleBrowserInsertSynthDefCodeMessage {
    command: 'insertSynthDefCode';
    synthDefName: string;
}

export interface SampleBrowserCopyToClipboardMessage {
    command: 'copyToClipboard';
    text: string;
}

export type SampleBrowserMessage =
    | SampleBrowserLoadMessage
    | SampleBrowserPreviewMessage
    | SampleBrowserStopPreviewMessage
    | SampleBrowserInsertSampleCodeMessage
    | SampleBrowserInsertSynthDefCodeMessage
    | SampleBrowserCopyToClipboardMessage
    | GoToSourceMessage;

// =============================================================================
// Effect Rack Messages
// =============================================================================

export interface EffectRackSelectGroupMessage {
    command: 'selectGroup';
    groupPath: string;
}

export interface EffectRackUpdateParamMessage {
    command: 'updateEffectParam';
    effectId: string;
    param: string;
    value: number;
}

export interface EffectRackInsertCodeMessage {
    command: 'insertEffectCode';
    effectName: string;
    groupPath: string;
}

export type EffectRackMessage =
    | EffectRackSelectGroupMessage
    | EffectRackUpdateParamMessage
    | EffectRackInsertCodeMessage
    | GoToSourceMessage;

// =============================================================================
// Pattern Editor Messages
// =============================================================================

export interface PatternEditorReadyMessage {
    command: 'ready';
}

export interface PatternEditorSelectGroupMessage {
    command: 'selectGroup';
    groupPath: string;
}

export interface PatternEditorAddPatternMessage {
    command: 'addPattern';
    patternName: string;
}

export interface PatternEditorRemovePatternMessage {
    command: 'removePattern';
    laneIndex: number;
}

export interface PatternEditorUpdateStepMessage {
    command: 'updateStep';
    laneIndex: number;
    stepIndex: number;
    velocity: number;
}

export interface PatternEditorUpdateLaneMessage {
    command: 'updateLane';
    laneIndex: number;
    grid: number[];
}

export interface PatternEditorResizeGridMessage {
    command: 'resizeGrid';
    stepsPerBar: number;
    numBars: number;
}

export interface PatternEditorTogglePlaybackMessage {
    command: 'togglePlayback';
    laneIndex: number;
}

export interface PatternEditorApplyEuclideanMessage {
    command: 'applyEuclidean';
    laneIndex: number;
    hits: number;
    steps: number;
    rotation: number;
}

export interface PatternEditorClearLaneMessage {
    command: 'clearLane';
    laneIndex: number;
}

export type PatternEditorMessage =
    | PatternEditorReadyMessage
    | PatternEditorSelectGroupMessage
    | PatternEditorAddPatternMessage
    | PatternEditorRemovePatternMessage
    | PatternEditorUpdateStepMessage
    | PatternEditorUpdateLaneMessage
    | PatternEditorResizeGridMessage
    | PatternEditorTogglePlaybackMessage
    | PatternEditorApplyEuclideanMessage
    | PatternEditorClearLaneMessage
    | GoToSourceMessage;

// =============================================================================
// Melody Editor Messages
// =============================================================================

export interface MelodyEditorLoadMessage {
    command: 'loadMelody';
    melodyName: string;
}

export interface MelodyEditorUpdateGridMessage {
    command: 'updateGrid';
    notes: Array<{
        note: number;
        startBeat: number;
        duration: number;
        velocity: number;
    }>;
}

export interface MelodyEditorResizeGridMessage {
    command: 'resizeGrid';
    numBars: number;
    gridSize: number;
}

export interface MelodyEditorTogglePlaybackMessage {
    command: 'togglePlayback';
}

export interface MelodyEditorPlayNoteMessage {
    command: 'playNote';
    note: number;
    velocity: number;
}

export type MelodyEditorMessage =
    | MelodyEditorLoadMessage
    | MelodyEditorUpdateGridMessage
    | MelodyEditorResizeGridMessage
    | MelodyEditorTogglePlaybackMessage
    | MelodyEditorPlayNoteMessage
    | GoToSourceMessage;

// =============================================================================
// Sound Designer Messages
// =============================================================================

export interface SoundDesignerGenerateCodeMessage {
    command: 'generateCode';
    code: string;
}

export interface SoundDesignerSavePresetMessage {
    command: 'savePreset';
    preset: unknown;
}

export interface SoundDesignerLoadPresetMessage {
    command: 'loadPreset';
}

export interface SoundDesignerShowInfoMessage {
    command: 'showInfo';
    message: string;
}

export interface SoundDesignerShowErrorMessage {
    command: 'showError';
    message: string;
}

export interface SoundDesignerPreviewNoteMessage {
    command: 'previewNote';
    note: number;
    velocity: number;
}

export type SoundDesignerMessage =
    | SoundDesignerGenerateCodeMessage
    | SoundDesignerSavePresetMessage
    | SoundDesignerLoadPresetMessage
    | SoundDesignerShowInfoMessage
    | SoundDesignerShowErrorMessage
    | SoundDesignerPreviewNoteMessage;

// =============================================================================
// Host → Webview Messages (sent from extension to webview)
// =============================================================================

export interface StateUpdateMessage {
    type: 'stateUpdate';
    data: unknown;
}

export interface TransportUpdateMessage {
    type: 'transportUpdate';
    transport: {
        bpm: number;
        running: boolean;
        current_beat: number;
    };
}

export interface MeterUpdateMessage {
    type: 'meterUpdate';
    meters: Record<string, { peak: number; rms: number }>;
}

export interface LanesUpdateMessage {
    type: 'lanesUpdate';
    lanes: unknown[];
}

export interface MelodyListMessage {
    type: 'melodyList';
    melodies: unknown[];
}

export interface MelodyUpdateMessage {
    type: 'melodyUpdate';
    melody: unknown;
}

export interface AutomationUpdateMessage {
    type: 'automationUpdate';
    lanes: unknown[];
}

export type HostToWebviewMessage =
    | StateUpdateMessage
    | TransportUpdateMessage
    | MeterUpdateMessage
    | LanesUpdateMessage
    | MelodyListMessage
    | MelodyUpdateMessage
    | AutomationUpdateMessage;

// =============================================================================
// Type Guards
// =============================================================================

export function isMixerMessage(message: unknown): message is MixerMessage {
    if (typeof message !== 'object' || message === null) return false;
    const cmd = (message as { command?: string }).command;
    return ['setAmp', 'setPan', 'mute', 'solo', 'select'].includes(cmd ?? '');
}

export function isGoToSourceMessage(message: unknown): message is GoToSourceMessage {
    if (typeof message !== 'object' || message === null) return false;
    const m = message as GoToSourceMessage;
    return m.command === 'goToSource' && typeof m.file === 'string' && typeof m.line === 'number';
}
