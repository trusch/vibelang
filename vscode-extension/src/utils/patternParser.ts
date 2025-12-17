/**
 * Pattern Parser Utility
 *
 * Parses VibeLang pattern strings (e.g., "x..x..x.|x.x.x.x.") into a grid representation
 * and generates pattern strings from grid data.
 *
 * Pattern Format:
 * - `x` = hit (velocity 1.0)
 * - `X`, `o`, `O` = accent (velocity 1.2)
 * - `1-9` = velocity levels (0.1 to 1.0)
 * - `.`, `_`, `0`, `-` = rest/hold
 * - `|` = bar separator (each bar = 4 beats)
 *
 * Grid Representation:
 * - Each row is a voice/lane
 * - Each column is a step
 * - Velocity 0 = rest, >0 = hit
 */

export interface PatternStep {
    velocity: number;  // 0 = rest, 0.1-1.2 = hit with velocity
    accent: boolean;   // true for accented notes
}

export interface PatternGrid {
    steps: PatternStep[];
    stepsPerBar: number;
    numBars: number;
    totalSteps: number;
    beatsPerBar: number;
}

export interface PatternConfig {
    stepsPerBar: number;  // 4, 8, 16, 32, etc.
    numBars: number;      // 1, 2, 4, 8, etc.
    beatsPerBar: number;  // Usually 4 for 4/4 time
}

/**
 * Parse a single character into a PatternStep
 */
function parseStepChar(ch: string): PatternStep {
    switch (ch) {
        case 'x':
            return { velocity: 1.0, accent: false };
        case 'X':
        case 'o':
        case 'O':
            return { velocity: 1.0, accent: true };
        case '1':
        case '2':
        case '3':
        case '4':
        case '5':
        case '6':
        case '7':
        case '8':
        case '9':
            const digit = parseInt(ch, 10);
            return { velocity: 0.1 + (digit / 9) * 0.9, accent: false };
        case '.':
        case '_':
        case '0':
        case '-':
        default:
            return { velocity: 0, accent: false };
    }
}

/**
 * Parse a pattern string into a PatternGrid
 */
export function parsePatternString(pattern: string, config?: Partial<PatternConfig>): PatternGrid {
    const beatsPerBar = config?.beatsPerBar ?? 4;

    // Split by bar separator
    const bars = pattern.split('|').map(bar =>
        bar.split('').filter(ch => !/\s/.test(ch))
    );

    const numBars = bars.length;

    // Determine steps per bar from the first non-empty bar, or use config
    let stepsPerBar = config?.stepsPerBar ?? 0;
    if (stepsPerBar === 0) {
        for (const bar of bars) {
            if (bar.length > 0) {
                stepsPerBar = bar.length;
                break;
            }
        }
    }
    // Default to 16 if still empty
    if (stepsPerBar === 0) {
        stepsPerBar = 16;
    }

    const totalSteps = numBars * stepsPerBar;
    const steps: PatternStep[] = [];

    for (let barIndex = 0; barIndex < numBars; barIndex++) {
        const bar = bars[barIndex] || [];

        // Normalize bar to stepsPerBar
        for (let stepIndex = 0; stepIndex < stepsPerBar; stepIndex++) {
            if (bar.length === stepsPerBar) {
                // Exact match
                steps.push(parseStepChar(bar[stepIndex]));
            } else if (bar.length === 0) {
                // Empty bar = all rests
                steps.push({ velocity: 0, accent: false });
            } else if (bar.length < stepsPerBar) {
                // Fewer steps - map proportionally
                const mappedIndex = Math.floor(stepIndex * bar.length / stepsPerBar);
                steps.push(parseStepChar(bar[mappedIndex]));
            } else {
                // More steps - map proportionally
                const mappedIndex = Math.floor(stepIndex * bar.length / stepsPerBar);
                steps.push(parseStepChar(bar[mappedIndex]));
            }
        }
    }

    return {
        steps,
        stepsPerBar,
        numBars,
        totalSteps,
        beatsPerBar,
    };
}

