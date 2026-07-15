import * as assert from 'node:assert/strict';
import { test } from 'node:test';
import { AutomationLane, generateFadeCode, parseFadeCode } from './automationTypes';
import { generateEffectRackCode } from './sourceEmitters';

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
