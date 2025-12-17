/**
 * VibeLang Runtime Manager
 *
 * REST API client for communicating with a running VibeLang instance.
 * Handles auto-discovery, connection management, and all API calls.
 */

import * as vscode from 'vscode';
import {
    TransportState,
    TransportUpdate,
    Group,
    GroupUpdate,
    Voice,
    VoiceCreate,
    VoiceUpdate,
    Pattern,
    PatternUpdate,
    Melody,
    MelodyUpdate,
    Sequence,
    SequenceUpdate,
    Effect,
    EffectUpdate,
    Sample,
    SynthDef,
    ActiveFade,
    LiveState,
    SessionState,
    MeterLevels,
} from './types';

const DEFAULT_PORT = 1606;
const DEFAULT_HOST = 'localhost';
const POLL_INTERVAL = 500; // ms between state polls
const CONNECTION_TIMEOUT = 2000; // ms

export type ConnectionStatus = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface RuntimeManagerOptions {
    host?: string;
    port?: number;
    autoConnect?: boolean;
}

export class RuntimeManager {
    private _host: string;
    private _port: number;
    private _status: ConnectionStatus = 'disconnected';
    private _pollTimer: NodeJS.Timeout | null = null;
    private _lastError: string | null = null;

    // Event emitters
    private _onStatusChange = new vscode.EventEmitter<ConnectionStatus>();
    private _onStateUpdate = new vscode.EventEmitter<SessionState>();
    private _onError = new vscode.EventEmitter<string>();

    public readonly onStatusChange = this._onStatusChange.event;
    public readonly onStateUpdate = this._onStateUpdate.event;
    public readonly onError = this._onError.event;

    // Cached state
    private _state: SessionState | null = null;

    constructor(options: RuntimeManagerOptions = {}) {
        this._host = options.host ?? DEFAULT_HOST;
        this._port = options.port ?? DEFAULT_PORT;

        if (options.autoConnect !== false) {
            this.tryConnect();
        }
    }

    // ==========================================================================
    // Connection Management
    // ==========================================================================

    get status(): ConnectionStatus {
        return this._status;
    }

    get baseUrl(): string {
        return `http://${this._host}:${this._port}`;
    }

    get state(): SessionState | null {
        return this._state;
    }

    get lastError(): string | null {
        return this._lastError;
    }

    private setStatus(status: ConnectionStatus) {
        if (this._status !== status) {
            this._status = status;
            this._onStatusChange.fire(status);
        }
    }

    async tryConnect(): Promise<boolean> {
        this.setStatus('connecting');

        try {
            const transport = await this.getTransport();
            if (transport) {
                this.setStatus('connected');
                this.startPolling();
                return true;
            }
        } catch (e) {
            this._lastError = e instanceof Error ? e.message : String(e);
        }

        this.setStatus('disconnected');
        return false;
    }

    disconnect() {
        this.stopPolling();
        this.setStatus('disconnected');
        this._state = null;
    }

    setConnection(host: string, port: number) {
        this.disconnect();
        this._host = host;
        this._port = port;
        this.tryConnect();
    }

    private startPolling() {
        this.stopPolling();
        this._pollTimer = setInterval(() => this.pollState(), POLL_INTERVAL);
        // Immediate first poll
        this.pollState();
    }

    private stopPolling() {
        if (this._pollTimer) {
            clearInterval(this._pollTimer);
            this._pollTimer = null;
        }
    }

    private async pollState() {
        try {
            const state = await this.fetchFullState();
            if (state) {
                this._state = state;
                this._onStateUpdate.fire(state);
            }
        } catch (e) {
            this._lastError = e instanceof Error ? e.message : String(e);
            this.setStatus('error');
            this._onError.fire(this._lastError);
        }
    }

    // ==========================================================================
    // HTTP Helpers
    // ==========================================================================

