/**
 * Waveform Provider
 *
 * Reads audio files and extracts waveform peak data for visualization.
 * Uses wav-decoder for WAV files and falls back to raw buffer analysis.
 */

import * as fs from 'fs';
import * as path from 'path';

export interface WaveformData {
    peaks: number[];       // Normalized -1 to 1
    duration: number;      // Seconds
    sampleRate: number;
    numFrames: number;
    numChannels: number;
}

export interface WaveformOptions {
    targetPoints?: number;  // Default: 1000
    channel?: 'left' | 'right' | 'mix'; // Default: 'mix'
}

/**
 * Get waveform data from an audio file
 */
export async function getWaveformData(
    filePath: string,
    options: WaveformOptions = {}
): Promise<WaveformData> {
    const { targetPoints = 1000, channel = 'mix' } = options;

    // Check file exists
    if (!fs.existsSync(filePath)) {
        throw new Error(`Audio file not found: ${filePath}`);
    }

    const ext = path.extname(filePath).toLowerCase();

    // Currently only WAV files are fully supported
    if (ext === '.wav') {
        return await parseWavFile(filePath, targetPoints, channel);
    }

    // For other formats, return empty waveform with error
    throw new Error(`Unsupported audio format: ${ext}. Currently only WAV files are supported.`);
}

/**
 * Parse a WAV file and extract waveform peaks
 */
async function parseWavFile(
    filePath: string,
    targetPoints: number,
    channel: 'left' | 'right' | 'mix'
): Promise<WaveformData> {
    const buffer = fs.readFileSync(filePath);

    // Parse WAV header
    const header = parseWavHeader(buffer);

    // Extract PCM samples
    const samples = extractPcmSamples(buffer, header);

    // Downsampling to target points
    const peaks = downsampleToPeaks(samples, targetPoints, header.numChannels, channel);

    return {
        peaks,
        duration: header.numFrames / header.sampleRate,
        sampleRate: header.sampleRate,
        numFrames: header.numFrames,
        numChannels: header.numChannels,
    };
}

interface WavHeader {
    sampleRate: number;
    numChannels: number;
    bitsPerSample: number;
    numFrames: number;
    dataOffset: number;
    dataSize: number;
}

/**
 * Parse WAV file header
 */
function parseWavHeader(buffer: Buffer): WavHeader {
    // Check RIFF header
    const riff = buffer.toString('ascii', 0, 4);
    if (riff !== 'RIFF') {
        throw new Error('Invalid WAV file: missing RIFF header');
    }

    // Check WAVE format
    const wave = buffer.toString('ascii', 8, 12);
    if (wave !== 'WAVE') {
        throw new Error('Invalid WAV file: missing WAVE format');
    }

    // Find fmt and data chunks
    let offset = 12;
    let sampleRate = 0;
    let numChannels = 0;
    let bitsPerSample = 0;
    let dataOffset = 0;
    let dataSize = 0;

    while (offset < buffer.length - 8) {
        const chunkId = buffer.toString('ascii', offset, offset + 4);
        const chunkSize = buffer.readUInt32LE(offset + 4);

        if (chunkId === 'fmt ') {
            const audioFormat = buffer.readUInt16LE(offset + 8);
            if (audioFormat !== 1 && audioFormat !== 3) {
                throw new Error(`Unsupported WAV format: ${audioFormat}. Only PCM (1) and IEEE float (3) are supported.`);
            }
            numChannels = buffer.readUInt16LE(offset + 10);
            sampleRate = buffer.readUInt32LE(offset + 12);
            bitsPerSample = buffer.readUInt16LE(offset + 22);
        } else if (chunkId === 'data') {
            dataOffset = offset + 8;
            dataSize = chunkSize;
        }

        offset += 8 + chunkSize;
        // Ensure word alignment
        if (chunkSize % 2 !== 0) {
            offset += 1;
        }
    }

    if (sampleRate === 0 || dataOffset === 0) {
        throw new Error('Invalid WAV file: missing fmt or data chunk');
    }

    const bytesPerSample = bitsPerSample / 8;
    const bytesPerFrame = bytesPerSample * numChannels;
    const numFrames = Math.floor(dataSize / bytesPerFrame);

    return {
        sampleRate,
        numChannels,
        bitsPerSample,
        numFrames,
        dataOffset,
        dataSize,
    };
}

/**
 * Extract PCM samples from WAV buffer
 * Returns array of frames, each frame is an array of channel samples
 */
function extractPcmSamples(buffer: Buffer, header: WavHeader): number[][] {
    const { dataOffset, numFrames, numChannels, bitsPerSample } = header;
    const bytesPerSample = bitsPerSample / 8;
    const samples: number[][] = [];

    for (let i = 0; i < numFrames; i++) {
        const frame: number[] = [];
        for (let ch = 0; ch < numChannels; ch++) {
            const sampleOffset = dataOffset + (i * numChannels + ch) * bytesPerSample;

            let value: number;
            if (bitsPerSample === 8) {
                // 8-bit: unsigned
                value = (buffer.readUInt8(sampleOffset) - 128) / 128;
            } else if (bitsPerSample === 16) {
                // 16-bit: signed
                value = buffer.readInt16LE(sampleOffset) / 32768;
            } else if (bitsPerSample === 24) {
                // 24-bit: signed (read as 3 bytes)
                const b0 = buffer.readUInt8(sampleOffset);
                const b1 = buffer.readUInt8(sampleOffset + 1);
                const b2 = buffer.readInt8(sampleOffset + 2);
                const int24 = b0 | (b1 << 8) | (b2 << 16);
                value = int24 / 8388608;
            } else if (bitsPerSample === 32) {
                // 32-bit: could be int or float, assume float for simplicity
                value = buffer.readFloatLE(sampleOffset);
            } else {
                throw new Error(`Unsupported bit depth: ${bitsPerSample}`);
            }

            frame.push(value);
        }
        samples.push(frame);
    }

    return samples;
}

/**
 * Downsample audio frames to peak data
 * Returns an array of min/max pairs for visualization
 */
function downsampleToPeaks(
    samples: number[][],
    targetPoints: number,
    numChannels: number,
    channel: 'left' | 'right' | 'mix'
): number[] {
    const numFrames = samples.length;
    if (numFrames === 0) {
        return [];
    }

    const framesPerPoint = numFrames / targetPoints;
    const peaks: number[] = [];

    for (let i = 0; i < targetPoints; i++) {
        const startFrame = Math.floor(i * framesPerPoint);
        const endFrame = Math.min(Math.floor((i + 1) * framesPerPoint), numFrames);

        let min = 1;
        let max = -1;

        for (let f = startFrame; f < endFrame; f++) {
            const frame = samples[f];
            let value: number;

            if (channel === 'left' || numChannels === 1) {
                value = frame[0];
            } else if (channel === 'right' && numChannels >= 2) {
                value = frame[1];
            } else {
                // Mix all channels
                value = frame.reduce((a, b) => a + b, 0) / numChannels;
            }

            if (value < min) min = value;
            if (value > max) max = value;
        }

        // Store as min/max pair (interleaved)
        peaks.push(min, max);
    }

    return peaks;
}