/**
 * Generate a pattern string from a PatternGrid
 */
export function generatePatternString(grid: PatternGrid): string {
    const bars: string[] = [];

    for (let barIndex = 0; barIndex < grid.numBars; barIndex++) {
        let barStr = '';
        const startStep = barIndex * grid.stepsPerBar;

        for (let stepIndex = 0; stepIndex < grid.stepsPerBar; stepIndex++) {
            const step = grid.steps[startStep + stepIndex];
            if (!step || step.velocity === 0) {
                barStr += '.';
            } else if (step.accent) {
                barStr += 'X';
            } else if (step.velocity >= 0.95) {
                barStr += 'x';
            } else {
                // Map velocity to 1-9
                const digit = Math.round((step.velocity - 0.1) / 0.9 * 9);
                barStr += Math.max(1, Math.min(9, digit)).toString();
            }
        }

        bars.push(barStr);
    }

    return bars.join('|');
}

/**
 * Create an empty pattern grid
 */
export function createEmptyGrid(config: PatternConfig): PatternGrid {
    const totalSteps = config.stepsPerBar * config.numBars;
    const steps: PatternStep[] = [];

    for (let i = 0; i < totalSteps; i++) {
        steps.push({ velocity: 0, accent: false });
    }

    return {
        steps,
        stepsPerBar: config.stepsPerBar,
        numBars: config.numBars,
        totalSteps,
        beatsPerBar: config.beatsPerBar,
    };
}

/**
 * Toggle a step in the grid
 */
export function toggleStep(grid: PatternGrid, stepIndex: number, velocity?: number): PatternGrid {
    const newSteps = [...grid.steps];
    const currentStep = newSteps[stepIndex];

    if (velocity !== undefined) {
        newSteps[stepIndex] = { velocity, accent: false };
    } else {
        // Toggle: if has velocity, clear it; otherwise set to 1.0
        if (currentStep.velocity > 0) {
            newSteps[stepIndex] = { velocity: 0, accent: false };
        } else {
            newSteps[stepIndex] = { velocity: 1.0, accent: false };
        }
    }

    return { ...grid, steps: newSteps };
}

/**
 * Toggle accent on a step
 */
export function toggleAccent(grid: PatternGrid, stepIndex: number): PatternGrid {
    const newSteps = [...grid.steps];
    const currentStep = newSteps[stepIndex];

    if (currentStep.velocity > 0) {
        newSteps[stepIndex] = {
            velocity: currentStep.velocity,
            accent: !currentStep.accent
        };
    }

    return { ...grid, steps: newSteps };
}

/**
 * Set velocity for a step
 */
export function setStepVelocity(grid: PatternGrid, stepIndex: number, velocity: number): PatternGrid {
    const newSteps = [...grid.steps];
    const currentStep = newSteps[stepIndex];

    newSteps[stepIndex] = {
        velocity: Math.max(0, Math.min(1.2, velocity)),
        accent: currentStep.accent
    };

    return { ...grid, steps: newSteps };
}

/**
 * Resize grid to new configuration
 */
export function resizeGrid(grid: PatternGrid, newConfig: PatternConfig): PatternGrid {
    const newTotalSteps = newConfig.stepsPerBar * newConfig.numBars;
    const newSteps: PatternStep[] = [];

    for (let i = 0; i < newTotalSteps; i++) {
        // Map from old grid position
        const oldBarIndex = Math.floor(i / newConfig.stepsPerBar) % grid.numBars;
        const oldStepInBar = Math.floor((i % newConfig.stepsPerBar) * grid.stepsPerBar / newConfig.stepsPerBar);
        const oldIndex = oldBarIndex * grid.stepsPerBar + oldStepInBar;

        if (oldIndex < grid.steps.length) {
            newSteps.push({ ...grid.steps[oldIndex] });
        } else {
            newSteps.push({ velocity: 0, accent: false });
        }
    }

    return {
        steps: newSteps,
        stepsPerBar: newConfig.stepsPerBar,
        numBars: newConfig.numBars,
        totalSteps: newTotalSteps,
        beatsPerBar: newConfig.beatsPerBar,
    };
}

