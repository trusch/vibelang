/**
 * Automation Types for VibeLang
 *
 * Defines data structures for automation curves in the arrangement timeline.
 * Automation allows parameters to change over time using control points and bezier curves.
 */

/**
 * Curve type determines how values interpolate between control points
 */
export type CurveType = 'linear' | 'exponential' | 'bezier' | 'step' | 'smooth';

/**
 * Target for automation - what parameter is being automated
 */
export interface AutomationTarget {
    /** Type of entity: group, voice, or effect */
    type: 'group' | 'voice' | 'effect';
    /** Name of the entity */
    name: string;
    /** Parameter name to automate (e.g., 'amp', 'freq', 'cutoff') */
    param: string;
}

/**
 * A single control point in an automation curve
 */
export interface AutomationPoint {
    /** Unique ID for this point */
    id: string;
    /** Beat position */
    beat: number;
    /** Value at this point (0.0 to 1.0 normalized) */
    value: number;
    /** Curve type for interpolation TO the next point */
    curveType: CurveType;
    /** Bezier control point 1 (outgoing) - relative offset from this point */
    bezierOut?: { beat: number; value: number };
    /** Bezier control point 2 (incoming to next point) - stored on THIS point */
    bezierIn?: { beat: number; value: number };
    /** Whether this point is selected in the UI */
    selected?: boolean;
}

/**
 * An automation lane containing a curve for a single parameter
 */
export interface AutomationLane {
    /** Unique ID for this lane */
    id: string;
    /** Target being automated */
    target: AutomationTarget;
    /** Control points defining the automation curve */
    points: AutomationPoint[];
    /** Whether this lane is expanded/visible */
    visible: boolean;
    /** Display color for the curve */
    color: string;
    /** Minimum value for the parameter */
    minValue: number;
    /** Maximum value for the parameter */
    maxValue: number;
    /** Whether the lane is armed for recording */
    armed?: boolean;
    /** Display label (auto-generated from target if not provided) */
    label?: string;
}

/**
 * Configuration for automation editing
 */
export interface AutomationConfig {
    /** Grid size for snapping (in beats) */
    gridSnap: number;
    /** Value snap resolution (e.g., 0.01 for 1% increments) */
    valueSnap: number;
    /** Default curve type for new points */
    defaultCurveType: CurveType;
    /** Show value labels on points */
    showValueLabels: boolean;
    /** Show grid lines */
    showGrid: boolean;
}

/**
 * Default configuration
 */
export const DEFAULT_AUTOMATION_CONFIG: AutomationConfig = {
    gridSnap: 0.25, // Snap to 16th notes
    valueSnap: 0,   // No value snapping
    defaultCurveType: 'smooth',
    showValueLabels: true,
    showGrid: true,
};

// =============================================================================
// Utility Functions
// =============================================================================

/**
 * Generate a unique ID for automation points/lanes
 */
export function generateAutomationId(): string {
    return 'auto_' + Date.now().toString(36) + Math.random().toString(36).substr(2, 9);
}

/**
 * Create an empty automation lane for a target
 */
export function createAutomationLane(
    target: AutomationTarget,
    minValue: number = 0,
    maxValue: number = 1
): AutomationLane {
    const colors: Record<string, string> = {
        'group': '#569cd6',
        'voice': '#9bbb59',
        'effect': '#c586c0',
    };

    return {
        id: generateAutomationId(),
        target,
        points: [],
        visible: true,
        color: colors[target.type] || '#858585',
        minValue,
        maxValue,
        label: `${target.name}.${target.param}`,
    };
}

/**
 * Create a control point
 */
export function createAutomationPoint(
    beat: number,
    value: number,
    curveType: CurveType = 'smooth'
): AutomationPoint {
    return {
        id: generateAutomationId(),
        beat,
        value,
        curveType,
    };
}

/**
 * Calculate the value at a given beat using the automation curve
 */
export function getValueAtBeat(lane: AutomationLane, beat: number): number {
    const points = [...lane.points].sort((a, b) => a.beat - b.beat);

    if (points.length === 0) return 0.5; // Default to middle
    if (points.length === 1) return points[0].value;

    // Before first point
    if (beat <= points[0].beat) return points[0].value;

    // After last point
    if (beat >= points[points.length - 1].beat) return points[points.length - 1].value;

    // Find surrounding points
    let p1: AutomationPoint | undefined;
    let p2: AutomationPoint | undefined;

    for (let i = 0; i < points.length - 1; i++) {
        if (beat >= points[i].beat && beat <= points[i + 1].beat) {
            p1 = points[i];
            p2 = points[i + 1];
            break;
        }
    }

    if (!p1 || !p2) return 0.5;

    // Calculate t (0 to 1) between points
    const t = (beat - p1.beat) / (p2.beat - p1.beat);

    // Interpolate based on curve type
    return interpolate(p1.value, p2.value, t, p1.curveType, p1, p2);
}

