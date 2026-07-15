"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.WEBVIEW_VIBE_EMITTER_RUNTIME = exports.vibe = void 0;
exports.quoteVibeString = quoteVibeString;
exports.formatVibeFloat = formatVibeFloat;
exports.generateEffectRackCode = generateEffectRackCode;
function quoteVibeString(value) {
    return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\n/g, '\\n').replace(/\r/g, '\\r')}"`;
}
function formatVibeFloat(value) {
    if (!Number.isFinite(value)) {
        throw new Error('VibeLang numeric arguments must be finite');
    }
    return Number.isInteger(value) ? `${value}.0` : String(value);
}
function renderArguments(arguments_) {
    return arguments_.map(argument => argument.source).join(', ');
}
function binaryOperator(left, operator, right) {
    if (left.type === 'f64' && right.type === 'f64') {
        if (left.value === undefined || right.value === undefined) {
            throw new Error('VibeLang f64 operator operands must retain numeric values');
        }
        const value = operator === '+'
            ? left.value + right.value
            : operator === '-' ? left.value - right.value : left.value * right.value;
        return { source: formatVibeFloat(value), type: 'f64', value };
    }
    return { source: `(${left.source} ${operator} ${right.source})`, type: 'NodeRef' };
}
function ugenFunctionName(name, rate) {
    if (name === 'DC')
        return `dc_${rate}`;
    let snakeName = '';
    for (let index = 0; index < name.length; index++) {
        const character = name[index];
        if (character >= 'A' && character <= 'Z') {
            const previous = name[index - 1];
            const next = name[index + 1];
            const previousLower = previous >= 'a' && previous <= 'z';
            const nextLower = next >= 'a' && next <= 'z';
            if (index > 0 && previous !== '_' && (previousLower || nextLower))
                snakeName += '_';
            snakeName += character.toLowerCase();
        }
        else {
            snakeName += character;
        }
    }
    return `${snakeName}_${rate}`;
}
exports.vibe = {
    string(value) {
        return { source: quoteVibeString(value), type: 'string' };
    },
    f64(value) {
        return { source: formatVibeFloat(value), type: 'f64', value };
    },
    f64Fixed(value, digits) {
        if (!Number.isFinite(value)) {
            throw new Error('VibeLang numeric arguments must be finite');
        }
        return { source: value.toFixed(digits), type: 'f64', value };
    },
    bool(value) {
        return { source: String(value), type: 'bool' };
    },
    expr(type, source) {
        return { source, type };
    },
    fn(source) {
        return { source, type: 'Fn' };
    },
    rangeF64(start, end) {
        return { source: `${formatVibeFloat(start)}..${formatVibeFloat(end)}`, type: 'Range<f64>' };
    },
    rangeF64Fixed(start, end, digits) {
        if (!Number.isFinite(start) || !Number.isFinite(end)) {
            throw new Error('VibeLang numeric arguments must be finite');
        }
        return { source: `${start.toFixed(digits)}..${end.toFixed(digits)}`, type: 'Range<f64>' };
    },
    free(name, arguments_) {
        return `${name}(${renderArguments(arguments_)})`;
    },
    member(receiver, name, arguments_) {
        void receiver;
        return `.${name}(${renderArguments(arguments_)})`;
    },
    property(receiver, target, name, argument) {
        void receiver;
        return `${target}.${name} = ${argument.source}`;
    },
    operator(leftType, operator, rightType, left, right) {
        if (left.type !== leftType || right.type !== rightType) {
            throw new Error(`VibeLang operator metadata does not match ${left.type} ${operator} ${right.type}`);
        }
        if (operator !== '+' && operator !== '-' && operator !== '*') {
            throw new Error(`Unsupported VibeLang emitter operator: ${operator}`);
        }
        return binaryOperator(left, operator, right);
    },
    add(left, right) {
        return binaryOperator(left, '+', right);
    },
    subtract(left, right) {
        return binaryOperator(left, '-', right);
    },
    multiply(left, right) {
        return binaryOperator(left, '*', right);
    },
    ugenName(name, rate) {
        return ugenFunctionName(name, rate);
    },
    ugen(name, arguments_) {
        return {
            source: `${name}(${renderArguments(arguments_)})`,
            type: 'NodeRef',
        };
    },
};
exports.WEBVIEW_VIBE_EMITTER_RUNTIME = `
const vibe = (() => {
    const quote = value => JSON.stringify(String(value));
    const float = value => {
        const number = Number(value);
        if (!Number.isFinite(number)) throw new Error('VibeLang numeric arguments must be finite');
        return Number.isInteger(number) ? number.toFixed(1) : String(number);
    };
    const render = arguments_ => arguments_.map(argument => argument.source).join(', ');
    const binary = (left, operator, right) => {
        if (left.type === 'f64' && right.type === 'f64') {
            if (left.value === undefined || right.value === undefined) throw new Error('VibeLang f64 operator operands must retain numeric values');
            const value = operator === '+' ? left.value + right.value : operator === '-' ? left.value - right.value : left.value * right.value;
            return { source: float(value), type: 'f64', value };
        }
        return { source: '(' + left.source + ' ' + operator + ' ' + right.source + ')', type: 'NodeRef' };
    };
    const ugenName = (name, rate) => {
        if (name === 'DC') return 'dc_' + rate;
        let snakeName = '';
        for (let index = 0; index < name.length; index++) {
            const character = name[index];
            if (character >= 'A' && character <= 'Z') {
                const previous = name[index - 1];
                const next = name[index + 1];
                const previousLower = previous >= 'a' && previous <= 'z';
                const nextLower = next >= 'a' && next <= 'z';
                if (index > 0 && previous !== '_' && (previousLower || nextLower)) snakeName += '_';
                snakeName += character.toLowerCase();
            } else {
                snakeName += character;
            }
        }
        return snakeName + '_' + rate;
    };
    return {
        string: value => ({ source: quote(value), type: 'string' }),
        f64: value => ({ source: float(value), type: 'f64', value: Number(value) }),
        f64Fixed: (value, digits) => {
            const number = Number(value);
            if (!Number.isFinite(number)) throw new Error('VibeLang numeric arguments must be finite');
            return { source: number.toFixed(digits), type: 'f64', value: number };
        },
        bool: value => ({ source: String(value), type: 'bool' }),
        expr: (type, source) => ({ source: String(source), type }),
        fn: source => ({ source, type: 'Fn' }),
        rangeF64: (start, end) => ({ source: float(start) + '..' + float(end), type: 'Range<f64>' }),
        rangeF64Fixed: (start, end, digits) => {
            const first = Number(start);
            const last = Number(end);
            if (!Number.isFinite(first) || !Number.isFinite(last)) throw new Error('VibeLang numeric arguments must be finite');
            return { source: first.toFixed(digits) + '..' + last.toFixed(digits), type: 'Range<f64>' };
        },
        free: (name, arguments_) => name + '(' + render(arguments_) + ')',
        member: (receiver, name, arguments_) => { void receiver; return '.' + name + '(' + render(arguments_) + ')'; },
        property: (receiver, target, name, argument) => { void receiver; return target + '.' + name + ' = ' + argument.source; },
        operator: (leftType, operator, rightType, left, right) => {
            if (left.type !== leftType || right.type !== rightType) throw new Error('VibeLang operator metadata mismatch');
            if (operator !== '+' && operator !== '-' && operator !== '*') throw new Error('Unsupported VibeLang emitter operator: ' + operator);
            return binary(left, operator, right);
        },
        add: (left, right) => binary(left, '+', right),
        subtract: (left, right) => binary(left, '-', right),
        multiply: (left, right) => binary(left, '*', right),
        ugenName,
        ugen: (name, arguments_) => ({ source: name + '(' + render(arguments_) + ')', type: 'NodeRef' }),
    };
})();
`;
function generateEffectRackCode(synthdefName, groupPath, parameters) {
    const effectId = `${groupPath}:${synthdefName}`;
    const lines = [
        '{',
        `    let inserted_effect = ${exports.vibe.free('fx', [exports.vibe.string(effectId)])};`,
        `    ${exports.vibe.property('Fx', 'inserted_effect', 'group_path', exports.vibe.string(groupPath))};`,
        `    inserted_effect${exports.vibe.member('Fx', 'synth', [exports.vibe.string(synthdefName)])}`,
    ];
    for (const parameter of parameters) {
        lines.push(`        ${exports.vibe.member('Fx', 'param', [exports.vibe.string(parameter.name), exports.vibe.f64(parameter.defaultValue)])}`);
    }
    lines.push(`        ${exports.vibe.member('Fx', 'apply', [])};`, '}');
    return lines.join('\n');
}
//# sourceMappingURL=sourceEmitters.js.map