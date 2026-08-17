"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.QUARANTINED_DEMAND_UGENS = void 0;
exports.isRuntimeCallableUGen = isRuntimeCallableUGen;
exports.QUARANTINED_DEMAND_UGENS = [
    'DNoiseRing',
    'Dbrown',
    'Dbrown2',
    'Dbufrd',
    'Dbufwr',
    'Dconst',
    'Ddup',
    'DetaBlockerBuf',
    'Dgauss',
    'Dgeom',
    'Dibrown',
    'Diwhite',
    'Dpoll',
    'Drand',
    'Dreset',
    'Dseq',
    'Dser',
    'Dseries',
    'Dshuf',
    'Dstutter',
    'Dswitch',
    'Dswitch1',
    'Dwhite',
    'Dwrand',
    'Dxrand',
];
function isRuntimeCallableUGen(ugen) {
    return !ugen.rates.some(rate => rate === 'demand' || rate === 'builder')
        && ugen.rates.some(rate => rate === 'ar' || rate === 'kr' || rate === 'ir');
}
//# sourceMappingURL=ugenAvailability.js.map