/**
 * Interpolate between two values based on curve type
 */
function interpolate(
    v1: number,
    v2: number,
    t: number,
    curveType: CurveType,
    p1?: AutomationPoint,
    p2?: AutomationPoint
): number {
    switch (curveType) {
        case 'linear':
            return v1 + (v2 - v1) * t;

        case 'exponential':
            // Exponential curve
            return v1 + (v2 - v1) * (1 - Math.pow(1 - t, 3));

        case 'step':
            // Hold v1 until we reach v2
            return t < 1 ? v1 : v2;

        case 'smooth':
            // Smooth ease in/out (cubic)
            const smoothT = t * t * (3 - 2 * t);
            return v1 + (v2 - v1) * smoothT;

        case 'bezier':
            // Full bezier curve if control points are provided
            if (p1?.bezierOut && p1?.bezierIn) {
                return cubicBezier(
                    v1,
                    v1 + (p1.bezierOut.value || 0),
                    v2 + (p1.bezierIn.value || 0),
                    v2,
                    t
                );
            }
            // Fall back to smooth
            return v1 + (v2 - v1) * (t * t * (3 - 2 * t));

        default:
            return v1 + (v2 - v1) * t;
    }
}

/**
 * Cubic bezier interpolation
 */
function cubicBezier(p0: number, p1: number, p2: number, p3: number, t: number): number {
    const oneMinusT = 1 - t;
    return (
        oneMinusT * oneMinusT * oneMinusT * p0 +
        3 * oneMinusT * oneMinusT * t * p1 +
        3 * oneMinusT * t * t * p2 +
        t * t * t * p3
    );
}

/**
 * Convert normalized value to parameter value
 */
export function normalizedToParamValue(normalized: number, min: number, max: number): number {
    return min + normalized * (max - min);
}

/**
 * Convert parameter value to normalized (0-1)
 */
export function paramValueToNormalized(value: number, min: number, max: number): number {
    if (max === min) return 0.5;
    return (value - min) / (max - min);
}

/**
 * Snap beat to grid
 */
export function snapBeatToGrid(beat: number, gridSize: number): number {
    if (gridSize <= 0) return beat;
    return Math.round(beat / gridSize) * gridSize;
}

/**
 * Generate fade() code from automation lane
 * This creates VibeLang code that can be inserted into source files
 */
export function generateFadeCode(lane: AutomationLane): string {
    const points = [...lane.points].sort((a, b) => a.beat - b.beat);
    if (points.length < 2) return '';

    const fades: string[] = [];
    const target = lane.target;

    for (let i = 0; i < points.length - 1; i++) {
        const p1 = points[i];
        const p2 = points[i + 1];
        const duration = p2.beat - p1.beat;

        // Convert normalized values to actual param values
        const startValue = normalizedToParamValue(p1.value, lane.minValue, lane.maxValue);
        const endValue = normalizedToParamValue(p2.value, lane.minValue, lane.maxValue);

        // Generate the fade call
        // Format: fade("target_type", "target_name", "param", start, end, duration, start_beat);
        let fadeCode: string;

        if (target.type === 'group') {
            fadeCode = `group("${target.name}").fade("${target.param}", ${startValue.toFixed(3)}, ${endValue.toFixed(3)}, ${duration.toFixed(2)});`;
        } else if (target.type === 'voice') {
            fadeCode = `voice("${target.name}").fade("${target.param}", ${startValue.toFixed(3)}, ${endValue.toFixed(3)}, ${duration.toFixed(2)});`;
        } else {
            fadeCode = `effect("${target.name}").fade("${target.param}", ${startValue.toFixed(3)}, ${endValue.toFixed(3)}, ${duration.toFixed(2)});`;
        }

        // Add scheduling comment if not at beat 0
        if (p1.beat > 0) {
            fadeCode = `// At beat ${p1.beat.toFixed(2)}:\n${fadeCode}`;
        }

        fades.push(fadeCode);
    }

    return fades.join('\n\n');
}

/**
 * Parse existing fade() calls from code to create automation points
 * This is used for bi-directional sync
 */
