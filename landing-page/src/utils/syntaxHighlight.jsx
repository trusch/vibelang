import React from 'react';

// Complete lists of VibeLang keywords and functions for syntax highlighting

// Built-in functions (from rhai-api.json)
const BUILTIN_FUNCTIONS = new Set([
  // Global/Transport
  'set_tempo', 'get_tempo', 'set_time_signature', 'set_quantization',
  'get_current_beat', 'get_current_bar', 'jump_to_start', 'nudge_transport',
  // Definition
  'define_synthdef', 'define_fx', 'define_group', 'define_macro', 'define_send',
  // Builders
  'voice', 'pattern', 'melody', 'sequence', 'fade', 'fx', 'automation',
  'scene', 'scene_morph', 'melody_gen', 'envelope',
  // Loading
  'sample', 'load_sfz', 'load_vst_instrument', 'load_vst_effect',
  'load_synthdef_bytes',
  // Getters
  'group', 'get_voice', 'get_pattern', 'get_melody', 'get_effect',
  'all_group_names', 'all_voice_names', 'all_pattern_names', 'all_melody_names',
  'all_effect_names', 'active_synth_count',
  // Recording
  'record', 'stop_recording', 'cancel_recording',
  // MIDI
  'midi_device', 'midi_map',
  // Utility
  'db', 'note', 'bars', 'sleep', 'sleep_secs', 'exit', 'exit_with_code',
  'trigger_macro', 'detect_bpm', 'set_group_gain', 'fade_group_gain', 'fade_param',
  // DSP Helpers
  'mix', 'sum', 'dup', 'channels', 'channel', 'sound_in', 'sound_in_channel',
  'detune_spread', 'env_perc', 'env_adsr', 'env_asr', 'env_triangle', 'Env',
  'db_to_amp', 'amp_to_db',
  // Signal Math
  'tanh', 'abs', 'sign', 'squared', 'cubed', 'sqrt', 'exp', 'ln',
  'distort', 'softclip', 'floor', 'ceil', 'round', 'clip', 'wrap', 'fold',
  'min', 'max', 'pow', 'modulo', 'round_to', 'lerp', 'zip'
]);

