// Tips, facts, and subtle jokes for VibeLang Playground
// Categories: tips (60%), facts (20%), jokes (15%), shortcuts (5%)

export const tips = [
  // ============================================================
  // PRO TIPS (60%)
  // ============================================================
  {
    type: 'tip',
    text: 'Use db(-6) instead of 0.5 for volume. Decibels are more intuitive for mixing!',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Euclidean rhythms with .euclid(5, 8) create complex patterns that feel natural.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'The | character in patterns is just visual. Use it to mark bar boundaries.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use .poly(4) on voices to enable chords. Each note needs its own voice slot.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'X for accents, x for normal hits, o for ghost notes. Dynamics make beats groove!',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Layer multiple synthdefs for thicker sounds. A sub bass + a gritty bass = magic.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use .swing(0.5) on hi-hats but not kicks. This creates that classic hip-hop feel.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Filter sweeps: use fade().on_effect("filter").param("cutoff").from(200).to(2000).over_bars(8).start()',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use .scale("minor").root("E") on melodies to play in key. The notes auto-quantize to the scale.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Add effects inside groups: fx("verb").synth("reverb").param("room", 0.8).apply()',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use .gate(0.8) on melodies to control note length. 1.0 = legato, 0.3 = staccato.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Detune oscillators slightly in custom synths: saw_ar(freq * 1.005) adds thickness.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Kick and bass fighting? Lower the bass gain or use different octaves to separate them.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'The stdlib has 600+ synths. Use the explorer to audition sounds before coding.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use .over_bars(4) on fades for musical timing. bars() converts bars to beats for sequences.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Chain melody methods: .notes("C4 E4 G4").gate(0.5).transpose(12).start()',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Tie notes with -: "C4 - - ." plays C4 for 3/16th notes then rests.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'For builds, automate multiple parameters. Filter + reverb + gain = epic drops!',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use sequence() to arrange patterns over time. Think of it like a DAW timeline.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Pink noise is better for drums than white noise. It has more low-end energy.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Always leave headroom! Keep your master around -6dB to prevent clipping.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use .euclid(7, 16, 2) - the third parameter rotates the pattern for variations.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Bracket notes for chords: "[C4 E4 G4]" plays C major all at once.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Half-time feel: slow the tempo but double the pattern density.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use breakdowns to reset energy. Strip elements before the drop for contrast.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'LFOs make static sounds alive! let lfo = voice("lfo").synth("lfo_tri"); v.param("cutoff").modulate_by(lfo, "out")',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Slow LFO rates (0.1-0.5 Hz) create evolving pads. Fast rates (4-8 Hz) create wobbles.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Different LFO shapes have different feels: sine = smooth, saw = ramp, tri = linear, square = choppy',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Modulate amplitude for tremolo, pitch for vibrato, filter for wah-wah effects.',
    icon: 'bulb'
  },
  {
    type: 'tip',
    text: 'Use lfo_random for organic, unpredictable modulation that sounds less robotic.',
    icon: 'bulb'
  },

  // ============================================================
  // FACTS (20%)
  // ============================================================
  {
    type: 'fact',
    text: 'VibeLang compiles to SuperCollider synthdefs running on scsynth in your browser!',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'The stdlib has 629 synthdefs across 22 categories. That\'s a lot of sounds!',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'Euclidean rhythms are found in traditional music worldwide, from Cuba to Africa.',
    icon: 'info'
  },
  {
    type: 'fact',
    text: '120 BPM is the average human heartbeat during light exercise. That\'s why it feels natural!',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'The 808 kick drum was originally considered a failure. Now it defines hip-hop!',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'VibeLang uses WebAssembly to run SuperCollider\'s audio engine in the browser.',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'The pentatonic scale has no semitones. That\'s why every note sounds "right"!',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'Swing originated from jazz musicians playing slightly behind the beat intentionally.',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'A4 = 440Hz is the modern standard, but Bach tuned to around 415Hz.',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'The "four on the floor" kick pattern has been used in disco, house, and techno since the 70s.',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'Sidechain compression was accidentally discovered by radio engineers in the 1930s.',
    icon: 'info'
  },
  {
    type: 'fact',
    text: 'The Moog filter is so iconic it has been patented, expired, and cloned thousands of times.',
    icon: 'info'
  },

  // ============================================================
  // JOKES (15%) - Subtle, professional humor
  // ============================================================
  {
    type: 'joke',
    text: 'Why did the synthesizer break up with the piano? Too many issues with their frequency.',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'My acid bassline went to rehab. It had too much resonance.',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'I asked my reverb for advice but it just kept repeating itself.',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'Why do producers love compression? It really brings everything together.',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'My delay pedal and I have great timing. Eventually.',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'I wrote a song about a tortilla. Actually, it\'s more of a wrap.',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'What do you call a musician without a girlfriend? Homeless. (But they have great reverb!)',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'My neighbors love my music. At least, that\'s what the police said they were interested in.',
    icon: 'star'
  },
  {
    type: 'joke',
    text: 'I\'d tell you a joke about an unfinished song but... (loading...)',
    icon: 'star'
  },

  // ============================================================
  // SHORTCUTS (5%)
  // ============================================================
  {
    type: 'shortcut',
    text: 'Ctrl+Enter runs your code. Ctrl+. stops playback.',
    icon: 'keyboard'
  },
  {
    type: 'shortcut',
    text: 'Ctrl+Space opens autocomplete. Tab inserts suggestions.',
    icon: 'keyboard'
  },
  {
    type: 'shortcut',
    text: 'Tab indents selected lines. Shift+Tab dedents them.',
    icon: 'keyboard'
  },
  {
    type: 'shortcut',
    text: 'Escape closes autocomplete and other popups.',
    icon: 'keyboard'
  }
];

// Get all tips
export function getAllTips() {
  return tips;
}

// Get tips by type
export function getTipsByType(type) {
  return tips.filter(tip => tip.type === type);
}

// Get a random tip with weighted distribution
export function getRandomTip() {
  // Weighted random: tips (60%), facts (20%), jokes (15%), shortcuts (5%)
  const rand = Math.random();
  let typeFilter;

  if (rand < 0.60) {
    typeFilter = 'tip';
  } else if (rand < 0.80) {
    typeFilter = 'fact';
  } else if (rand < 0.95) {
    typeFilter = 'joke';
  } else {
    typeFilter = 'shortcut';
  }

  const filtered = tips.filter(t => t.type === typeFilter);
  return filtered[Math.floor(Math.random() * filtered.length)];
}

// Get a random tip that isn't the current one
export function getNextTip(currentTip) {
  let next;
  let attempts = 0;
  do {
    next = getRandomTip();
    attempts++;
  } while (next?.text === currentTip?.text && attempts < 10);
  return next;
}

export default tips;