export function parseFadeCode(code: string, target: AutomationTarget): AutomationPoint[] {
    const points: AutomationPoint[] = [];

    // Match fade calls for this target
    // Format variations:
    // - group("name").fade("param", start, end, duration)
    // - voice("name").fade("param", start, end, duration)
    // - effect("name").fade("param", start, end, duration)

    const pattern = new RegExp(
        `${target.type}\\s*\\(\\s*["']${escapeRegex(target.name)}["']\\s*\\)\\s*\\.\\s*fade\\s*\\(\\s*["']${escapeRegex(target.param)}["']\\s*,\\s*([\\d.]+)\\s*,\\s*([\\d.]+)\\s*,\\s*([\\d.]+)\\s*\\)`,
        'g'
    );

    let match;
    let currentBeat = 0;

    while ((match = pattern.exec(code)) !== null) {
        const startValue = parseFloat(match[1]);
        const endValue = parseFloat(match[2]);
        const duration = parseFloat(match[3]);

        // Add start point
        points.push(createAutomationPoint(currentBeat, startValue, 'smooth'));

        // Add end point
        currentBeat += duration;
        points.push(createAutomationPoint(currentBeat, endValue, 'smooth'));
    }

    // Remove duplicate points at same beat
    const uniquePoints: AutomationPoint[] = [];
    for (const point of points) {
        const existing = uniquePoints.find(p => Math.abs(p.beat - point.beat) < 0.001);
        if (!existing) {
            uniquePoints.push(point);
        }
    }

    return uniquePoints;
}

function escapeRegex(str: string): string {
    return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// =============================================================================
// Drawing Helpers
// =============================================================================

/**
 * Generate SVG path data for an automation curve
 */
export function generateCurvePath(
    points: AutomationPoint[],
    width: number,
    height: number,
    maxBeats: number
): string {
    const sorted = [...points].sort((a, b) => a.beat - b.beat);
    if (sorted.length === 0) return '';

    const beatToX = (beat: number) => (beat / maxBeats) * width;
    const valueToY = (value: number) => height - (value * height);

    let path = `M ${beatToX(sorted[0].beat)} ${valueToY(sorted[0].value)}`;

    for (let i = 0; i < sorted.length - 1; i++) {
        const p1 = sorted[i];
        const p2 = sorted[i + 1];

        const x1 = beatToX(p1.beat);
        const y1 = valueToY(p1.value);
        const x2 = beatToX(p2.beat);
        const y2 = valueToY(p2.value);

        switch (p1.curveType) {
            case 'linear':
                path += ` L ${x2} ${y2}`;
                break;

            case 'step':
                path += ` L ${x2} ${y1} L ${x2} ${y2}`;
                break;

            case 'smooth':
                // Smooth curve using cubic bezier with auto-calculated control points
                const cp1x = x1 + (x2 - x1) * 0.5;
                const cp1y = y1;
                const cp2x = x1 + (x2 - x1) * 0.5;
                const cp2y = y2;
                path += ` C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${x2} ${y2}`;
                break;

            case 'exponential':
                // Exponential curve using quadratic bezier
                const cpx = x1 + (x2 - x1) * 0.7;
                const cpy = y2;
                path += ` Q ${cpx} ${cpy}, ${x2} ${y2}`;
                break;

            case 'bezier':
                // Full bezier with handles
                if (p1.bezierOut && p1.bezierIn) {
                    const bcp1x = x1 + beatToX(p1.bezierOut.beat);
                    const bcp1y = y1 - p1.bezierOut.value * height;
                    const bcp2x = x2 + beatToX(p1.bezierIn.beat);
                    const bcp2y = y2 - p1.bezierIn.value * height;
                    path += ` C ${bcp1x} ${bcp1y}, ${bcp2x} ${bcp2y}, ${x2} ${y2}`;
                } else {
                    path += ` L ${x2} ${y2}`;
                }
                break;

            default:
                path += ` L ${x2} ${y2}`;
        }
    }

    return path;
}

/**
 * Generate filled area path (for showing automation as filled region)
 */
export function generateFilledPath(
    points: AutomationPoint[],
    width: number,
    height: number,
    maxBeats: number
): string {
    const curvePath = generateCurvePath(points, width, height, maxBeats);
    if (!curvePath) return '';

    const sorted = [...points].sort((a, b) => a.beat - b.beat);
    const beatToX = (beat: number) => (beat / maxBeats) * width;

    const firstX = beatToX(sorted[0].beat);
    const lastX = beatToX(sorted[sorted.length - 1].beat);

    // Close the path along the bottom
    return `${curvePath} L ${lastX} ${height} L ${firstX} ${height} Z`;
}