// UGen functions (audio rate and control rate)
const UGEN_FUNCTIONS = new Set([
  // Oscillators
  'sin_osc_ar', 'sin_osc_kr', 'saw_ar', 'saw_kr', 'pulse_ar', 'pulse_kr',
  'lf_saw_ar', 'lf_saw_kr', 'lf_pulse_ar', 'lf_pulse_kr', 'lf_tri_ar', 'lf_tri_kr',
  'var_saw_ar', 'blip_ar', 'sync_saw_ar', 'formant_ar', 'v_osc_ar',
  'osc_ar', 'osc_kr', 'klang_ar', 'dynklang_ar',
  // Noise
  'white_noise_ar', 'white_noise_kr', 'pink_noise_ar', 'brown_noise_ar',
  'gray_noise_ar', 'clip_noise_ar', 'lf_noise0_ar', 'lf_noise0_kr',
  'lf_noise1_ar', 'lf_noise1_kr', 'lf_noise2_ar', 'lf_noise2_kr',
  'dust_ar', 'dust2_ar', 'crackle_ar', 'lf_clip_noise_ar',
  // Filters
  'lpf_ar', 'hpf_ar', 'bpf_ar', 'brf_ar', 'rlpf_ar', 'rhpf_ar',
  'resonz_ar', 'ringz_ar', 'formlet_ar', 'moog_ff_ar', 'moog_ladder_ar',
  'bhi_pass4_ar', 'blo_pass4_ar', 'bbandpass_ar', 'bbandstop_ar',
  'one_pole_ar', 'one_zero_ar', 'two_pole_ar', 'two_zero_ar',
  'sos_ar', 'dc_ar', 'leak_dc_ar', 'integrator_ar', 'decay_ar', 'decay2_ar',
  'lag_ar', 'lag_kr', 'lag2_ar', 'lag3_ar', 'ramp_ar', 'var_lag_ar',
  'slew_ar', 'median_ar', 'lf_det_ar', 'mid_eq_ar', 'bi_eq_ar',
  'svf_ar', 'bmoog_ar', 'dfm1_ar', 'diode_ring_mod_ar',
  // Envelopes
  'env_gen_ar', 'env_gen_kr', 'linen_ar', 'linen_kr', 'line_ar', 'line_kr',
  'x_line_ar', 'x_line_kr', 'amp_comp_ar', 'amp_comp_a_ar',
  // Delays
  'delay_n_ar', 'delay_l_ar', 'delay_c_ar', 'comb_n_ar', 'comb_l_ar', 'comb_c_ar',
  'allpass_n_ar', 'allpass_l_ar', 'allpass_c_ar', 'pluck_ar',
  'buf_delay_n_ar', 'buf_delay_l_ar', 'buf_delay_c_ar',
  'buf_comb_n_ar', 'buf_comb_l_ar', 'buf_comb_c_ar',
  // Reverb
  'free_verb_ar', 'free_verb2_ar', 'g_verb_ar',
  // Dynamics
  'limiter_ar', 'normalizer_ar', 'compander_ar', 'amplitude_ar', 'amplitude_kr',
  'peak_follower_ar', 'detect_silence_ar',
  // Buffer
  'play_buf_ar', 'buf_rd_ar', 'buf_wr_ar', 'record_buf_ar', 'buf_frames_ir',
  'buf_rate_scale_ir', 'buf_dur_ir', 'buf_channels_ir', 'buf_sample_rate_ir',
  'local_buf_ar', 'set_buf_ar', 'clear_buf_ar', 'scope_buf_ar',
  // Triggers
  'trig_ar', 'trig1_ar', 't_delay_ar', 't_rand_ar', 't_i_rand_ar', 't_choose_ar',
  'stepper_ar', 'pulse_count_ar', 'pulse_divider_ar', 'toggle_ff_ar', 'set_reset_ff_ar',
  'sweep_ar', 'phasor_ar', 'peak_ar', 'running_min_ar', 'running_max_ar',
  'latch_ar', 'gate_ar', 'schmidt_ar', 'in_range_ar', 'changed_ar', 'trigger_ar',
  // Panning
  'pan2_ar', 'balance2_ar', 'rotate2_ar', 'pan_az_ar', 'pan_b_ar', 'pan_b2_ar',
  'bi_pan_b2_ar', 'decode_b2_ar', 'splay_ar', 'splay_az_ar', 'lin_pan2_ar',
  'x_fade2_ar', 'lin_x_fade2_ar',
  // Control
  'in_ar', 'in_kr', 'local_in_ar', 'local_out_ar', 'in_feedback_ar',
  'out_ar', 'replace_out_ar', 'x_out_ar', 'offset_out_ar',
  'control_kr', 'trig_control_ar', 'lag_control_kr', 'in_trig_ar',
  // Random
  'rand_ar', 'i_rand_ar', 'lin_rand_ar', 'n_rand_ar', 'exp_rand_ar',
  'coin_gate_ar', 'random_seed_ar',
  // Pitch/Time
  'pitch_shift_ar', 'pitch_ar', 'zero_crossing_ar', 'timer_ar',
  // FFT
  'fft_ar', 'ifft_ar', 'pv_brick_wall_ar', 'pv_bin_scramble_ar',
  'pv_mag_noise_ar', 'pv_phase_shift_ar', 'pv_copy_ar', 'pv_max_ar', 'pv_min_ar',
  'pv_mul_ar', 'pv_add_ar', 'pv_mag_ar', 'pv_mag_squared_ar',
  // Physical Modeling
  'spring_ar', 'ball_ar', 't_ball_ar', 'membrane_circle_ar',
  'membrane_hexagon_ar',
  // Analysis
  'running_sum_ar', 'running_sum_kr',
  // Granular
  'grain_sin_ar', 'grain_fm_ar', 'grain_buf_ar', 'grain_in_ar', 'warp1_ar',
  't_grains_ar', 't_grains2_ar', 't_grains3_ar',
  // Multichannel
  'mix_ar', 'select_ar', 'select_kr',
  // Info
  'sample_rate_ir', 'sample_dur_ir', 'control_rate_ir', 'control_dur_ir',
  'sub_sample_offset_ir', 'radians_per_sample_ir', 'num_input_buses_ir',
  'num_output_buses_ir', 'num_audio_buses_ir', 'num_control_buses_ir', 'num_buffers_ir',
  'num_running_synths_ir', 'node_id_ir'
]);

