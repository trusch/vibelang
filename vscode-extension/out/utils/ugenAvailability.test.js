"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const node_test_1 = require("node:test");
const ugenAvailability_1 = require("./ugenAvailability");
(0, node_test_1.test)('all 25 canonical demand-rate UGens are quarantined from editor surfaces', () => {
    const manifestDirectory = path.resolve(__dirname, '../../../crates/vibelang-dsp/ugen_manifests');
    const ugens = fs.readdirSync(manifestDirectory)
        .filter(file => file.endsWith('.json'))
        .flatMap(file => JSON.parse(fs.readFileSync(path.join(manifestDirectory, file), 'utf8')));
    const demandUGens = ugens
        .filter(ugen => ugen.rates.includes('demand'))
        .sort((left, right) => left.name.localeCompare(right.name));
    const expectedNames = [...ugenAvailability_1.QUARANTINED_DEMAND_UGENS]
        .sort((left, right) => left.localeCompare(right));
    assert.equal(demandUGens.length, 25);
    assert.deepEqual(demandUGens.map(ugen => ugen.name), expectedNames);
    assert.ok(demandUGens.every(ugen => !(0, ugenAvailability_1.isRuntimeCallableUGen)(ugen)));
    assert.equal((0, ugenAvailability_1.isRuntimeCallableUGen)({ name: 'Demand', rates: ['ar', 'kr'] }), true);
});
//# sourceMappingURL=ugenAvailability.test.js.map