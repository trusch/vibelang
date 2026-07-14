import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { test } from 'node:test';
import {
    isRuntimeCallableUGen,
    QUARANTINED_DEMAND_UGENS,
    RateTaggedUGen,
} from './ugenAvailability';

test('all 25 canonical demand-rate UGens are quarantined from editor surfaces', () => {
    const manifestDirectory = path.resolve(
        __dirname,
        '../../../crates/vibelang-dsp/ugen_manifests'
    );
    const ugens = fs.readdirSync(manifestDirectory)
        .filter(file => file.endsWith('.json'))
        .flatMap(file => JSON.parse(
            fs.readFileSync(path.join(manifestDirectory, file), 'utf8')
        ) as RateTaggedUGen[]);
    const demandUGens = ugens
        .filter(ugen => ugen.rates.includes('demand'))
        .sort((left, right) => left.name.localeCompare(right.name));
    const expectedNames = [...QUARANTINED_DEMAND_UGENS]
        .sort((left, right) => left.localeCompare(right));

    assert.equal(demandUGens.length, 25);
    assert.deepEqual(demandUGens.map(ugen => ugen.name), expectedNames);
    assert.ok(demandUGens.every(ugen => !isRuntimeCallableUGen(ugen)));
    assert.equal(isRuntimeCallableUGen({ name: 'Demand', rates: ['ar', 'kr'] }), true);
});