// Methods (called with dot notation)
const METHODS = new Set([
  // Voice methods
  'synth', 'on', 'poly', 'gain', 'set_param', 'mute', 'unmute', 'solo',
  'trigger', 'note_on', 'note_off', 'control_change', 'stop', 'stop_all',
  'run', 'fade_param',
  // Pattern/Melody methods
  'step', 'euclid', 'len', 'swing', 'quantize', 'lane', 'start', 'apply',
  'launch', 'is_playing', 'notes', 'scale', 'root', 'gate', 'transpose',
  // Sequence methods
  'loop_bars', 'loop_beats', 'clip', 'clip_once', 'clip_loops', 'start_once',
  'pause', 'resume',
  // Fade methods
  'on_group', 'on_voice', 'on_effect', 'on_pattern', 'on_melody',
  'param', 'from', 'to', 'over', 'over_bars', 'register',
  // Synthdef/Fx builder
  'body',
  // Envelope builder
  'perc', 'adsr', 'asr', 'attack', 'decay', 'sustain', 'release', 'triangle',
  'time_scale', 'level_scale', 'cleanup_on_finish', 'build',
  // Group methods
  'route_to', 'send', 'add_effect', 'remove_effect', 'get_effect', 'get_effects',
  'clear_effects', 'effect_count', 'is_active', 'fade_gain_to', 'stutter',
  'filter_sweep', 'name', 'parent',
  // Scheduling
  'now', 'after', 'at',
  // Effect
  'bypass', 'set', 'remove',
  // Sample
  'warp_to_bpm', 'analyze_bpm', 'slice', 'trigger_slice', 'buffer_id',
  'synthdef_name', 'duration', 'sample_rate', 'num_channels', 'detected_bpm',
  'offset', 'length', 'rate', 'loop_mode', 'amp', 'warp', 'set_speed',
  'set_pitch', 'semitones', 'window_size', 'overlaps',
  // SFZ
  'num_regions', 'info', 'id', 'path',
  // Lane
  'values',
  // Recording
  'from_group', 'bars', 'beats', 'seconds', 'count_in', 'metronome',
  'to_file', 'immediate', 'start_at_bar', 'channels',
  // MelodyGen
  'tonality', 'density', 'seed', 'render', 'render_step', 'motif', 'contour',
  'random_walk', 'reverse', 'rotate', 'palindrome', 'degrade', 'humanize',
  'striate', 'polyrhythm', 'degrees', 'base_gate', 'base_amp', 'range',
  // MIDI
  'input', 'output', 'in_out', 'channel', 'cc', 'pad', 'led',
  // Automation
  'to_param', 'curve', 'loop', 'glide_ms',
  // Array
  'map', 'filter', 'reduce', 'zip', 'sum', 'push', 'pop', 'shift'
]);

// Rhai keywords
const KEYWORDS = new Set([
  'let', 'const', 'if', 'else', 'for', 'while', 'loop', 'in',
  'fn', 'return', 'true', 'false', 'import', 'export',
  'continue', 'break', 'throw', 'try', 'catch', 'this', 'switch', 'case'
]);

// Simple syntax highlighter for VibeLang code
export function highlightCode(code) {
  const lines = code.split('\n');

  return lines.map((line, lineIndex) => {
    const tokens = tokenizeLine(line);
    return (
      <React.Fragment key={lineIndex}>
        {tokens.map((token, i) => (
          <span key={i} className={token.type ? `hl-${token.type}` : undefined}>
            {token.value}
          </span>
        ))}
        {lineIndex < lines.length - 1 && '\n'}
      </React.Fragment>
    );
  });
}

