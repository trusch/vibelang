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
const DEFAULT_CONNECTION_TIMEOUT = 2000; // ms
const DEFAULT_MAX_RETRIES = 10;
const DEFAULT_RETRY_DELAY = 300; // ms - initial retry delay
const MAX_RETRY_DELAY = 5000; // ms - cap for exponential backoff
class RuntimeManager {
    constructor(options = {}) {
        this._status = 'disconnected';
        this._pollTimer = null;
        this._lastError = null;
        this._reconnectTimer = null;
        this._isReconnecting = false;
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
        this._connectionTimeout = options.connectionTimeout ?? DEFAULT_CONNECTION_TIMEOUT;
        this._maxRetries = options.maxRetries ?? DEFAULT_MAX_RETRIES;
        this._reconnectOnDisconnect = options.reconnectOnDisconnect ?? true;
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
    /**
     * Attempt to connect to the runtime with retry logic.
     * Uses exponential backoff between retries.
     * @param maxRetries Override the default max retries (use 0 for single attempt)
     */
    async tryConnect(maxRetries) {
        const retries = maxRetries ?? this._maxRetries;
        this.setStatus('connecting');
        this._isReconnecting = false;
        let delay = DEFAULT_RETRY_DELAY;
        for (let attempt = 0; attempt <= retries; attempt++) {
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
            // Don't delay after the last attempt
            if (attempt < retries) {
                await this.sleep(delay);
                // Exponential backoff with cap
                delay = Math.min(delay * 1.5, MAX_RETRY_DELAY);
            }
        }
        this.setStatus('disconnected');
        return false;
    }
    sleep(ms) {
        return new Promise(resolve => setTimeout(resolve, ms));
    }
    disconnect() {
        this.stopPolling();
        this.cancelReconnect();
        this._isReconnecting = false;
        this.setStatus('disconnected');
        this._state = null;
    }
    cancelReconnect() {
        if (this._reconnectTimer) {
            clearTimeout(this._reconnectTimer);
            this._reconnectTimer = null;
        }
    }
    scheduleReconnect() {
        if (!this._reconnectOnDisconnect || this._isReconnecting) {
            return;
        }
        this._isReconnecting = true;
        this.cancelReconnect();
        // Wait a bit before attempting to reconnect
        this._reconnectTimer = setTimeout(async () => {
            if (this._status !== 'connected' && this._isReconnecting) {
                await this.tryConnect();
            }
            this._isReconnecting = false;
        }, DEFAULT_RETRY_DELAY);
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
            this.stopPolling();
            this.setStatus('error');
            this._onError.fire(this._lastError);
            // Trigger automatic reconnection
            this.scheduleReconnect();
        }
    }
    // ==========================================================================
    // HTTP Helpers
    // ==========================================================================
    async fetch(path, options = {}) {
        const url = `${this.baseUrl}${path}`;
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), this._connectionTimeout);
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
            // Check if response has content before parsing JSON
            const contentLength = response.headers.get('content-length');
            const contentType = response.headers.get('content-type');
            // If no content-length or it's 0, or no JSON content-type, return null
            if (contentLength === '0' || contentLength === null) {
                // Try to read body - if empty, return null
                const text = await response.text();
                if (!text || text.trim() === '') {
                    return null;
                }
                // If there is text, try to parse it as JSON
                try {
                    return JSON.parse(text);
                }
                catch {
                    return null;
                }
            }
            // If content-type isn't JSON, return null
            if (contentType && !contentType.includes('application/json')) {
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
        console.log(`[RuntimeManager] PATCH /patterns/${name}:`, JSON.stringify(update));
        const result = await this.patch(`/patterns/${encodeURIComponent(name)}`, update);
        console.log(`[RuntimeManager] Response:`, result ? JSON.stringify(result) : 'null');
        return result;
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
        this.cancelReconnect();
        this._onStatusChange.dispose();
        this._onStateUpdate.dispose();
        this._onError.dispose();
    }
}
exports.RuntimeManager = RuntimeManager;
//# sourceMappingURL=runtimeManager.js.map