/**
 * Generate a Euclidean rhythm pattern
 */
export function generateEuclidean(hits: number, steps: number): PatternStep[] {
    if (steps === 0) return [];
    if (hits >= steps) {
        return Array(steps).fill(null).map(() => ({ velocity: 1.0, accent: false }));
    }
    if (hits === 0) {
        return Array(steps).fill(null).map(() => ({ velocity: 0, accent: false }));
    }

    // Bresenham-style Euclidean algorithm
    const pattern: boolean[] = [];
    let bucket = 0;

    for (let i = 0; i < steps; i++) {
        bucket += hits;
        if (bucket >= steps) {
            bucket -= steps;
            pattern.push(true);
        } else {
            pattern.push(false);
        }
    }

    return pattern.map(hit => ({
        velocity: hit ? 1.0 : 0,
        accent: false,
    }));
}

/**
 * Apply Euclidean rhythm to a grid
 */
export function applyEuclidean(grid: PatternGrid, hits: number): PatternGrid {
    const euclidean = generateEuclidean(hits, grid.stepsPerBar);
    const newSteps: PatternStep[] = [];

    for (let barIndex = 0; barIndex < grid.numBars; barIndex++) {
        for (let stepIndex = 0; stepIndex < grid.stepsPerBar; stepIndex++) {
            newSteps.push({ ...euclidean[stepIndex] });
        }
    }

    return { ...grid, steps: newSteps };
}

/**
 * Shift pattern left or right
 */
export function shiftPattern(grid: PatternGrid, amount: number): PatternGrid {
    const newSteps: PatternStep[] = [];

    for (let barIndex = 0; barIndex < grid.numBars; barIndex++) {
        const barSteps = grid.steps.slice(
            barIndex * grid.stepsPerBar,
            (barIndex + 1) * grid.stepsPerBar
        );

        // Normalize shift amount
        const shift = ((amount % grid.stepsPerBar) + grid.stepsPerBar) % grid.stepsPerBar;

        // Rotate within bar
        for (let i = 0; i < grid.stepsPerBar; i++) {
            const sourceIndex = (i - shift + grid.stepsPerBar) % grid.stepsPerBar;
            newSteps.push({ ...barSteps[sourceIndex] });
        }
    }

    return { ...grid, steps: newSteps };
}

/**
 * Invert pattern (swap hits and rests)
 */
export function invertPattern(grid: PatternGrid): PatternGrid {
    const newSteps = grid.steps.map(step => ({
        velocity: step.velocity > 0 ? 0 : 1.0,
        accent: false,
    }));

    return { ...grid, steps: newSteps };
}

/**
 * Calculate beat position from step index
 */
export function stepToBeat(grid: PatternGrid, stepIndex: number): number {
    const barIndex = Math.floor(stepIndex / grid.stepsPerBar);
    const stepInBar = stepIndex % grid.stepsPerBar;
    const beatInBar = (stepInBar / grid.stepsPerBar) * grid.beatsPerBar;
    return barIndex * grid.beatsPerBar + beatInBar;
}

/**
 * Calculate step index from beat position
 */
export function beatToStep(grid: PatternGrid, beat: number): number {
    const barIndex = Math.floor(beat / grid.beatsPerBar);
    const beatInBar = beat % grid.beatsPerBar;
    const stepInBar = Math.floor((beatInBar / grid.beatsPerBar) * grid.stepsPerBar);
    return barIndex * grid.stepsPerBar + stepInBar;
}

/**
 * Get grid total duration in beats
 */
export function getGridDurationBeats(grid: PatternGrid): number {
    return grid.numBars * grid.beatsPerBar;
}