function tokenizeLine(line) {
  const tokens = [];
  let remaining = line;

  while (remaining.length > 0) {
    let matched = false;

    // Comments
    if (remaining.startsWith('//')) {
      tokens.push({ type: 'comment', value: remaining });
      remaining = '';
      matched = true;
    }

    // Strings
    if (!matched) {
      const stringMatch = remaining.match(/^"([^"\\]|\\.)*"/);
      if (stringMatch) {
        tokens.push({ type: 'string', value: stringMatch[0] });
        remaining = remaining.slice(stringMatch[0].length);
        matched = true;
      }
    }

    // Single-quoted strings
    if (!matched) {
      const singleStringMatch = remaining.match(/^'([^'\\]|\\.)*'/);
      if (singleStringMatch) {
        tokens.push({ type: 'string', value: singleStringMatch[0] });
        remaining = remaining.slice(singleStringMatch[0].length);
        matched = true;
      }
    }

    // Keywords (must check before identifiers)
    if (!matched) {
      const keywordMatch = remaining.match(/^([a-zA-Z_][a-zA-Z0-9_]*)\b/);
      if (keywordMatch && KEYWORDS.has(keywordMatch[1])) {
        tokens.push({ type: 'keyword', value: keywordMatch[1] });
        remaining = remaining.slice(keywordMatch[1].length);
        matched = true;
      }
    }

    // Built-in functions
    if (!matched) {
      const builtinMatch = remaining.match(/^([a-zA-Z_][a-zA-Z0-9_]*)\b/);
      if (builtinMatch && BUILTIN_FUNCTIONS.has(builtinMatch[1])) {
        tokens.push({ type: 'builtin', value: builtinMatch[1] });
        remaining = remaining.slice(builtinMatch[1].length);
        matched = true;
      }
    }

    // UGen functions
    if (!matched) {
      const ugenMatch = remaining.match(/^([a-zA-Z_][a-zA-Z0-9_]*)\b/);
      if (ugenMatch && UGEN_FUNCTIONS.has(ugenMatch[1])) {
        tokens.push({ type: 'ugen', value: ugenMatch[1] });
        remaining = remaining.slice(ugenMatch[1].length);
        matched = true;
      }
    }

    // Methods (after dots)
    if (!matched) {
      const methodMatch = remaining.match(/^\.([a-zA-Z_][a-zA-Z0-9_]*)\b/);
      if (methodMatch && METHODS.has(methodMatch[1])) {
        tokens.push({ type: 'punctuation', value: '.' });
        tokens.push({ type: 'method', value: methodMatch[1] });
        remaining = remaining.slice(methodMatch[0].length);
        matched = true;
      }
    }

    // Numbers (including floats and negative)
    if (!matched) {
      const numberMatch = remaining.match(/^-?\d+\.?\d*([eE][+-]?\d+)?/);
      if (numberMatch) {
        tokens.push({ type: 'number', value: numberMatch[0] });
        remaining = remaining.slice(numberMatch[0].length);
        matched = true;
      }
    }

    // Closure pipes |param1, param2|
    if (!matched) {
      const pipeMatch = remaining.match(/^\|([^|]*)\|/);
      if (pipeMatch) {
        tokens.push({ type: 'bracket', value: '|' });
        // Tokenize params inside
        const params = pipeMatch[1].split(',');
        params.forEach((param, i) => {
          if (i > 0) tokens.push({ type: 'punctuation', value: ',' });
          const trimmed = param.trim();
          if (trimmed) tokens.push({ type: 'identifier', value: ' ' + trimmed + ' ' });
        });
        tokens.push({ type: 'bracket', value: '|' });
        remaining = remaining.slice(pipeMatch[0].length);
        matched = true;
      }
    }

    // Identifiers
    if (!matched) {
      const identMatch = remaining.match(/^[a-zA-Z_][a-zA-Z0-9_]*/);
      if (identMatch) {
        tokens.push({ type: 'identifier', value: identMatch[0] });
        remaining = remaining.slice(identMatch[0].length);
        matched = true;
      }
    }

    // Range operator
    if (!matched && remaining.startsWith('..')) {
      tokens.push({ type: 'operator', value: '..' });
      remaining = remaining.slice(2);
      matched = true;
    }

    // Operators and punctuation
    if (!matched) {
      const opMatch = remaining.match(/^(\|\||&&|==|!=|<=|>=|=>|->|\+|-|\*|\/|=|<|>|\||\&|!|%|\^)/);
      if (opMatch) {
        tokens.push({ type: 'operator', value: opMatch[0] });
        remaining = remaining.slice(opMatch[0].length);
        matched = true;
      }
    }

    // Map literal #{
    if (!matched && remaining.startsWith('#{')) {
      tokens.push({ type: 'bracket', value: '#{' });
      remaining = remaining.slice(2);
      matched = true;
    }

    // Brackets and parens
    if (!matched) {
      const bracketMatch = remaining.match(/^[(){}\[\]]/);
      if (bracketMatch) {
        tokens.push({ type: 'bracket', value: bracketMatch[0] });
        remaining = remaining.slice(1);
        matched = true;
      }
    }

    // Other punctuation
    if (!matched) {
      const punctMatch = remaining.match(/^[,;:.]/);
      if (punctMatch) {
        tokens.push({ type: 'punctuation', value: punctMatch[0] });
        remaining = remaining.slice(1);
        matched = true;
      }
    }

    // Whitespace
    if (!matched) {
      const wsMatch = remaining.match(/^\s+/);
      if (wsMatch) {
        tokens.push({ type: null, value: wsMatch[0] });
        remaining = remaining.slice(wsMatch[0].length);
        matched = true;
      }
    }

    // Fallback: single character
    if (!matched && remaining.length > 0) {
      tokens.push({ type: null, value: remaining[0] });
      remaining = remaining.slice(1);
    }
  }

  return tokens;
}

// Styled code component with syntax highlighting
export function HighlightedCode({ code, className = '' }) {
  return (
    <pre className={`highlighted-code ${className}`}>
      <code>{highlightCode(code)}</code>
    </pre>
  );
}

export default HighlightedCode;
