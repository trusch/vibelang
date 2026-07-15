import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { test } from 'node:test';
import { AutomationLane, generateFadeCode, parseFadeCode } from './automationTypes';
import {
    generateEffectRackCode,
    vibe,
    WEBVIEW_VIBE_EMITTER_RUNTIME,
} from './sourceEmitters';

test('arrangement automation emits canonical scheduled Fade builders', () => {
    const lane: AutomationLane = {
        id: 'lane',
        target: { type: 'group', name: 'mix"bus', param: 'amp' },
        points: [
            { id: 'a', beat: 0, value: 0.25, curveType: 'linear' },
            { id: 'b', beat: 4, value: 0.75, curveType: 'smooth' },
        ],
        visible: true,
        color: '#fff',
        minValue: 0,
        maxValue: 2,
    };

    const code = generateFadeCode(lane);
    assert.match(code, /^sequence\("automation:group:mix\\"bus:amp"\)/);
    assert.match(code, /\.clip\(0\.00\.\.4\.00, fade\(/);
    assert.match(code, /\.on_group\("mix\\"bus"\)/);
    assert.match(code, /\.from\(0\.500\)/);
    assert.match(code, /\.to\(1\.500\)/);
    assert.match(code, /\.over\(4\.00\)/);
    assert.match(code, /\.curve\("linear"\)/);
    assert.match(code, /\.apply\(\)\)\n    \.start\(\);$/);
    assert.doesNotMatch(code, /(?:group|voice|effect)\([^)]*\)\.fade\(/);

    const parsed = parseFadeCode(code, lane.target);
    assert.deepEqual(parsed.map(point => [point.beat, point.value]), [[0, 0.5], [4, 1.5]]);
});

test('effect rack emits an Fx builder targeted through its registered property', () => {
    const code = generateEffectRackCode('room"verb', 'main/drums', [
        { name: 'mix', defaultValue: 1 },
        { name: 'size', defaultValue: 0.75 },
    ]);

    assert.equal(code, `{
    let inserted_effect = fx("main/drums:room\\"verb");
    inserted_effect.group_path = "main/drums";
    inserted_effect.synth("room\\"verb")
        .param("mix", 1.0)
        .param("size", 0.75)
        .apply();
}`);
    assert.doesNotMatch(code, new RegExp(['add', 'effect'].join('_')));
});

test('effect rack rejects non-finite numeric literals', () => {
    assert.throws(
        () => generateEffectRackCode('verb', 'main', [{ name: 'mix', defaultValue: Infinity }]),
        /must be finite/,
    );
});

test('operator emission folds scalar arithmetic and preserves strict NodeRef f64 literals', () => {
    assert.equal(vibe.multiply(vibe.f64(4), vibe.f64(1)).source, '4.0');
    assert.equal(
        vibe.multiply(vibe.expr('NodeRef', 'sin_osc_kr(4.0)'), vibe.f64(1)).source,
        '(sin_osc_kr(4.0) * 1.0)'
    );
});

test('webview emitter runtime is executable and matches strict literal/operator output', () => {
    const webviewVibe = Function(`${WEBVIEW_VIBE_EMITTER_RUNTIME}\nreturn vibe;`)();
    assert.equal(
        webviewVibe.member('SampleHandle', 'semitones', [webviewVibe.f64(-5)]),
        '.semitones(-5.0)'
    );
    assert.equal(
        webviewVibe.multiply(
            webviewVibe.expr('NodeRef', 'sin_osc_kr(4.0)'),
            webviewVibe.f64(1)
        ).source,
        '(sin_osc_kr(4.0) * 1.0)'
    );
});

test('host and webview UGen naming resolve every packaged callable rate to the manifest', () => {
    const webviewVibe = Function(`${WEBVIEW_VIBE_EMITTER_RUNTIME}\nreturn vibe;`)();
    const manifest = JSON.parse(fs.readFileSync(
        path.resolve(__dirname, '../../../api/public-api-manifest-v1.json'),
        'utf8'
    )) as {
        entries: Array<{
            registered_name: string;
            receiver: string | null;
            details: { type: string; callable?: boolean };
            overloads: Array<{ parameters: Array<{ accepted_types: string[] }> }>;
        }>;
    };
    const byName = new Map(manifest.entries.map(entry => [entry.registered_name, entry]));
    const directory = path.resolve(__dirname, '../../ugen_manifests');
    const ugens = fs.readdirSync(directory)
        .filter(file => file.endsWith('.json'))
        .flatMap(file => JSON.parse(fs.readFileSync(path.join(directory, file), 'utf8')) as Array<{
            name: string;
            rates: string[];
            inputs: unknown[];
        }>);
    let checked = 0;
    for (const ugen of ugens) {
        if (ugen.rates.some(rate => rate === 'demand' || rate === 'builder')) continue;
        for (const rate of ugen.rates.filter(rate => rate === 'ar' || rate === 'kr' || rate === 'ir')) {
            const name = vibe.ugenName(ugen.name, rate);
            assert.equal(webviewVibe.ugenName(ugen.name, rate), name);
            const entry = byName.get(name);
            assert.ok(entry, `${ugen.name} ${rate} resolved to missing ${name}`);
            assert.equal(entry.receiver, null);
            assert.equal(entry.details.type, 'ugen');
            assert.equal(entry.details.callable, true);
            assert.ok(entry.overloads.some(overload =>
                overload.parameters.length === ugen.inputs.length
                && overload.parameters.every(parameter => parameter.accepted_types.includes('Dynamic'))
            ), `${name} lacks the exact ${ugen.inputs.length}-argument Dynamic overload`);
            checked++;
        }
    }
    assert.equal(checked, 558);
});
