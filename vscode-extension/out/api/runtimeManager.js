"use strict";
/**
 * VibeLang Runtime Manager
 *
 * REST API client for communicating with a running VibeLang instance.
 * Handles auto-discovery, connection management, and all API calls.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.RuntimeManager = void 0;
const vscode = require("vscode");
const DEFAULT_PORT = 1606;
const DEFAULT_HOST = 'localhost';
const POLL_INTERVAL = 500; // ms between state polls
const CONNECTION_TIMEOUT = 2000; // ms
class RuntimeManager {
    constructor(options = {}) {
        this._status = 'disconnected';
        this._pollTimer = null;
        this._lastError = null;
        // Event emitters
        this._onStatusChange = new vscode.EventEmitter();
        this._onStateUpdate = new vscode.EventEmitter();
        this._onError = new vscode.EventEmitter();
        this.onStatusChange = this._onStatusChange.event;
        this.onStateUpdate = this._onStateUpdate.event;
        this.onError = this._onError.event;
        // Cached state
        this._state = null;
        this._host = options.host ?? DEFAULT_HOST;
        this._port = options.port ?? DEFAULT_PORT;
        if (options.autoConnect !== false) {
            this.tryConnect();
        }
    }
    // ==========================================================================
    // Connection Management
    // ==========================================================================
    get status() {
        return this._status;
    }
    get baseUrl() {
        return `http://${this._host}:${this._port}`;
    }
    get state() {
        return this._state;
    }
    get lastError() {
        return this._lastError;
    }
    setStatus(status) {
        if (this._status !== status) {
            this._status = status;
            this._onStatusChange.fire(status);
        }
    }
    async tryConnect() {
        this.setStatus('connecting');
        try {
            const transport = await this.getTransport();
            if (transport) {
                this.setStatus('connected');
                this.startPolling();
                return true;
            }
        }
        catch (e) {
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
    setConnection(host, port) {
        this.disconnect();
        this._host = host;
        this._port = port;
        this.tryConnect();
    }
    startPolling() {
        this.stopPolling();
        this._pollTimer = setInterval(() => this.pollState(), POLL_INTERVAL);
        // Immediate first poll
        this.pollState();
    }
    stopPolling() {
        if (this._pollTimer) {
            clearInterval(this._pollTimer);
            this._pollTimer = null;
        }
    }
    async pollState() {
        try {
            const state = await this.fetchFullState();
            if (state) {
                this._state = state;
                this._onStateUpdate.fire(state);
            }
        }
        catch (e) {
            this._lastError = e instanceof Error ? e.message : String(e);
            this.setStatus('error');
            this._onError.fire(this._lastError);
        }
    }
    // ==========================================================================
    // HTTP Helpers
    // ==========================================================================
    async fetch(path, options = {}) {
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
            return await response.json();
        }
        catch (e) {
            clearTimeout(timeoutId);
            if (e instanceof Error && e.name === 'AbortError') {
                throw new Error('Connection timeout');
            }
            throw e;
        }
    }
    async get(path) {
        return this.fetch(path);
    }
    async post(path, body) {
        return this.fetch(path, {
            method: 'POST',
            body: body ? JSON.stringify(body) : undefined,
        });
    }
    async patch(path, body) {
        return this.fetch(path, {
            method: 'PATCH',
            body: JSON.stringify(body),
        });
    }
    async put(path, body) {
        return this.fetch(path, {
            method: 'PUT',
            body: JSON.stringify(body),
        });
    }
    async delete(path) {
        try {
            await this.fetch(path, { method: 'DELETE' });
            return true;
        }
        catch {
            return false;
        }
    }
    // ==========================================================================
    // Full State Fetch
    // ==========================================================================
    async fetchFullState() {
        // Fetch all entities in parallel for efficiency
        const [transport, groups, voices, patterns, melodies, sequences, effects, samples, synthdefs, live,] = await Promise.all([
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
    async getTransport() {
        return this.get('/transport');
    }
    async updateTransport(update) {
        return this.patch('/transport', update);
    }
    async startTransport() {
        return this.post('/transport/start');
    }
    async stopTransport() {
        return this.post('/transport/stop');
    }
    async seekTransport(beat) {
        return this.post('/transport/seek', { beat });
    }
    // ==========================================================================
    // Groups API
    // ==========================================================================
    async getGroups() {
        return this.get('/groups');
    }
    async getGroup(path) {
        return this.get(`/groups/${encodeURIComponent(path)}`);
    }
    async updateGroup(path, update) {
        return this.patch(`/groups/${encodeURIComponent(path)}`, update);
    }
    async muteGroup(path) {
        return this.post(`/groups/${encodeURIComponent(path)}/mute`);
    }
    async unmuteGroup(path) {
        return this.post(`/groups/${encodeURIComponent(path)}/unmute`);
    }
    async soloGroup(path) {
        return this.post(`/groups/${encodeURIComponent(path)}/solo`);
    }
    async unsoloGroup(path) {
        return this.post(`/groups/${encodeURIComponent(path)}/unsolo`);
    }
    async setGroupParam(path, param, value, fadeBeats) {
        await this.put(`/groups/${encodeURIComponent(path)}/params/${param}`, {
            value,
            fade_beats: fadeBeats,
        });
    }
    // ==========================================================================
    // Voices API
    // ==========================================================================
    async getVoices() {
        return this.get('/voices');
    }
    async getVoice(name) {
        return this.get(`/voices/${encodeURIComponent(name)}`);
    }
    async updateVoice(name, update) {
        return this.patch(`/voices/${encodeURIComponent(name)}`, update);
    }
    async triggerVoice(name, params) {
        await this.post(`/voices/${encodeURIComponent(name)}/trigger`, { params });
    }
    async stopVoice(name) {
        await this.post(`/voices/${encodeURIComponent(name)}/stop`);
    }
    async noteOn(name, note, velocity = 100) {
        await this.post(`/voices/${encodeURIComponent(name)}/note-on`, { note, velocity });
    }
    async noteOff(name, note) {
        await this.post(`/voices/${encodeURIComponent(name)}/note-off`, { note });
    }
    async muteVoice(name) {
        await this.post(`/voices/${encodeURIComponent(name)}/mute`);
    }
    async unmuteVoice(name) {
        await this.post(`/voices/${encodeURIComponent(name)}/unmute`);
    }
    async setVoiceParam(name, param, value, fadeBeats) {
        await this.put(`/voices/${encodeURIComponent(name)}/params/${param}`, {
            value,
            fade_beats: fadeBeats,
        });
    }
    // ==========================================================================
    // Patterns API
    // ==========================================================================
    async getPatterns() {
        return this.get('/patterns');
    }
    async getPattern(name) {
        return this.get(`/patterns/${encodeURIComponent(name)}`);
    }
    async updatePattern(name, update) {
        return this.patch(`/patterns/${encodeURIComponent(name)}`, update);
    }
    async startPattern(name, quantizeBeats) {
        return this.post(`/patterns/${encodeURIComponent(name)}/start`, {
            quantize_beats: quantizeBeats,
        });
    }
    async stopPattern(name, quantizeBeats) {
        return this.post(`/patterns/${encodeURIComponent(name)}/stop`, {
            quantize_beats: quantizeBeats,
        });
    }
    // ==========================================================================
    // Melodies API
    // ==========================================================================
    async getMelodies() {
        return this.get('/melodies');
    }
    async getMelody(name) {
        return this.get(`/melodies/${encodeURIComponent(name)}`);
    }
    async updateMelody(name, update) {
        return this.patch(`/melodies/${encodeURIComponent(name)}`, update);
    }
    async startMelody(name, quantizeBeats) {
        await this.post(`/melodies/${encodeURIComponent(name)}/start`, {
            quantize_beats: quantizeBeats,
        });
    }
    async stopMelody(name) {
        await this.post(`/melodies/${encodeURIComponent(name)}/stop`);
    }
    // ==========================================================================
    // Sequences API
    // ==========================================================================
    async getSequences() {
        return this.get('/sequences');
    }
    async getSequence(name) {
        return this.get(`/sequences/${encodeURIComponent(name)}`);
    }
    async updateSequence(name, update) {
        return this.patch(`/sequences/${encodeURIComponent(name)}`, update);
    }
    async startSequence(name, playOnce = false) {
        await this.post(`/sequences/${encodeURIComponent(name)}/start`, { play_once: playOnce });
    }
    async stopSequence(name) {
        await this.post(`/sequences/${encodeURIComponent(name)}/stop`);
    }
    async pauseSequence(name) {
        await this.post(`/sequences/${encodeURIComponent(name)}/pause`);
    }
    // ==========================================================================
    // Effects API
    // ==========================================================================
    async getEffects() {
        return this.get('/effects');
    }
    async getEffect(id) {
        return this.get(`/effects/${encodeURIComponent(id)}`);
    }
    async updateEffect(id, update) {
        return this.patch(`/effects/${encodeURIComponent(id)}`, update);
    }
    async removeEffect(id) {
        return this.delete(`/effects/${encodeURIComponent(id)}`);
    }
    async setEffectParam(id, param, value, fadeBeats) {
        await this.put(`/effects/${encodeURIComponent(id)}/params/${param}`, {
            value,
            fade_beats: fadeBeats,
        });
    }
    // ==========================================================================
    // Samples API
    // ==========================================================================
    async getSamples() {
        return this.get('/samples');
    }
    async getSample(id) {
        return this.get(`/samples/${encodeURIComponent(id)}`);
    }
    async loadSample(path, id) {
        return this.post('/samples', { id, path });
    }
    async deleteSample(id) {
        return this.delete(`/samples/${encodeURIComponent(id)}`);
    }
    // ==========================================================================
    // SynthDefs API
    // ==========================================================================
    async getSynthDefs() {
        return this.get('/synthdefs');
    }
    async getSynthDef(name) {
        return this.get(`/synthdefs/${encodeURIComponent(name)}`);
    }
    // ==========================================================================
    // Fades API
    // ==========================================================================
    async getFades() {
        return this.get('/fades');
    }
    // ==========================================================================
    // Live State API
    // ==========================================================================
    async getLiveState() {
        return this.get('/live');
    }
    async getMeters() {
        return this.get('/live/meters');
    }
    // ==========================================================================
    // Eval API
    // ==========================================================================
    async evalCode(code) {
        const result = await this.post('/eval', { code });
        return result ?? { success: false, error: 'No response from server' };
    }
    // ==========================================================================
    // Voice Management
    // ==========================================================================
    async createVoice(voice) {
        return this.post('/voices', voice);
    }
    async deleteVoice(name) {
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
exports.RuntimeManager = RuntimeManager;
//# sourceMappingURL=runtimeManager.js.map