    private async fetch<T>(
        path: string,
        options: RequestInit = {}
    ): Promise<T | null> {
        const url = `${this.baseUrl}${path}`;

        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), CONNECTION_TIMEOUT);

        try {
            const response = await fetch(url, {
                ...options,
                signal: controller.signal,
                headers: {
                    'Content-Type': 'application/json',
                    ...options.headers,
                },
            });

            clearTimeout(timeoutId);

            if (!response.ok) {
                if (response.status === 404) {
                    return null;
                }
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }

            if (response.status === 204) {
                return null;
            }

            return await response.json() as T;
        } catch (e) {
            clearTimeout(timeoutId);
            if (e instanceof Error && e.name === 'AbortError') {
                throw new Error('Connection timeout');
            }
            throw e;
        }
    }

    private async get<T>(path: string): Promise<T | null> {
        return this.fetch<T>(path);
    }

    private async post<T>(path: string, body?: unknown): Promise<T | null> {
        return this.fetch<T>(path, {
            method: 'POST',
            body: body ? JSON.stringify(body) : undefined,
        });
    }

    private async patch<T>(path: string, body: unknown): Promise<T | null> {
        return this.fetch<T>(path, {
            method: 'PATCH',
            body: JSON.stringify(body),
        });
    }

    private async put<T>(path: string, body: unknown): Promise<T | null> {
        return this.fetch<T>(path, {
            method: 'PUT',
            body: JSON.stringify(body),
        });
    }

    private async delete(path: string): Promise<boolean> {
        try {
            await this.fetch(path, { method: 'DELETE' });
            return true;
        } catch {
            return false;
        }
    }

    // ==========================================================================
    // Full State Fetch
    // ==========================================================================

    async fetchFullState(): Promise<SessionState | null> {
        // Fetch all entities in parallel for efficiency
        const [
            transport,
            groups,
            voices,
            patterns,
            melodies,
            sequences,
            effects,
            samples,
            synthdefs,
            live,
        ] = await Promise.all([
            this.getTransport(),
            this.getGroups(),
            this.getVoices(),
            this.getPatterns(),
            this.getMelodies(),
            this.getSequences(),
            this.getEffects(),
            this.getSamples(),
            this.getSynthDefs(),
            this.getLiveState(),
        ]);

        if (!transport) {
            return null;
        }

        return {
            connected: true,
            transport,
            groups: groups ?? [],
            voices: voices ?? [],
            patterns: patterns ?? [],
            melodies: melodies ?? [],
            sequences: sequences ?? [],
            effects: effects ?? [],
            samples: samples ?? [],
            synthdefs: synthdefs ?? [],
            live: live ?? {
                transport,
                active_synths: [],
                active_sequences: [],
                active_fades: [],
            },
        };
    }

    // ==========================================================================
    // Transport API
    // ==========================================================================

    async getTransport(): Promise<TransportState | null> {
        return this.get<TransportState>('/transport');
    }

    async updateTransport(update: TransportUpdate): Promise<TransportState | null> {
        return this.patch<TransportState>('/transport', update);
    }

    async startTransport(): Promise<TransportState | null> {
        return this.post<TransportState>('/transport/start');
    }

    async stopTransport(): Promise<TransportState | null> {
        return this.post<TransportState>('/transport/stop');
    }

    async seekTransport(beat: number): Promise<TransportState | null> {
        return this.post<TransportState>('/transport/seek', { beat });
    }

    // ==========================================================================
    // Groups API
    // ==========================================================================

    async getGroups(): Promise<Group[] | null> {
        return this.get<Group[]>('/groups');
    }

    async getGroup(path: string): Promise<Group | null> {
        return this.get<Group>(`/groups/${encodeURIComponent(path)}`);
    }

    async updateGroup(path: string, update: GroupUpdate): Promise<Group | null> {
        return this.patch<Group>(`/groups/${encodeURIComponent(path)}`, update);
    }

    async muteGroup(path: string): Promise<Group | null> {
        return this.post<Group>(`/groups/${encodeURIComponent(path)}/mute`);
    }

    async unmuteGroup(path: string): Promise<Group | null> {
        return this.post<Group>(`/groups/${encodeURIComponent(path)}/unmute`);
    }

    async soloGroup(path: string): Promise<Group | null> {
        return this.post<Group>(`/groups/${encodeURIComponent(path)}/solo`);
    }

    async unsoloGroup(path: string): Promise<Group | null> {
        return this.post<Group>(`/groups/${encodeURIComponent(path)}/unsolo`);
    }

    async setGroupParam(
        path: string,
        param: string,
        value: number,
        fadeBeats?: number
    ): Promise<void> {
        await this.put(`/groups/${encodeURIComponent(path)}/params/${param}`, {
            value,
            fade_beats: fadeBeats,
        });
    }

    // ==========================================================================
    // Voices API
    // ==========================================================================

    async getVoices(): Promise<Voice[] | null> {
        return this.get<Voice[]>('/voices');
    }

    async getVoice(name: string): Promise<Voice | null> {
        return this.get<Voice>(`/voices/${encodeURIComponent(name)}`);
    }

    async updateVoice(name: string, update: VoiceUpdate): Promise<Voice | null> {
        return this.patch<Voice>(`/voices/${encodeURIComponent(name)}`, update);
    }

    async triggerVoice(name: string, params?: Record<string, number>): Promise<void> {
        await this.post(`/voices/${encodeURIComponent(name)}/trigger`, { params });
    }

    async stopVoice(name: string): Promise<void> {
        await this.post(`/voices/${encodeURIComponent(name)}/stop`);
    }

    async noteOn(name: string, note: number, velocity = 100): Promise<void> {
        await this.post(`/voices/${encodeURIComponent(name)}/note-on`, { note, velocity });
    }

    async noteOff(name: string, note: number): Promise<void> {
        await this.post(`/voices/${encodeURIComponent(name)}/note-off`, { note });
    }

    async muteVoice(name: string): Promise<void> {
        await this.post(`/voices/${encodeURIComponent(name)}/mute`);
    }

    async unmuteVoice(name: string): Promise<void> {
        await this.post(`/voices/${encodeURIComponent(name)}/unmute`);
    }

    async setVoiceParam(
        name: string,
        param: string,
        value: number,
        fadeBeats?: number
    ): Promise<void> {
        await this.put(`/voices/${encodeURIComponent(name)}/params/${param}`, {
            value,
            fade_beats: fadeBeats,
        });
    }

    // ==========================================================================
    // Patterns API
    // ==========================================================================

    async getPatterns(): Promise<Pattern[] | null> {
        return this.get<Pattern[]>('/patterns');
    }

    async getPattern(name: string): Promise<Pattern | null> {
        return this.get<Pattern>(`/patterns/${encodeURIComponent(name)}`);
    }

    async updatePattern(name: string, update: PatternUpdate): Promise<Pattern | null> {
        return this.patch<Pattern>(`/patterns/${encodeURIComponent(name)}`, update);
    }

    async startPattern(name: string, quantizeBeats?: number): Promise<Pattern | null> {
        return this.post<Pattern>(`/patterns/${encodeURIComponent(name)}/start`, {
            quantize_beats: quantizeBeats,
        });
    }

    async stopPattern(name: string, quantizeBeats?: number): Promise<Pattern | null> {
        return this.post<Pattern>(`/patterns/${encodeURIComponent(name)}/stop`, {
            quantize_beats: quantizeBeats,
        });
    }

    // ==========================================================================
    // Melodies API
    // ==========================================================================

    async getMelodies(): Promise<Melody[] | null> {
        return this.get<Melody[]>('/melodies');
    }

    async getMelody(name: string): Promise<Melody | null> {
        return this.get<Melody>(`/melodies/${encodeURIComponent(name)}`);
    }

    async updateMelody(name: string, update: MelodyUpdate): Promise<Melody | null> {
        return this.patch<Melody>(`/melodies/${encodeURIComponent(name)}`, update);
    }

    async startMelody(name: string, quantizeBeats?: number): Promise<void> {
        await this.post(`/melodies/${encodeURIComponent(name)}/start`, {
            quantize_beats: quantizeBeats,
        });
    }

    async stopMelody(name: string): Promise<void> {
        await this.post(`/melodies/${encodeURIComponent(name)}/stop`);
    }

    // ==========================================================================
    // Sequences API
    // ==========================================================================

    async getSequences(): Promise<Sequence[] | null> {
        return this.get<Sequence[]>('/sequences');
    }

    async getSequence(name: string): Promise<Sequence | null> {
        return this.get<Sequence>(`/sequences/${encodeURIComponent(name)}`);
    }

    async updateSequence(name: string, update: SequenceUpdate): Promise<Sequence | null> {
        return this.patch<Sequence>(`/sequences/${encodeURIComponent(name)}`, update);
    }

    async startSequence(name: string, playOnce = false): Promise<void> {
        await this.post(`/sequences/${encodeURIComponent(name)}/start`, { play_once: playOnce });
    }

    async stopSequence(name: string): Promise<void> {
        await this.post(`/sequences/${encodeURIComponent(name)}/stop`);
    }

    async pauseSequence(name: string): Promise<void> {
        await this.post(`/sequences/${encodeURIComponent(name)}/pause`);
    }

    // ==========================================================================
    // Effects API
    // ==========================================================================

    async getEffects(): Promise<Effect[] | null> {
        return this.get<Effect[]>('/effects');
    }

    async getEffect(id: string): Promise<Effect | null> {
        return this.get<Effect>(`/effects/${encodeURIComponent(id)}`);
    }

    async updateEffect(id: string, update: EffectUpdate): Promise<Effect | null> {
        return this.patch<Effect>(`/effects/${encodeURIComponent(id)}`, update);
    }

    async removeEffect(id: string): Promise<boolean> {
        return this.delete(`/effects/${encodeURIComponent(id)}`);
    }

    async setEffectParam(
        id: string,
        param: string,
        value: number,
        fadeBeats?: number
    ): Promise<void> {
        await this.put(`/effects/${encodeURIComponent(id)}/params/${param}`, {
            value,
            fade_beats: fadeBeats,
        });
    }

    // ==========================================================================
    // Samples API
    // ==========================================================================

    async getSamples(): Promise<Sample[] | null> {
        return this.get<Sample[]>('/samples');
    }

    async getSample(id: string): Promise<Sample | null> {
        return this.get<Sample>(`/samples/${encodeURIComponent(id)}`);
    }

    async loadSample(path: string, id?: string): Promise<Sample | null> {
        return this.post<Sample>('/samples', { id, path });
    }

    async deleteSample(id: string): Promise<boolean> {
        return this.delete(`/samples/${encodeURIComponent(id)}`);
    }

    // ==========================================================================
    // SynthDefs API
    // ==========================================================================

    async getSynthDefs(): Promise<SynthDef[] | null> {
        return this.get<SynthDef[]>('/synthdefs');
    }

    async getSynthDef(name: string): Promise<SynthDef | null> {
        return this.get<SynthDef>(`/synthdefs/${encodeURIComponent(name)}`);
    }

    // ==========================================================================
    // Fades API
    // ==========================================================================

    async getFades(): Promise<ActiveFade[] | null> {
        return this.get<ActiveFade[]>('/fades');
    }

    // ==========================================================================
    // Live State API
    // ==========================================================================

    async getLiveState(): Promise<LiveState | null> {
        return this.get<LiveState>('/live');
    }

    async getMeters(): Promise<MeterLevels | null> {
        return this.get<MeterLevels>('/live/meters');
    }

    // ==========================================================================
    // Eval API
    // ==========================================================================

    async evalCode(code: string): Promise<{ success: boolean; result?: string; error?: string }> {
        const result = await this.post<{ success: boolean; result?: string; error?: string }>('/eval', { code });
        return result ?? { success: false, error: 'No response from server' };
    }

    // ==========================================================================
    // Voice Management
    // ==========================================================================

    async createVoice(voice: VoiceCreate): Promise<Voice | null> {
        return this.post<Voice>('/voices', voice);
    }

    async deleteVoice(name: string): Promise<boolean> {
        return this.delete(`/voices/${encodeURIComponent(name)}`);
    }

    // ==========================================================================
    // Cleanup
    // ==========================================================================

    dispose() {
        this.stopPolling();
        this._onStatusChange.dispose();
        this._onStateUpdate.dispose();
        this._onError.dispose();
    }
}
