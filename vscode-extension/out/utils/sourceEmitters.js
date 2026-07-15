"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.quoteVibeString = quoteVibeString;
exports.generateEffectRackCode = generateEffectRackCode;
function quoteVibeString(value) {
    return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r')}"`;
}
function vibeFloat(value) {
    if (!Number.isFinite(value)) {
        throw new Error('VibeLang numeric arguments must be finite');
    }
    return Number.isInteger(value) ? `${value}.0` : String(value);
}
function generateEffectRackCode(synthdefName, groupPath, parameters) {
    const effectId = `${groupPath}:${synthdefName}`;
    const lines = [
        '{',
        `    let inserted_effect = fx(${quoteVibeString(effectId)});`,
        `    inserted_effect.group_path = ${quoteVibeString(groupPath)};`,
        `    inserted_effect.synth(${quoteVibeString(synthdefName)})`,
    ];
    for (const parameter of parameters) {
        lines.push(`        .param(${quoteVibeString(parameter.name)}, ${vibeFloat(parameter.defaultValue)})`);
    }
    lines.push('        .apply();', '}');
    return lines.join('\n');
}
//# sourceMappingURL=sourceEmitters.js.map