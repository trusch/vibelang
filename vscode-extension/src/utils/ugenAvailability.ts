export interface RateTaggedUGen {
    name: string;
    rates: string[];
}

export const QUARANTINED_DEMAND_UGENS = [
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
] as const;

export function isRuntimeCallableUGen(ugen: RateTaggedUGen): boolean {
    return !ugen.rates.some(rate => rate === 'demand' || rate === 'builder')
        && ugen.rates.some(rate => rate === 'ar' || rate === 'kr' || rate === 'ir');
}
