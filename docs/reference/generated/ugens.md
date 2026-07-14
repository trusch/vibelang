# Generated UGen function index

> Generated mechanically from `crates/vibelang-dsp/ugen_manifests/*.json` using the exact `build.rs` snake-case and rate rules. Do not hand-edit individual entries.

This page exhaustively indexes **1199 registered functions** from **827 callable manifest classes**. The remaining **48 builder-only records** are listed separately and are not registered by the generator. The 1,199 callable names are unique.

## Exact generated-call contract

For a manifest class with ordered inputs `p1..pN`, every listed rate-suffixed function registers the exact positional overloads `f()`, `f(p1)`, ... through `f(p1,...,pN)` when `N <= 20`. If `N > 20`, positional overloads stop at 20 and an additional `f(values: Array)` requires exactly N entries. Omitted inputs use the numeric manifest default shown below; a missing/non-numeric default becomes 0. Provided signal inputs accept a number or NodeRef, except special array/pseudo lowerings.

A shape input must be finite, integral, and 1..32767. Invalid Dynamic conversion or graph construction often reaches `unwrap()` and can panic. Output is NodeRef unless a documented pseudo-lowering returns a Dynamic multichannel shape. Plugin availability depends on the connected scsynth/browser backend even when the Rhai name exists.

The generator ignores the manifest's historical `functions` field and derives names from class + rate; this matters for acronyms such as `K2A` (`k2a_ar`). Source: [`build.rs`](../../../crates/vibelang-dsp/build.rs#L432-L806).

## Callable functions

### analysis.json

Manifest: [`analysis.json`](../../../crates/vibelang-dsp/ugen_manifests/analysis.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `RunningSum` | `running_sum_ar`<br>`running_sum_kr` | 0..2 positional | `in` (signal; default `0`)<br>`numsamp` (float; default `40`) | 1 channel | backend UGen must be installed |
| `RunningMin` | `running_min_ar`<br>`running_min_kr` | 0..2 positional | `in` (signal; default `0`)<br>`numsamp` (float; default `40`) | 1 channel | backend UGen must be installed |
| `RunningMax` | `running_max_ar`<br>`running_max_kr` | 0..2 positional | `in` (signal; default `0`)<br>`numsamp` (float; default `40`) | 1 channel | backend UGen must be installed |
| `PeakFollower` | `peak_follower_ar`<br>`peak_follower_kr` | 0..2 positional | `in` (signal; default `0`)<br>`decay` (float; default `0.999`) | 1 channel | backend UGen must be installed |
| `ZeroCrossing` | `zero_crossing_ar`<br>`zero_crossing_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Pitch` | `pitch_kr` | 0..10 positional | `in` (signal; default `0`)<br>`initFreq` (float; default `440`)<br>`minFreq` (float; default `60`)<br>`maxFreq` (float; default `4000`)<br>`execFreq` (float; default `100`)<br>`maxBinsPerOctave` (float; default `16`)<br>`median` (float; default `1`)<br>`ampThreshold` (float; default `0.01`)<br>`peakThreshold` (float; default `0.5`)<br>`downSample` (float; default `1`) | 2 channels | backend UGen must be installed |
| `BeatTrack` | `beat_track_kr` | 0..2 positional | `chain` (signal; default `0`)<br>`lock` (float; default `0`) | 4 channels | backend UGen must be installed |
| `BeatTrack2` | `beat_track2_kr` | 0..6 positional | `busindex` (float; default `0`)<br>`numfeatures` (float; default `1`)<br>`windowsize` (float; default `2`)<br>`phaseaccuracy` (float; default `0.02`)<br>`lock` (float; default `0`)<br>`weightingscheme` (float; default `-2.1`) | 6 channels | backend UGen must be installed |
| `KeyTrack` | `key_track_kr` | 0..3 positional | `chain` (signal; default `0`)<br>`keydecay` (float; default `2`)<br>`chromaleak` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `MFCC` | `mfcc_kr` | 0..2 positional | `chain` (signal; default `0`)<br>`numcoeff` (int; default `13`) | 13 channels | backend UGen must be installed |
| `Loudness` | `loudness_kr` | 0..3 positional | `chain` (signal; default `0`)<br>`smask` (float; default `0.25`)<br>`tmask` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Onsets` | `onsets_kr` | 0..9 positional | `chain` (signal; default `0`)<br>`threshold` (float; default `0.5`)<br>`odftype` (int; default `3`)<br>`relaxtime` (float; default `1`)<br>`floor` (float; default `0.1`)<br>`mingap` (int; default `10`)<br>`medianspan` (int; default `11`)<br>`whtype` (int; default `1`)<br>`rawodf` (int; default `0`) | 1 channel | backend UGen must be installed |
| `SpecCentroid` | `spec_centroid_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `SpecFlatness` | `spec_flatness_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `SpecPcile` | `spec_pcile_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`fraction` (float; default `0.5`)<br>`interpolate` (int; default `0`)<br>`binout` (int; default `0`) | 1 channel | backend UGen must be installed |
| `PV_HainsworthFoote` | `pv_hainsworth_foote_ar` | 0..5 positional | `buffer` (signal; default `0`)<br>`proph` (float; default `0`)<br>`propf` (float; default `0`)<br>`threshold` (float; default `1`)<br>`waittime` (float; default `0.04`) | 1 channel | backend UGen must be installed |
| `PV_JensenAndersen` | `pv_jensen_andersen_ar` | 0..7 positional | `buffer` (signal; default `0`)<br>`propsc` (float; default `0.25`)<br>`prophfe` (float; default `0.25`)<br>`prophfc` (float; default `0.25`)<br>`propsf` (float; default `0.25`)<br>`threshold` (float; default `1`)<br>`waittime` (float; default `0.04`) | 1 channel | backend UGen must be installed |

### atk_foa.json

Manifest: [`atk_foa.json`](../../../crates/vibelang-dsp/ugen_manifests/atk_foa.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `FoaPanB` | `foa_pan_b_ar`<br>`foa_pan_b_kr` | 0..3 positional | `in` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaDirectO` | `foa_direct_o_ar`<br>`foa_direct_o_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaDirectX` | `foa_direct_x_ar`<br>`foa_direct_x_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaDirectY` | `foa_direct_y_ar`<br>`foa_direct_y_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaDirectZ` | `foa_direct_z_ar`<br>`foa_direct_z_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaRotate` | `foa_rotate_ar`<br>`foa_rotate_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaTilt` | `foa_tilt_ar`<br>`foa_tilt_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaTumble` | `foa_tumble_ar`<br>`foa_tumble_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaFocusX` | `foa_focus_x_ar`<br>`foa_focus_x_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaFocusY` | `foa_focus_y_ar`<br>`foa_focus_y_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaFocusZ` | `foa_focus_z_ar`<br>`foa_focus_z_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaPushX` | `foa_push_x_ar`<br>`foa_push_x_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaPushY` | `foa_push_y_ar`<br>`foa_push_y_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaPushZ` | `foa_push_z_ar`<br>`foa_push_z_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaPressX` | `foa_press_x_ar`<br>`foa_press_x_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaPressY` | `foa_press_y_ar`<br>`foa_press_y_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaPressZ` | `foa_press_z_ar`<br>`foa_press_z_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaZoomX` | `foa_zoom_x_ar`<br>`foa_zoom_x_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaZoomY` | `foa_zoom_y_ar`<br>`foa_zoom_y_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaZoomZ` | `foa_zoom_z_ar`<br>`foa_zoom_z_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaDominateX` | `foa_dominate_x_ar`<br>`foa_dominate_x_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`gain` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaDominateY` | `foa_dominate_y_ar`<br>`foa_dominate_y_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`gain` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaDominateZ` | `foa_dominate_z_ar`<br>`foa_dominate_z_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`gain` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaAsymmetry` | `foa_asymmetry_ar`<br>`foa_asymmetry_kr` | 0..5 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`angle` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FoaNFC` | `foa_nfc_ar`<br>`foa_nfc_kr` | 0..6 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`distance` (float; default `1`)<br>`speedOfSound` (float; default `343`) | 4 channels | backend UGen must be installed |
| `FoaProximity` | `foa_proximity_ar`<br>`foa_proximity_kr` | 0..6 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`distance` (float; default `1`)<br>`speedOfSound` (float; default `343`) | 4 channels | backend UGen must be installed |
| `FoaPsychoShelf` | `foa_psycho_shelf_ar`<br>`foa_psycho_shelf_kr` | 0..7 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`freq` (float; default `400`)<br>`k0` (float; default `1`)<br>`k1` (float; default `1`) | 4 channels | backend UGen must be installed |

### bufdelays.json

Manifest: [`bufdelays.json`](../../../crates/vibelang-dsp/ugen_manifests/bufdelays.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `BufDelayN` | `buf_delay_n_ar`<br>`buf_delay_n_kr` | 0..3 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `BufDelayL` | `buf_delay_l_ar`<br>`buf_delay_l_kr` | 0..3 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `BufDelayC` | `buf_delay_c_ar`<br>`buf_delay_c_kr` | 0..3 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `BufCombN` | `buf_comb_n_ar` | 0..4 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BufCombL` | `buf_comb_l_ar` | 0..4 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BufCombC` | `buf_comb_c_ar` | 0..4 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BufAllpassN` | `buf_allpass_n_ar` | 0..4 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BufAllpassL` | `buf_allpass_l_ar` | 0..4 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BufAllpassC` | `buf_allpass_c_ar` | 0..4 positional | `buf` (float; default `0`)<br>`in` (signal; default `0`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |

### buffers.json

Manifest: [`buffers.json`](../../../crates/vibelang-dsp/ugen_manifests/buffers.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `PlayBuf` | `play_buf_ar`<br>`play_buf_kr` | 0..7 positional | `numChannels` (float; default `1`)<br>`bufnum` (float; default `0`)<br>`rate` (float; default `1`)<br>`trigger` (signal; default `1`)<br>`startPos` (float; default `0`)<br>`loop` (float; default `0`)<br>`doneAction` (float; default `0`) | channels from `numChannels` (manifest default 1) | backend UGen must be installed |
| `BufRd` | `buf_rd_ar`<br>`buf_rd_kr` | 0..5 positional | `numChannels` (float; default `1`)<br>`bufnum` (float; default `0`)<br>`phase` (signal; default `0`)<br>`loop` (float; default `1`)<br>`interpolation` (float; default `2`) | channels from `numChannels` (manifest default 1) | backend UGen must be installed |
| `BufWr` | `buf_wr_ar`<br>`buf_wr_kr` | 0..4 positional | `inputArray` (signal; default `0`)<br>`bufnum` (float; default `0`)<br>`phase` (signal; default `0`)<br>`loop` (float; default `1`) | 0 channels; Array input is flattened | backend UGen must be installed |
| `RecordBuf` | `record_buf_ar`<br>`record_buf_kr` | 0..9 positional | `inputArray` (signal; default `0`)<br>`bufnum` (float; default `0`)<br>`offset` (float; default `0`)<br>`recLevel` (float; default `1`)<br>`preLevel` (float; default `0`)<br>`run` (float; default `1`)<br>`loop` (float; default `1`)<br>`trigger` (signal; default `1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BufFrames` | `buf_frames_ir`<br>`buf_frames_kr` | 0..1 positional | `bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BufDur` | `buf_dur_ir`<br>`buf_dur_kr` | 0..1 positional | `bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BufChannels` | `buf_channels_ir`<br>`buf_channels_kr` | 0..1 positional | `bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BufSampleRate` | `buf_sample_rate_ir`<br>`buf_sample_rate_kr` | 0..1 positional | `bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BufRateScale` | `buf_rate_scale_ir`<br>`buf_rate_scale_kr` | 0..1 positional | `bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BufSamples` | `buf_samples_ir`<br>`buf_samples_kr` | 0..1 positional | `bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Warp1` | `warp1_ar` | 0..9 positional | `numChannels` (float; default `1`)<br>`bufnum` (float; default `0`)<br>`pointer` (signal; default `0`)<br>`freqScale` (float; default `1`)<br>`windowSize` (float; default `0.2`)<br>`envbufnum` (float; default `-1`)<br>`overlaps` (float; default `8`)<br>`windowRandRatio` (float; default `0`)<br>`interp` (float; default `1`) | channels from `numChannels` (manifest default 1) | backend UGen must be installed |
| `LocalBuf` | `local_buf_ir` | 0..2 positional | `numChannels` (float; default `1`)<br>`numFrames` (float; default `1`) | 1 channel | backend UGen must be installed |
| `MaxLocalBufs` | `max_local_bufs_ir` | 0..1 positional | `numBufs` (float; default `0`) | 1 channel | backend UGen must be installed |
| `SetBuf` | `set_buf_ir` | 0..3 positional | `buffer` (float; default `0`)<br>`offset` (float; default `0`)<br>`values` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `ClearBuf` | `clear_buf_ir` | 0..1 positional | `buffer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `ScopeOut` | `scope_out_ar`<br>`scope_out_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`inputArray` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `ScopeOut2` | `scope_out2_ar`<br>`scope_out2_kr` | 0..4 positional | `scopeNum` (float; default `0`)<br>`maxFrames` (float; default `4096`)<br>`scopeFrames` (float; default `4096`)<br>`inputArray` (signal; default `0`) | 1 channel | backend UGen must be installed |

### chaos.json

Manifest: [`chaos.json`](../../../crates/vibelang-dsp/ugen_manifests/chaos.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `CuspN` | `cusp_n_ar` | 0..4 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `1.9`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `CuspL` | `cusp_l_ar` | 0..4 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `1.9`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `FBSineN` | `fb_sine_n_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`im` (float; default `1`)<br>`fb` (float; default `0.1`)<br>`a` (float; default `1.1`)<br>`c` (float; default `0.5`)<br>`xi` (float; default `0.1`)<br>`yi` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `FBSineL` | `fb_sine_l_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`im` (float; default `1`)<br>`fb` (float; default `0.1`)<br>`a` (float; default `1.1`)<br>`c` (float; default `0.5`)<br>`xi` (float; default `0.1`)<br>`yi` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `FBSineC` | `fb_sine_c_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`im` (float; default `1`)<br>`fb` (float; default `0.1`)<br>`a` (float; default `1.1`)<br>`c` (float; default `0.5`)<br>`xi` (float; default `0.1`)<br>`yi` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `GbmanN` | `gbman_n_ar` | 0..3 positional | `freq` (float; default `22050`)<br>`xi` (float; default `1.2`)<br>`yi` (float; default `2.1`) | 1 channel | backend UGen must be installed |
| `GbmanL` | `gbman_l_ar` | 0..3 positional | `freq` (float; default `22050`)<br>`xi` (float; default `1.2`)<br>`yi` (float; default `2.1`) | 1 channel | backend UGen must be installed |
| `HenonN` | `henon_n_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1.4`)<br>`b` (float; default `0.3`)<br>`x0` (float; default `0`)<br>`x1` (float; default `0`) | 1 channel | backend UGen must be installed |
| `HenonL` | `henon_l_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1.4`)<br>`b` (float; default `0.3`)<br>`x0` (float; default `0`)<br>`x1` (float; default `0`) | 1 channel | backend UGen must be installed |
| `HenonC` | `henon_c_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1.4`)<br>`b` (float; default `0.3`)<br>`x0` (float; default `0`)<br>`x1` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LatoocarfianN` | `latoocarfian_n_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `3`)<br>`c` (float; default `0.5`)<br>`d` (float; default `0.5`)<br>`xi` (float; default `0.5`)<br>`yi` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `LatoocarfianL` | `latoocarfian_l_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `3`)<br>`c` (float; default `0.5`)<br>`d` (float; default `0.5`)<br>`xi` (float; default `0.5`)<br>`yi` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `LatoocarfianC` | `latoocarfian_c_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `3`)<br>`c` (float; default `0.5`)<br>`d` (float; default `0.5`)<br>`xi` (float; default `0.5`)<br>`yi` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `LinCongN` | `lin_cong_n_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1.1`)<br>`c` (float; default `0.13`)<br>`m` (float; default `1`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LinCongL` | `lin_cong_l_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1.1`)<br>`c` (float; default `0.13`)<br>`m` (float; default `1`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LinCongC` | `lin_cong_c_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1.1`)<br>`c` (float; default `0.13`)<br>`m` (float; default `1`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LorenzL` | `lorenz_l_ar` | 0..8 positional | `freq` (float; default `22050`)<br>`s` (float; default `10`)<br>`r` (float; default `28`)<br>`b` (float; default `2.667`)<br>`h` (float; default `0.05`)<br>`xi` (float; default `0.1`)<br>`yi` (float; default `0`)<br>`zi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `QuadN` | `quad_n_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `-1`)<br>`c` (float; default `-0.75`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `QuadL` | `quad_l_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `-1`)<br>`c` (float; default `-0.75`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `QuadC` | `quad_c_ar` | 0..5 positional | `freq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `-1`)<br>`c` (float; default `-0.75`)<br>`xi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `StandardN` | `standard_n_ar` | 0..4 positional | `freq` (float; default `22050`)<br>`k` (float; default `1`)<br>`xi` (float; default `0.5`)<br>`yi` (float; default `0`) | 1 channel | backend UGen must be installed |
| `StandardL` | `standard_l_ar` | 0..4 positional | `freq` (float; default `22050`)<br>`k` (float; default `1`)<br>`xi` (float; default `0.5`)<br>`yi` (float; default `0`) | 1 channel | backend UGen must be installed |

### control.json

Manifest: [`control.json`](../../../crates/vibelang-dsp/ugen_manifests/control.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `MouseX` | `mouse_x_kr` | 0..4 positional | `minval` (float; default `0`)<br>`maxval` (float; default `1`)<br>`warp` (float; default `0`)<br>`lag` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `MouseY` | `mouse_y_kr` | 0..4 positional | `minval` (float; default `0`)<br>`maxval` (float; default `1`)<br>`warp` (float; default `0`)<br>`lag` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `MouseButton` | `mouse_button_kr` | 0..3 positional | `minval` (float; default `0`)<br>`maxval` (float; default `1`)<br>`lag` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `KeyState` | `key_state_kr` | 0..4 positional | `keycode` (float; default `0`)<br>`minval` (float; default `0`)<br>`maxval` (float; default `1`)<br>`lag` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `Phasor` | `phasor_ar`<br>`phasor_kr` | 0..5 positional | `trig` (signal; default `0`)<br>`rate` (float; default `1`)<br>`start` (float; default `0`)<br>`end` (float; default `1`)<br>`resetPos` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Sweep` | `sweep_ar`<br>`sweep_kr` | 0..2 positional | `trig` (signal; default `0`)<br>`rate` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Select` | `select_ar`<br>`select_kr` | 0..2 positional | `which` (float; default `0`)<br>`array` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Timer` | `timer_ar`<br>`timer_kr` | 0..1 positional | `trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Slope` | `slope_ar`<br>`slope_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `LFGauss` | `lf_gauss_ar`<br>`lf_gauss_kr` | 0..5 positional | `duration` (float; default `1`)<br>`width` (float; default `0.1`)<br>`iphase` (float; default `0`)<br>`loop` (float; default `1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |

### conversion.json

Manifest: [`conversion.json`](../../../crates/vibelang-dsp/ugen_manifests/conversion.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `K2A` | `k2a_ar` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `A2K` | `a2k_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `DC` | `dc_ar`<br>`dc_kr` | 0..1 positional | `in` (float; default `0`) | 1 channel | backend UGen must be installed |
| `T2A` | `t2a_ar` | 0..2 positional | `in` (signal; default `0`)<br>`offset` (float; default `0`) | 1 channel | backend UGen must be installed |
| `T2K` | `t2k_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |

### convolution.json

Manifest: [`convolution.json`](../../../crates/vibelang-dsp/ugen_manifests/convolution.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Convolution` | `convolution_ar` | 0..3 positional | `in` (signal; default `0`)<br>`kernel` (signal; default `0`)<br>`framesize` (float; default `1024`) | 1 channel | backend UGen must be installed |
| `Convolution2` | `convolution2_ar` | 0..4 positional | `in` (signal; default `0`)<br>`kernel` (float; default `0`)<br>`trigger` (signal; default `0`)<br>`framesize` (float; default `2048`) | 1 channel | backend UGen must be installed |
| `Convolution2L` | `convolution2l_ar` | 0..5 positional | `in` (signal; default `0`)<br>`kernel` (float; default `0`)<br>`trigger` (signal; default `0`)<br>`framesize` (float; default `2048`)<br>`crossfade` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Convolution3` | `convolution3_ar`<br>`convolution3_kr` | 0..4 positional | `in` (signal; default `0`)<br>`kernel` (float; default `0`)<br>`trigger` (signal; default `0`)<br>`framesize` (float; default `2048`) | 1 channel | backend UGen must be installed |
| `StereoConvolution2L` | `stereo_convolution2l_ar` | 0..6 positional | `in` (signal; default `0`)<br>`kernelL` (float; default `0`)<br>`kernelR` (float; default `0`)<br>`trigger` (signal; default `0`)<br>`framesize` (float; default `2048`)<br>`crossfade` (float; default `1`) | 2 channels | backend UGen must be installed |
| `PartConv` | `part_conv_ar` | 0..3 positional | `in` (signal; default `0`)<br>`fftsize` (float; default `2048`)<br>`irbufnum` (float; default `0`) | 1 channel | backend UGen must be installed |

### delays.json

Manifest: [`delays.json`](../../../crates/vibelang-dsp/ugen_manifests/delays.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Delay1` | `delay1_ar`<br>`delay1_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Delay2` | `delay2_ar`<br>`delay2_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `DelayN` | `delay_n_ar`<br>`delay_n_kr` | 0..3 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `DelayL` | `delay_l_ar`<br>`delay_l_kr` | 0..3 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `DelayC` | `delay_c_ar`<br>`delay_c_kr` | 0..3 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `CombN` | `comb_n_ar`<br>`comb_n_kr` | 0..4 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `CombL` | `comb_l_ar`<br>`comb_l_kr` | 0..4 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `CombC` | `comb_c_ar`<br>`comb_c_kr` | 0..4 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `AllpassN` | `allpass_n_ar`<br>`allpass_n_kr` | 0..4 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `AllpassL` | `allpass_l_ar`<br>`allpass_l_kr` | 0..4 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `AllpassC` | `allpass_c_ar`<br>`allpass_c_kr` | 0..4 positional | `in` (signal; default `0`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `DelTapWr` | `del_tap_wr_ar`<br>`del_tap_wr_kr` | 0..2 positional | `buffer` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `DelTapRd` | `del_tap_rd_ar`<br>`del_tap_rd_kr` | 0..4 positional | `buffer` (float; default `0`)<br>`phase` (signal; default `0`)<br>`delTime` (float; default `0.2`)<br>`interp` (float; default `1`) | 1 channel | backend UGen must be installed |
| `GrainTap` | `grain_tap_ar` | 0..6 positional | `bufnum` (float; default `0`)<br>`grainDur` (float; default `0.2`)<br>`pchRatio` (float; default `1`)<br>`pchDispersion` (float; default `0`)<br>`timeDispersion` (float; default `0`)<br>`overlap` (float; default `2`) | 1 channel | backend UGen must be installed |

### demand.json

Manifest: [`demand.json`](../../../crates/vibelang-dsp/ugen_manifests/demand.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Demand` | `demand_ar`<br>`demand_kr` | 0..3 positional | `trig` (signal; default `0`)<br>`reset` (signal; default `0`)<br>`demandUGens` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `TDuty` | `t_duty_ar`<br>`t_duty_kr` | 0..5 positional | `dur` (signal; default `1`)<br>`reset` (signal; default `0`)<br>`doneAction` (float; default `0`)<br>`level` (signal; default `1`)<br>`gapFirst` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Dseq` | `dseq_demand` | 0..2 positional | `array` (signal; default `0`)<br>`repeats` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Dser` | `dser_demand` | 0..2 positional | `array` (signal; default `0`)<br>`repeats` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Drand` | `drand_demand` | 0..2 positional | `array` (signal; default `0`)<br>`repeats` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Dxrand` | `dxrand_demand` | 0..2 positional | `array` (signal; default `0`)<br>`repeats` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Dseries` | `dseries_demand` | 0..3 positional | `start` (float; default `1`)<br>`step` (float; default `1`)<br>`length` (float; default `100000000`) | 1 channel | backend UGen must be installed |
| `Dgeom` | `dgeom_demand` | 0..3 positional | `start` (float; default `1`)<br>`grow` (float; default `2`)<br>`length` (float; default `100000000`) | 1 channel | backend UGen must be installed |
| `Dbrown` | `dbrown_demand` | 0..4 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`step` (float; default `0.01`)<br>`length` (float; default `100000000`) | 1 channel | backend UGen must be installed |
| `Dwhite` | `dwhite_demand` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`length` (float; default `100000000`) | 1 channel | backend UGen must be installed |
| `Dibrown` | `dibrown_demand` | 0..4 positional | `lo` (float; default `0`)<br>`hi` (float; default `12`)<br>`step` (float; default `1`)<br>`length` (float; default `100000000`) | 1 channel | backend UGen must be installed |
| `Diwhite` | `diwhite_demand` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`length` (float; default `100000000`) | 1 channel | backend UGen must be installed |
| `Dbufrd` | `dbufrd_demand` | 0..3 positional | `bufnum` (signal; default `0`)<br>`phase` (signal; default `0`)<br>`loop` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Dbufwr` | `dbufwr_demand` | 0..4 positional | `input` (signal; default `0`)<br>`bufnum` (signal; default `0`)<br>`phase` (signal; default `0`)<br>`loop` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Dconst` | `dconst_demand` | 0..3 positional | `sum` (signal; default `0`)<br>`in` (signal; default `0`)<br>`tolerance` (float; default `0.001`) | 1 channel | backend UGen must be installed |
| `Ddup` | `ddup_demand` | 0..2 positional | `n` (signal; default `2`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Dstutter` | `dstutter_demand` | 0..2 positional | `n` (signal; default `2`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Dshuf` | `dshuf_demand` | 0..2 positional | `list` (signal; default `0`)<br>`repeats` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Dwrand` | `dwrand_demand` | 0..3 positional | `list` (signal; default `0`)<br>`weights` (signal; default `0`)<br>`repeats` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Dswitch` | `dswitch_demand` | 0..2 positional | `list` (signal; default `0`)<br>`index` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Dswitch1` | `dswitch1_demand` | 0..2 positional | `list` (signal; default `0`)<br>`index` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Dreset` | `dreset_demand` | 0..2 positional | `in` (signal; default `0`)<br>`reset` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Dpoll` | `dpoll_demand` | 0..4 positional | `in` (signal; default `0`)<br>`label` (signal; default `0`)<br>`run` (signal; default `1`)<br>`trigid` (float; default `-1`) | 1 channel | backend UGen must be installed |
| `Duty` | `duty_ar`<br>`duty_kr` | 0..4 positional | `dur` (signal; default `1`)<br>`reset` (signal; default `0`)<br>`doneAction` (float; default `0`)<br>`level` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `DemandEnvGen` | `demand_env_gen_ar`<br>`demand_env_gen_kr` | 0..10 positional | `level` (signal; default `0`)<br>`dur` (signal; default `1`)<br>`shape` (signal; default `1`)<br>`curve` (signal; default `0`)<br>`gate` (signal; default `1`)<br>`reset` (signal; default `1`)<br>`levelScale` (signal; default `1`)<br>`levelBias` (signal; default `0`)<br>`timeScale` (signal; default `1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |

### disk_io.json

Manifest: [`disk_io.json`](../../../crates/vibelang-dsp/ugen_manifests/disk_io.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `DiskIn` | `disk_in_ar` | 0..3 positional | `numChannels` (float; default `1`)<br>`bufnum` (float; default `0`)<br>`loop` (float; default `0`) | 1 channel | backend UGen must be installed |
| `DiskOut` | `disk_out_ar` | 0..2 positional | `bufnum` (float; default `0`)<br>`channelsArray` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `VDiskIn` | `v_disk_in_ar` | 0..5 positional | `numChannels` (float; default `1`)<br>`bufnum` (float; default `0`)<br>`rate` (signal; default `1`)<br>`loop` (float; default `0`)<br>`sendID` (float; default `0`) | 1 channel | backend UGen must be installed |

### dynamics.json

Manifest: [`dynamics.json`](../../../crates/vibelang-dsp/ugen_manifests/dynamics.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Compander` | `compander_ar` | 0..7 positional | `in` (signal; default `0`)<br>`control` (signal; default `0`)<br>`thresh` (float; default `0.5`)<br>`slopeBelow` (float; default `1`)<br>`slopeAbove` (float; default `1`)<br>`clampTime` (float; default `0.01`)<br>`relaxTime` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `Limiter` | `limiter_ar` | 0..3 positional | `in` (signal; default `0`)<br>`level` (float; default `1`)<br>`dur` (float; default `0.01`) | 1 channel | backend UGen must be installed |
| `Normalizer` | `normalizer_ar` | 0..3 positional | `in` (signal; default `0`)<br>`level` (float; default `1`)<br>`dur` (float; default `0.01`) | 1 channel | backend UGen must be installed |
| `Amplitude` | `amplitude_ar`<br>`amplitude_kr` | 0..3 positional | `in` (signal; default `0`)<br>`attackTime` (float; default `0.01`)<br>`releaseTime` (float; default `0.01`) | 1 channel | backend UGen must be installed |

### envelopes.json

Manifest: [`envelopes.json`](../../../crates/vibelang-dsp/ugen_manifests/envelopes.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `EnvGen` | `env_gen_ar`<br>`env_gen_kr` | 0..5 positional | `gate` (float; default `1`)<br>`levelScale` (float; default `1`)<br>`levelBias` (float; default `0`)<br>`timeScale` (float; default `1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Line` | `line_ar`<br>`line_kr` | 0..4 positional | `start` (float; default `0`)<br>`end` (float; default `1`)<br>`dur` (float; default `1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |
| `XLine` | `x_line_ar`<br>`x_line_kr` | 0..4 positional | `start` (float; default `1`)<br>`end` (float; default `2`)<br>`dur` (float; default `1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Lag` | `lag_ar`<br>`lag_kr` | 0..2 positional | `in` (signal; default `0`)<br>`lagTime` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `LagUD` | `lag_ud_ar`<br>`lag_ud_kr` | 0..3 positional | `in` (signal; default `0`)<br>`lagTimeU` (float; default `0.1`)<br>`lagTimeD` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `VarLag` | `var_lag_ar`<br>`var_lag_kr` | 0..5 positional | `in` (signal; default `0`)<br>`time` (float; default `0.1`)<br>`curvature` (float; default `0`)<br>`warp` (float; default `5`)<br>`start` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Decay` | `decay_ar`<br>`decay_kr` | 0..2 positional | `in` (signal; default `0`)<br>`decayTime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Decay2` | `decay2_ar`<br>`decay2_kr` | 0..3 positional | `in` (signal; default `0`)<br>`attackTime` (float; default `0.01`)<br>`decayTime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Linen` | `linen_kr` | 0..5 positional | `gate` (float; default `1`)<br>`attackTime` (float; default `0.01`)<br>`susLevel` (float; default `1`)<br>`releaseTime` (float; default `1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |
| `IEnvGen` | `i_env_gen_ar`<br>`i_env_gen_kr` | 0..3 positional | `index` (signal; default `0`)<br>`mul` (float; default `1`)<br>`add` (float; default `0`) | 1 channel | backend UGen must be installed |

### fft.json

Manifest: [`fft.json`](../../../crates/vibelang-dsp/ugen_manifests/fft.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `FFT` | `fft_kr` | 0..6 positional | `buffer` (float; default `0`)<br>`in` (signal; default `0`)<br>`hop` (float; default `0.5`)<br>`wintype` (float; default `0`)<br>`active` (float; default `1`)<br>`winsize` (float; default `0`) | 1 channel | backend UGen must be installed |
| `IFFT` | `ifft_ar` | 0..3 positional | `buffer` (signal; default `0`)<br>`wintype` (float; default `0`)<br>`winsize` (float; default `0`) | 1 channel | backend UGen must be installed |

### filters.json

Manifest: [`filters.json`](../../../crates/vibelang-dsp/ugen_manifests/filters.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `LPF` | `lpf_ar`<br>`lpf_kr` | 0..2 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `HPF` | `hpf_ar`<br>`hpf_kr` | 0..2 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `BPF` | `bpf_ar`<br>`bpf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BRF` | `brf_ar`<br>`brf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `RLPF` | `rlpf_ar`<br>`rlpf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `RHPF` | `rhpf_ar`<br>`rhpf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `MoogFF` | `moog_ff_ar`<br>`moog_ff_kr` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (float; default `100`)<br>`gain` (float; default `2`)<br>`reset` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Resonz` | `resonz_ar`<br>`resonz_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`bwr` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Formlet` | `formlet_ar`<br>`formlet_kr` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`attacktime` (float; default `1`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Ringz` | `ringz_ar`<br>`ringz_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`decaytime` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Integrator` | `integrator_ar`<br>`integrator_kr` | 0..2 positional | `in` (signal; default `0`)<br>`coef` (float; default `1`) | 1 channel | backend UGen must be installed |
| `LeakDC` | `leak_dc_ar`<br>`leak_dc_kr` | 0..2 positional | `in` (signal; default `0`)<br>`coef` (float; default `0.995`) | 1 channel | backend UGen must be installed |
| `Median` | `median_ar`<br>`median_kr` | 0..2 positional | `length` (float; default `3`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `OnePole` | `one_pole_ar`<br>`one_pole_kr` | 0..2 positional | `in` (signal; default `0`)<br>`coef` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `OneZero` | `one_zero_ar`<br>`one_zero_kr` | 0..2 positional | `in` (signal; default `0`)<br>`coef` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `TwoPole` | `two_pole_ar`<br>`two_pole_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`radius` (float; default `0.8`) | 1 channel | backend UGen must be installed |
| `TwoZero` | `two_zero_ar`<br>`two_zero_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`radius` (float; default `0.8`) | 1 channel | backend UGen must be installed |
| `Slew` | `slew_ar`<br>`slew_kr` | 0..3 positional | `in` (signal; default `0`)<br>`up` (float; default `1`)<br>`dn` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Ramp` | `ramp_ar`<br>`ramp_kr` | 0..2 positional | `in` (signal; default `0`)<br>`lagTime` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `Lag2` | `lag2_ar`<br>`lag2_kr` | 0..2 positional | `in` (signal; default `0`)<br>`lagTime` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `Lag3` | `lag3_ar`<br>`lag3_kr` | 0..2 positional | `in` (signal; default `0`)<br>`lagTime` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `FOS` | `fos_ar`<br>`fos_kr` | 0..4 positional | `in` (signal; default `0`)<br>`a0` (float; default `0`)<br>`a1` (float; default `0`)<br>`b1` (float; default `0`) | 1 channel | backend UGen must be installed |
| `SOS` | `sos_ar`<br>`sos_kr` | 0..6 positional | `in` (signal; default `0`)<br>`a0` (float; default `0`)<br>`a1` (float; default `0`)<br>`a2` (float; default `0`)<br>`b1` (float; default `0`)<br>`b2` (float; default `0`) | 1 channel | backend UGen must be installed |
| `APF` | `apf_ar`<br>`apf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`radius` (float; default `0.8`) | 1 channel | backend UGen must be installed |
| `BLowPass` | `b_low_pass_ar`<br>`b_low_pass_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BHiPass` | `b_hi_pass_ar`<br>`b_hi_pass_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BBandPass` | `b_band_pass_ar`<br>`b_band_pass_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`bw` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BBandStop` | `b_band_stop_ar`<br>`b_band_stop_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`bw` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BPeakEQ` | `b_peak_eq_ar`<br>`b_peak_eq_kr` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`rq` (float; default `1`)<br>`db` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BLowShelf` | `b_low_shelf_ar`<br>`b_low_shelf_kr` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`rs` (float; default `1`)<br>`db` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BHiShelf` | `b_hi_shelf_ar`<br>`b_hi_shelf_kr` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`rs` (float; default `1`)<br>`db` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BAllPass` | `b_all_pass_ar`<br>`b_all_pass_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `BPZ2` | `bpz2_ar`<br>`bpz2_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `BRZ2` | `brz2_ar`<br>`brz2_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `HPZ1` | `hpz1_ar`<br>`hpz1_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `HPZ2` | `hpz2_ar`<br>`hpz2_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `LPZ1` | `lpz1_ar`<br>`lpz1_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `LPZ2` | `lpz2_ar`<br>`lpz2_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `DetectSilence` | `detect_silence_ar`<br>`detect_silence_kr` | 0..4 positional | `in` (signal; default `0`)<br>`amp` (float; default `0.0001`)<br>`time` (float; default `0.1`)<br>`doneAction` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Flip` | `flip_ar` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `FreqShift` | `freq_shift_ar` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `0`)<br>`phase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Hilbert` | `hilbert_ar` | 0..1 positional | `in` (signal; default `0`) | 2 channels | backend UGen must be installed |
| `Lag2UD` | `lag2ud_ar`<br>`lag2ud_kr` | 0..3 positional | `in` (signal; default `0`)<br>`lagTimeU` (float; default `0.1`)<br>`lagTimeD` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `Lag3UD` | `lag3ud_ar`<br>`lag3ud_kr` | 0..3 positional | `in` (signal; default `0`)<br>`lagTimeU` (float; default `0.1`)<br>`lagTimeD` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `MidEQ` | `mid_eq_ar`<br>`mid_eq_kr` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`)<br>`db` (float; default `0`) | 1 channel | backend UGen must be installed |

### granular.json

Manifest: [`granular.json`](../../../crates/vibelang-dsp/ugen_manifests/granular.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `TGrains` | `t_grains_ar` | 0..9 positional | `numChannels` (float; default `2`)<br>`trigger` (signal; default `0`)<br>`bufnum` (float; default `0`)<br>`rate` (float; default `1`)<br>`centerPos` (float; default `0`)<br>`dur` (float; default `0.1`)<br>`pan` (float; default `0`)<br>`amp` (float; default `0.1`)<br>`interp` (float; default `4`) | channels from `numChannels` (manifest default 2) | backend UGen must be installed |
| `GrainBuf` | `grain_buf_ar` | 0..10 positional | `numChannels` (float; default `2`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`) | channels from `numChannels` (manifest default 2) | backend UGen must be installed |
| `GrainSin` | `grain_sin_ar` | 0..7 positional | `numChannels` (float; default `2`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`) | channels from `numChannels` (manifest default 2) | backend UGen must be installed |
| `GrainFM` | `grain_fm_ar` | 0..9 positional | `numChannels` (float; default `2`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`) | channels from `numChannels` (manifest default 2) | backend UGen must be installed |
| `GrainIn` | `grain_in_ar` | 0..7 positional | `numChannels` (float; default `2`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`) | channels from `numChannels` (manifest default 2) | backend UGen must be installed |

### info.json

Manifest: [`info.json`](../../../crates/vibelang-dsp/ugen_manifests/info.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `SampleRate` | `sample_rate_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `SampleDur` | `sample_dur_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `ControlRate` | `control_rate_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `ControlDur` | `control_dur_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `RadiansPerSample` | `radians_per_sample_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `NumOutputBuses` | `num_output_buses_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `NumInputBuses` | `num_input_buses_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `NumAudioBuses` | `num_audio_buses_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `NumControlBuses` | `num_control_buses_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `NumBuffers` | `num_buffers_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `NumRunningSynths` | `num_running_synths_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `BlockSize` | `block_size_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `NodeID` | `node_id_ir` | 0 | — | 1 channel | backend UGen must be installed |
| `SubsampleOffset` | `subsample_offset_ir` | 0 | — | 1 channel | backend UGen must be installed |

### inout.json

Manifest: [`inout.json`](../../../crates/vibelang-dsp/ugen_manifests/inout.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `In` | `in_ar`<br>`in_kr` | 0..2 positional | `bus` (float; default `0`)<br>`numChannels` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Out` | `out_ar`<br>`out_kr` | 0..2 positional | `bus` (float; default `0`)<br>`channelsArray` (signal; default `0`) | 0 channels | backend UGen must be installed |
| `ReplaceOut` | `replace_out_ar`<br>`replace_out_kr` | 0..2 positional | `bus` (float; default `0`)<br>`channelsArray` (signal; default `0`) | 0 channels | backend UGen must be installed |
| `OffsetOut` | `offset_out_ar` | 0..2 positional | `bus` (float; default `0`)<br>`channelsArray` (signal; default `0`) | 0 channels | backend UGen must be installed |
| `LocalIn` | `local_in_ar`<br>`local_in_kr` | 0..2 positional | `numChannels` (float; default `1`)<br>`default` (signal; default `0`) | channels from `numChannels` (manifest default 1); default signal Array is expanded/cycled | backend UGen must be installed |
| `LocalOut` | `local_out_ar`<br>`local_out_kr` | 0..1 positional | `channelsArray` (signal; default `0`) | 0 channels; Array input is flattened | backend UGen must be installed |
| `InFeedback` | `in_feedback_ar` | 0..2 positional | `bus` (float; default `0`)<br>`numChannels` (float; default `1`) | 1 channel | backend UGen must be installed |
| `XOut` | `x_out_ar`<br>`x_out_kr` | 0..3 positional | `bus` (float; default `0`)<br>`xfade` (float; default `0`)<br>`channelsArray` (signal; default `0`) | 0 channels | backend UGen must be installed |

### math.json

Manifest: [`math.json`](../../../crates/vibelang-dsp/ugen_manifests/math.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Clip` | `clip_ar`<br>`clip_kr`<br>`clip_ir` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (float; default `0`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Fold` | `fold_ar`<br>`fold_kr`<br>`fold_ir` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (float; default `0`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Wrap` | `wrap_ar`<br>`wrap_kr`<br>`wrap_ir` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (float; default `0`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `MulAdd` | `mul_add_ar`<br>`mul_add_kr`<br>`mul_add_ir` | 0..3 positional | `in` (signal; default `0`)<br>`mul` (float; default `1`)<br>`add` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LinExp` | `lin_exp_ar`<br>`lin_exp_kr`<br>`lin_exp_ir` | 0..5 positional | `in` (signal; default `0`)<br>`srclo` (float; default `0`)<br>`srchi` (float; default `1`)<br>`dstlo` (float; default `1`)<br>`dsthi` (float; default `2`) | 1 channel | backend UGen must be installed |
| `LinLin` | `lin_lin_ar`<br>`lin_lin_kr`<br>`lin_lin_ir` | 0..5 positional | `in` (signal; default `0`)<br>`srclo` (float; default `0`)<br>`srchi` (float; default `1`)<br>`dstlo` (float; default `1`)<br>`dsthi` (float; default `2`) | 1 channel; handwritten pseudo-lowering; registered return is Dynamic | backend UGen must be installed |
| `Sum3` | `sum3_ar`<br>`sum3_kr` | 0..3 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`)<br>`c` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Sum4` | `sum4_ar`<br>`sum4_kr` | 0..4 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`)<br>`c` (signal; default `0`)<br>`d` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Tanh` | `tanh_ar`<br>`tanh_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel; emits `UnaryOpUGen`; special index 28 | backend UGen must be installed |
| `Ring1` | `ring1_ar`<br>`ring1_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 30; manifest pseudo metadata | backend UGen must be installed |
| `Ring2` | `ring2_ar`<br>`ring2_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 31; manifest pseudo metadata | backend UGen must be installed |
| `Ring3` | `ring3_ar`<br>`ring3_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 32; manifest pseudo metadata | backend UGen must be installed |
| `Ring4` | `ring4_ar`<br>`ring4_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 33; manifest pseudo metadata | backend UGen must be installed |
| `Hypot` | `hypot_ar`<br>`hypot_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 23; manifest pseudo metadata | backend UGen must be installed |
| `HypotApx` | `hypot_apx_ar`<br>`hypot_apx_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 24; manifest pseudo metadata | backend UGen must be installed |
| `Atan2` | `atan2_ar`<br>`atan2_kr` | 0..2 positional | `y` (signal; default `0`)<br>`x` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 22; manifest pseudo metadata | backend UGen must be installed |
| `SumSqr` | `sum_sqr_ar`<br>`sum_sqr_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 35; manifest pseudo metadata | backend UGen must be installed |
| `DifSqr` | `dif_sqr_ar`<br>`dif_sqr_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 34; manifest pseudo metadata | backend UGen must be installed |
| `SqrSum` | `sqr_sum_ar`<br>`sqr_sum_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 36; manifest pseudo metadata | backend UGen must be installed |
| `SqrDif` | `sqr_dif_ar`<br>`sqr_dif_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 37; manifest pseudo metadata | backend UGen must be installed |
| `AbsDif` | `abs_dif_ar`<br>`abs_dif_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 38; manifest pseudo metadata | backend UGen must be installed |
| `Thresh` | `thresh_ar`<br>`thresh_kr` | 0..2 positional | `in` (signal; default `0`)<br>`thresh` (float; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 39; manifest pseudo metadata | backend UGen must be installed |
| `AMClip` | `am_clip_ar`<br>`am_clip_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 40; manifest pseudo metadata | backend UGen must be installed |
| `ScaleNeg` | `scale_neg_ar`<br>`scale_neg_kr` | 0..2 positional | `in` (signal; default `0`)<br>`scale` (float; default `1`) | 1 channel; emits `BinaryOpUGen`; special index 41; manifest pseudo metadata | backend UGen must be installed |
| `Clip2` | `clip2_ar`<br>`clip2_kr` | 0..2 positional | `in` (signal; default `0`)<br>`max` (float; default `1`) | 1 channel; emits `BinaryOpUGen`; special index 42; manifest pseudo metadata | backend UGen must be installed |
| `Wrap2` | `wrap2_ar`<br>`wrap2_kr` | 0..2 positional | `in` (signal; default `0`)<br>`max` (float; default `1`) | 1 channel; emits `BinaryOpUGen`; special index 45; manifest pseudo metadata | backend UGen must be installed |
| `Fold2` | `fold2_ar`<br>`fold2_kr` | 0..2 positional | `in` (signal; default `0`)<br>`max` (float; default `1`) | 1 channel; emits `BinaryOpUGen`; special index 44; manifest pseudo metadata | backend UGen must be installed |
| `Excess` | `excess_ar`<br>`excess_kr` | 0..2 positional | `in` (signal; default `0`)<br>`max` (float; default `1`) | 1 channel; emits `BinaryOpUGen`; special index 43; manifest pseudo metadata | backend UGen must be installed |
| `FirstArg` | `first_arg_ar`<br>`first_arg_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel; emits `BinaryOpUGen`; special index 46; manifest pseudo metadata | backend UGen must be installed |

### mi_ugens.json

Manifest: [`mi_ugens.json`](../../../crates/vibelang-dsp/ugen_manifests/mi_ugens.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `MiPlaits` | `mi_plaits_ar` | 0..13 positional | `pitch` (float; default `60`)<br>`engine` (int; default `0`)<br>`harm` (float; default `0.5`)<br>`timbre` (float; default `0.5`)<br>`morph` (float; default `0.5`)<br>`trigger` (signal; default `0`)<br>`level` (float; default `1`)<br>`fm_mod` (float; default `0`)<br>`timb_mod` (float; default `0`)<br>`morph_mod` (float; default `0`)<br>`decay` (float; default `0.5`)<br>`lpg_colour` (float; default `0.5`)<br>`mul` (float; default `1`) | 2 channels | requires `mi-UGens` |
| `MiBraids` | `mi_braids_ar` | 0..10 positional | `pitch` (float; default `60`)<br>`timbre` (float; default `0.5`)<br>`color` (float; default `0.5`)<br>`model` (int; default `0`)<br>`trig` (signal; default `0`)<br>`resamp` (int; default `0`)<br>`decim` (int; default `1`)<br>`bits` (int; default `0`)<br>`ws` (float; default `0`)<br>`mul` (float; default `1`) | 1 channel | requires `mi-UGens` |
| `MiRings` | `mi_rings_ar` | 0..13 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`)<br>`pit` (float; default `60`)<br>`struct` (float; default `0.25`)<br>`bright` (float; default `0.5`)<br>`damp` (float; default `0.7`)<br>`pos` (float; default `0.25`)<br>`model` (int; default `0`)<br>`poly` (int; default `1`)<br>`intern_exciter` (int; default `0`)<br>`easteregg` (int; default `0`)<br>`bypass` (int; default `0`)<br>`mul` (float; default `1`) | 2 channels | requires `mi-UGens` |
| `MiElements` | `mi_elements_ar` | 0..20 positional | `blow_in` (signal; default `0`)<br>`strike_in` (signal; default `0`)<br>`gate` (signal; default `0`)<br>`pit` (float; default `60`)<br>`strength` (float; default `0.5`)<br>`contour` (float; default `0.5`)<br>`bow_level` (float; default `0`)<br>`blow_level` (float; default `0`)<br>`strike_level` (float; default `0.5`)<br>`flow` (float; default `0.5`)<br>`mallet` (float; default `0.5`)<br>`bow_timb` (float; default `0.5`)<br>`blow_timb` (float; default `0.5`)<br>`strike_timb` (float; default `0.5`)<br>`geom` (float; default `0.5`)<br>`bright` (float; default `0.5`)<br>`damp` (float; default `0.7`)<br>`pos` (float; default `0.25`)<br>`space` (float; default `0.3`)<br>`model` (int; default `0`) | 2 channels | requires `mi-UGens` |
| `MiClouds` | `mi_clouds_ar` | 0..15 positional | `in` (signal; default `0`)<br>`pit` (float; default `0`)<br>`pos` (float; default `0`)<br>`size` (float; default `0.5`)<br>`dens` (float; default `0.5`)<br>`tex` (float; default `0.5`)<br>`drywet` (float; default `0.5`)<br>`in_gain` (float; default `1`)<br>`spread` (float; default `0`)<br>`rvb` (float; default `0`)<br>`fb` (float; default `0`)<br>`freeze` (int; default `0`)<br>`mode` (int; default `0`)<br>`lofi` (int; default `0`)<br>`trig` (signal; default `0`) | 2 channels | requires `mi-UGens` |
| `MiTides` | `mi_tides_ar` | 0..12 positional | `freq` (float; default `1`)<br>`shape` (float; default `0.5`)<br>`slope` (float; default `0.5`)<br>`smooth` (float; default `0.5`)<br>`shift` (float; default `0.5`)<br>`trig` (signal; default `0`)<br>`clock` (signal; default `0`)<br>`output_mode` (int; default `1`)<br>`ramp_mode` (int; default `1`)<br>`ratio` (int; default `9`)<br>`rate` (int; default `0`)<br>`mul` (float; default `1`) | 4 channels | requires `mi-UGens` |
| `MiWarps` | `mi_warps_ar` | 0..10 positional | `carrier` (signal; default `0`)<br>`modulator` (signal; default `0`)<br>`lev1` (float; default `0.5`)<br>`lev2` (float; default `0.5`)<br>`algo` (float; default `0`)<br>`timb` (float; default `0.5`)<br>`osc` (int; default `0`)<br>`freq` (float; default `110`)<br>`vgain` (float; default `1`)<br>`easteregg` (int; default `0`) | 2 channels | requires `mi-UGens` |
| `MiVerb` | `mi_verb_ar` | 0..8 positional | `in` (signal; default `0`)<br>`time` (float; default `0.5`)<br>`drywet` (float; default `0.5`)<br>`damp` (float; default `0.5`)<br>`hp` (float; default `0`)<br>`freeze` (int; default `0`)<br>`diff` (float; default `0.625`)<br>`mul` (float; default `1`) | 2 channels | requires `mi-UGens` |
| `MiRipples` | `mi_ripples_ar` | 0..5 positional | `in` (signal; default `0`)<br>`cf` (float; default `0.5`)<br>`reson` (float; default `0.3`)<br>`drive` (float; default `1`)<br>`mul` (float; default `1`) | 1 channel | requires `mi-UGens` |
| `MiGrids` | `mi_grids_ar` | 0..15 positional | `on_off` (int; default `1`)<br>`bpm` (float; default `120`)<br>`map_x` (float; default `0.5`)<br>`map_y` (float; default `0.5`)<br>`chaos` (float; default `0`)<br>`bd_dens` (float; default `0.5`)<br>`sd_dens` (float; default `0.5`)<br>`hh_dens` (float; default `0.5`)<br>`clock_trig` (signal; default `0`)<br>`reset_trig` (signal; default `0`)<br>`ext_clock` (int; default `0`)<br>`mode` (int; default `0`)<br>`swing` (int; default `0`)<br>`config` (int; default `0`)<br>`reso` (int; default `2`) | 6 channels | requires `mi-UGens` |
| `MiMu` | `mi_mu_ar` | 0..3 positional | `in` (signal; default `0`)<br>`gain` (float; default `1`)<br>`bypass` (int; default `0`) | 1 channel | requires `mi-UGens` |
| `MiOmi` | `mi_omi_ar` | 0..20 positional | `audio_in` (signal; default `0`)<br>`gate` (signal; default `0`)<br>`pit` (float; default `60`)<br>`contour` (float; default `0.5`)<br>`detune` (float; default `0`)<br>`level1` (float; default `1`)<br>`level2` (float; default `0.5`)<br>`ratio1` (float; default `0.5`)<br>`ratio2` (float; default `0.5`)<br>`fm1` (float; default `0.5`)<br>`fm2` (float; default `0.5`)<br>`fb` (float; default `0`)<br>`xfb` (float; default `0`)<br>`filter_mode` (float; default `0`)<br>`cutoff` (float; default `0.8`)<br>`reson` (float; default `0.3`)<br>`strength` (float; default `0.5`)<br>`env` (float; default `0.5`)<br>`rotate` (float; default `0`)<br>`space` (float; default `0.3`) | 2 channels | requires `mi-UGens` |

### multichannel.json

Manifest: [`multichannel.json`](../../../crates/vibelang-dsp/ugen_manifests/multichannel.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Mix` | `mix_ar`<br>`mix_kr` | 0..1 positional | `array` (signal; default `0`) | 1 channel; handwritten pseudo-lowering; registered return is Dynamic | backend UGen must be installed |
| `Splay` | `splay_ar`<br>`splay_kr` | 0..5 positional | `inArray` (signal; default `0`)<br>`spread` (float; default `1`)<br>`level` (float; default `1`)<br>`center` (float; default `0`)<br>`levelComp` (float; default `1`) | 2 channels; handwritten pseudo-lowering; registered return is Dynamic | backend UGen must be installed |

### noise.json

Manifest: [`noise.json`](../../../crates/vibelang-dsp/ugen_manifests/noise.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `WhiteNoise` | `white_noise_ar`<br>`white_noise_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `PinkNoise` | `pink_noise_ar`<br>`pink_noise_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `BrownNoise` | `brown_noise_ar`<br>`brown_noise_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `GrayNoise` | `gray_noise_ar`<br>`gray_noise_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `Dust` | `dust_ar`<br>`dust_kr` | 0..1 positional | `density` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LFNoise0` | `lf_noise0_ar`<br>`lf_noise0_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |
| `LFNoise1` | `lf_noise1_ar`<br>`lf_noise1_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |
| `LFNoise2` | `lf_noise2_ar`<br>`lf_noise2_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |
| `Dust2` | `dust2_ar`<br>`dust2_kr` | 0..1 positional | `density` (float; default `0`) | 1 channel | backend UGen must be installed |
| `ClipNoise` | `clip_noise_ar`<br>`clip_noise_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `Crackle` | `crackle_ar`<br>`crackle_kr` | 0..1 positional | `chaosParam` (float; default `1.5`) | 1 channel | backend UGen must be installed |
| `LFClipNoise` | `lf_clip_noise_ar`<br>`lf_clip_noise_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |
| `LFDNoise0` | `lfd_noise0_ar`<br>`lfd_noise0_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |
| `LFDNoise1` | `lfd_noise1_ar`<br>`lfd_noise1_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |
| `LFDNoise3` | `lfd_noise3_ar`<br>`lfd_noise3_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |
| `Hasher` | `hasher_ar`<br>`hasher_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `ExpRand` | `exp_rand_ir` | 0..2 positional | `lo` (float; default `0.01`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `NRand` | `n_rand_ir` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`n` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Logistic` | `logistic_ar`<br>`logistic_kr` | 0..3 positional | `chaosParam` (float; default `3`)<br>`freq` (float; default `1000`)<br>`init` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `MantissaMask` | `mantissa_mask_ar`<br>`mantissa_mask_kr` | 0..2 positional | `in` (signal; default `0`)<br>`bits` (float; default `3`) | 1 channel | backend UGen must be installed |
| `RandID` | `rand_id_kr`<br>`rand_id_ir` | 0..1 positional | `id` (float; default `0`) | 0 channels | backend UGen must be installed |
| `RandSeed` | `rand_seed_kr`<br>`rand_seed_ir` | 0..2 positional | `trig` (signal; default `0`)<br>`seed` (float; default `56789`) | 0 channels | backend UGen must be installed |
| `LFDClipNoise` | `lfd_clip_noise_ar`<br>`lfd_clip_noise_kr` | 0..1 positional | `freq` (float; default `500`) | 1 channel | backend UGen must be installed |

### oscillators.json

Manifest: [`oscillators.json`](../../../crates/vibelang-dsp/ugen_manifests/oscillators.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `SinOsc` | `sin_osc_ar`<br>`sin_osc_kr` | 0..2 positional | `freq` (float; default `440`)<br>`phase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Saw` | `saw_ar`<br>`saw_kr` | 0..1 positional | `freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `Pulse` | `pulse_ar`<br>`pulse_kr` | 0..2 positional | `freq` (float; default `440`)<br>`width` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `LFSaw` | `lf_saw_ar`<br>`lf_saw_kr` | 0..2 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LFPulse` | `lf_pulse_ar`<br>`lf_pulse_kr` | 0..3 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`)<br>`width` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `LFTri` | `lf_tri_ar`<br>`lf_tri_kr` | 0..2 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `VarSaw` | `var_saw_ar`<br>`var_saw_kr` | 0..3 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`)<br>`width` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `Blip` | `blip_ar`<br>`blip_kr` | 0..2 positional | `freq` (float; default `440`)<br>`numharm` (float; default `200`) | 1 channel | backend UGen must be installed |
| `Impulse` | `impulse_ar`<br>`impulse_kr` | 0..2 positional | `freq` (float; default `440`)<br>`phase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `SinOscFB` | `sin_osc_fb_ar`<br>`sin_osc_fb_kr` | 0..2 positional | `freq` (float; default `440`)<br>`feedback` (float; default `0`) | 1 channel | backend UGen must be installed |
| `FSinOsc` | `f_sin_osc_ar`<br>`f_sin_osc_kr` | 0..2 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LFPar` | `lf_par_ar`<br>`lf_par_kr` | 0..2 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `SyncSaw` | `sync_saw_ar`<br>`sync_saw_kr` | 0..2 positional | `syncFreq` (float; default `440`)<br>`sawFreq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `Formant` | `formant_ar` | 0..3 positional | `fundfreq` (float; default `440`)<br>`formfreq` (float; default `1760`)<br>`bwfreq` (float; default `880`) | 1 channel | backend UGen must be installed |
| `VOsc` | `v_osc_ar`<br>`v_osc_kr` | 0..3 positional | `bufpos` (float; default `0`)<br>`freq` (float; default `440`)<br>`phase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `VOsc3` | `v_osc3_ar`<br>`v_osc3_kr` | 0..4 positional | `bufpos` (float; default `0`)<br>`freq1` (float; default `110`)<br>`freq2` (float; default `220`)<br>`freq3` (float; default `440`) | 1 channel | backend UGen must be installed |
| `COsc` | `c_osc_ar`<br>`c_osc_kr` | 0..3 positional | `bufnum` (float; default `0`)<br>`freq` (float; default `440`)<br>`beats` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `Klang` | `klang_ar` | 0..3 positional | `specificationsArrayRef` (signal; default `0`)<br>`freqscale` (float; default `1`)<br>`freqoffset` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Klank` | `klank_ar` | 0..5 positional | `specificationsArrayRef` (signal; default `0`)<br>`input` (signal; default `0`)<br>`freqscale` (float; default `1`)<br>`freqoffset` (float; default `0`)<br>`decayscale` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Gendy1` | `gendy1_ar`<br>`gendy1_kr` | 0..10 positional | `ampdist` (float; default `1`)<br>`durdist` (float; default `1`)<br>`adparam` (float; default `1`)<br>`ddparam` (float; default `1`)<br>`minfreq` (float; default `440`)<br>`maxfreq` (float; default `660`)<br>`ampscale` (float; default `0.5`)<br>`durscale` (float; default `0.5`)<br>`initCPs` (float; default `12`)<br>`knum` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Gendy2` | `gendy2_ar`<br>`gendy2_kr` | 0..12 positional | `ampdist` (float; default `1`)<br>`durdist` (float; default `1`)<br>`adparam` (float; default `1`)<br>`ddparam` (float; default `1`)<br>`minfreq` (float; default `440`)<br>`maxfreq` (float; default `660`)<br>`ampscale` (float; default `0.5`)<br>`durscale` (float; default `0.5`)<br>`initCPs` (float; default `12`)<br>`knum` (float; default `1`)<br>`a` (float; default `1.17`)<br>`c` (float; default `0.31`) | 1 channel | backend UGen must be installed |
| `Gendy3` | `gendy3_ar`<br>`gendy3_kr` | 0..9 positional | `ampdist` (float; default `1`)<br>`durdist` (float; default `1`)<br>`adparam` (float; default `1`)<br>`ddparam` (float; default `1`)<br>`freq` (float; default `440`)<br>`ampscale` (float; default `0.5`)<br>`durscale` (float; default `0.5`)<br>`initCPs` (float; default `12`)<br>`knum` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Osc` | `osc_ar`<br>`osc_kr` | 0..3 positional | `bufnum` (float; default `0`)<br>`freq` (float; default `440`)<br>`phase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `OscN` | `osc_n_ar`<br>`osc_n_kr` | 0..3 positional | `bufnum` (float; default `0`)<br>`freq` (float; default `440`)<br>`phase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Shaper` | `shaper_ar`<br>`shaper_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Index` | `index_ar`<br>`index_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `IndexL` | `index_l_ar`<br>`index_l_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `IndexInBetween` | `index_in_between_ar`<br>`index_in_between_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `DegreeToKey` | `degree_to_key_ar`<br>`degree_to_key_kr` | 0..3 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`)<br>`octave` (float; default `12`) | 1 channel | backend UGen must be installed |
| `DetectIndex` | `detect_index_ar`<br>`detect_index_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `FoldIndex` | `fold_index_ar`<br>`fold_index_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `WrapIndex` | `wrap_index_ar`<br>`wrap_index_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `TWindex` | `t_windex_ar`<br>`t_windex_kr` | 0..3 positional | `in` (signal; default `0`)<br>`array` (signal; default `0`)<br>`normalize` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PSinGrain` | `p_sin_grain_ar` | 0..3 positional | `freq` (float; default `440`)<br>`dur` (float; default `0.2`)<br>`amp` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `Vibrato` | `vibrato_ar`<br>`vibrato_kr` | 0..9 positional | `freq` (float; default `440`)<br>`rate` (float; default `6`)<br>`depth` (float; default `0.02`)<br>`delay` (float; default `0`)<br>`onset` (float; default `0`)<br>`rateVariation` (float; default `0.04`)<br>`depthVariation` (float; default `0.1`)<br>`iphase` (float; default `0`)<br>`trig` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AmpComp` | `amp_comp_ar`<br>`amp_comp_kr`<br>`amp_comp_ir` | 0..3 positional | `freq` (float; default `261.6256`)<br>`root` (float; default `261.6256`)<br>`exp` (float; default `0.3333`) | 1 channel | backend UGen must be installed |
| `AmpCompA` | `amp_comp_a_ar`<br>`amp_comp_a_kr`<br>`amp_comp_a_ir` | 0..4 positional | `freq` (float; default `1000`)<br>`root` (float; default `0`)<br>`minAmp` (float; default `0.32`)<br>`rootAmp` (float; default `1`) | 1 channel | backend UGen must be installed |
| `ModDif` | `mod_dif_ar`<br>`mod_dif_kr`<br>`mod_dif_ir` | 0..3 positional | `x` (float; default `0`)<br>`y` (float; default `0`)<br>`mod` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Unwrap` | `unwrap_ar`<br>`unwrap_kr` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (float; default `0`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `LFCub` | `lf_cub_ar`<br>`lf_cub_kr` | 0..2 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`) | 1 channel | backend UGen must be installed |

### panning.json

Manifest: [`panning.json`](../../../crates/vibelang-dsp/ugen_manifests/panning.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Pan2` | `pan2_ar`<br>`pan2_kr` | 0..3 positional | `in` (signal; default `0`)<br>`pos` (float; default `0`)<br>`level` (float; default `1`) | 2 channels | backend UGen must be installed |
| `Balance2` | `balance2_ar`<br>`balance2_kr` | 0..4 positional | `left` (signal; default `0`)<br>`right` (signal; default `0`)<br>`pos` (float; default `0`)<br>`level` (float; default `1`) | 2 channels | backend UGen must be installed |
| `LinPan2` | `lin_pan2_ar`<br>`lin_pan2_kr` | 0..3 positional | `in` (signal; default `0`)<br>`pos` (float; default `0`)<br>`level` (float; default `1`) | 2 channels | backend UGen must be installed |
| `Pan4` | `pan4_ar`<br>`pan4_kr` | 0..4 positional | `in` (signal; default `0`)<br>`xpos` (float; default `0`)<br>`ypos` (float; default `0`)<br>`level` (float; default `1`) | 4 channels | backend UGen must be installed |
| `Rotate2` | `rotate2_ar`<br>`rotate2_kr` | 0..3 positional | `x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`pos` (float; default `0`) | 2 channels | backend UGen must be installed |
| `XFade2` | `x_fade2_ar`<br>`x_fade2_kr` | 0..4 positional | `inA` (signal; default `0`)<br>`inB` (signal; default `0`)<br>`pan` (float; default `0`)<br>`level` (float; default `1`) | 1 channel | backend UGen must be installed |
| `LinXFade2` | `lin_x_fade2_ar`<br>`lin_x_fade2_kr` | 0..4 positional | `inA` (signal; default `0`)<br>`inB` (signal; default `0`)<br>`pan` (float; default `0`)<br>`level` (float; default `1`) | 1 channel | backend UGen must be installed |
| `PanAz` | `pan_az_ar`<br>`pan_az_kr` | 0..6 positional | `numChans` (float; default `4`)<br>`in` (signal; default `0`)<br>`pos` (float; default `0`)<br>`level` (float; default `1`)<br>`width` (float; default `2`)<br>`orientation` (float; default `0.5`) | 4 channels | backend UGen must be installed |
| `SplayAz` | `splay_az_ar`<br>`splay_az_kr` | 0..8 positional | `numChans` (float; default `4`)<br>`inArray` (signal; default `0`)<br>`spread` (float; default `1`)<br>`level` (float; default `1`)<br>`width` (float; default `2`)<br>`center` (float; default `0`)<br>`orientation` (float; default `0.5`)<br>`levelComp` (float; default `1`) | 4 channels; handwritten pseudo-lowering; registered return is Dynamic | backend UGen must be installed |
| `PanB` | `pan_b_ar`<br>`pan_b_kr` | 0..4 positional | `in` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`gain` (float; default `1`) | 4 channels | backend UGen must be installed |
| `PanB2` | `pan_b2_ar`<br>`pan_b2_kr` | 0..3 positional | `in` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`gain` (float; default `1`) | 3 channels | backend UGen must be installed |
| `BiPanB2` | `bi_pan_b2_ar`<br>`bi_pan_b2_kr` | 0..4 positional | `inA` (signal; default `0`)<br>`inB` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`gain` (float; default `1`) | 3 channels | backend UGen must be installed |
| `DecodeB2` | `decode_b2_ar`<br>`decode_b2_kr` | 0..5 positional | `numChans` (float; default `4`)<br>`w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`orientation` (float; default `0.5`) | 2 channels | backend UGen must be installed |

### physical.json

Manifest: [`physical.json`](../../../crates/vibelang-dsp/ugen_manifests/physical.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Pluck` | `pluck_ar` | 0..6 positional | `in` (signal; default `0`)<br>`trig` (signal; default `1`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`)<br>`coef` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `Spring` | `spring_ar` | 0..3 positional | `in` (signal; default `0`)<br>`spring` (float; default `1`)<br>`damp` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Ball` | `ball_ar`<br>`ball_kr` | 0..4 positional | `in` (signal; default `0`)<br>`g` (float; default `1`)<br>`damp` (float; default `0`)<br>`friction` (float; default `0.01`) | 1 channel | backend UGen must be installed |
| `TBall` | `t_ball_ar`<br>`t_ball_kr` | 0..4 positional | `in` (signal; default `0`)<br>`g` (float; default `10`)<br>`damp` (float; default `0`)<br>`friction` (float; default `0.01`) | 1 channel | backend UGen must be installed |

### pitchtime.json

Manifest: [`pitchtime.json`](../../../crates/vibelang-dsp/ugen_manifests/pitchtime.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `PitchShift` | `pitch_shift_ar` | 0..5 positional | `in` (signal; default `0`)<br>`windowSize` (float; default `0.2`)<br>`pitchRatio` (float; default `1`)<br>`pitchDispersion` (float; default `0`)<br>`timeDispersion` (float; default `0`) | 1 channel | backend UGen must be installed |

### pv_spectral.json

Manifest: [`pv_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/pv_spectral.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `PV_Add` | `pv_add_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Copy` | `pv_copy_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_CopyPhase` | `pv_copy_phase_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Conj` | `pv_conj_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Div` | `pv_div_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Max` | `pv_max_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Min` | `pv_min_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Mul` | `pv_mul_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagAbove` | `pv_mag_above_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagBelow` | `pv_mag_below_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagClip` | `pv_mag_clip_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagDiv` | `pv_mag_div_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`zeroed` (float; default `0.0001`) | 1 channel | backend UGen must be installed |
| `PV_MagMul` | `pv_mag_mul_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagSquared` | `pv_mag_squared_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagFreeze` | `pv_mag_freeze_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`freeze` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagShift` | `pv_mag_shift_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`stretch` (float; default `1`)<br>`shift` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagSmear` | `pv_mag_smear_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`bins` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagNoise` | `pv_mag_noise_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_BinScramble` | `pv_bin_scramble_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`wipe` (float; default `0`)<br>`width` (float; default `0.2`)<br>`trig` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_BinShift` | `pv_bin_shift_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`stretch` (float; default `1`)<br>`shift` (float; default `0`)<br>`interp` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_BinWipe` | `pv_bin_wipe_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`wipe` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_BrickWall` | `pv_brick_wall_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`wipe` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_LocalMax` | `pv_local_max_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Diffuser` | `pv_diffuser_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`trig` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_RandComb` | `pv_rand_comb_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`wipe` (float; default `0`)<br>`trig` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_RandWipe` | `pv_rand_wipe_kr` | 0..4 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`wipe` (float; default `0`)<br>`trig` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_RectComb` | `pv_rect_comb_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`numTeeth` (float; default `0`)<br>`phase` (float; default `0`)<br>`width` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `PV_RectComb2` | `pv_rect_comb2_kr` | 0..5 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`numTeeth` (float; default `0`)<br>`phase` (float; default `0`)<br>`width` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `PV_PhaseShift` | `pv_phase_shift_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`shift` (float; default `0`)<br>`integrate` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_PhaseShift90` | `pv_phase_shift90_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_PhaseShift270` | `pv_phase_shift270_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_ConformalMap` | `pv_conformal_map_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`areal` (float; default `0`)<br>`aimag` (float; default `0`) | 1 channel | backend UGen must be installed |

### random.json

Manifest: [`random.json`](../../../crates/vibelang-dsp/ugen_manifests/random.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Rand` | `rand_ir` | 0..2 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `IRand` | `i_rand_ir` | 0..2 positional | `lo` (float; default `0`)<br>`hi` (float; default `127`) | 1 channel | backend UGen must be installed |
| `TRand` | `t_rand_ar`<br>`t_rand_kr` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `TIRand` | `ti_rand_ar`<br>`ti_rand_kr` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `127`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `TExpRand` | `t_exp_rand_ar`<br>`t_exp_rand_kr` | 0..3 positional | `lo` (float; default `0.01`)<br>`hi` (float; default `1`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `LinRand` | `lin_rand_ir` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`minmax` (float; default `0`) | 1 channel | backend UGen must be installed |
| `CoinGate` | `coin_gate_ar`<br>`coin_gate_kr` | 0..2 positional | `prob` (float; default `0.5`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |

### reverb.json

Manifest: [`reverb.json`](../../../crates/vibelang-dsp/ugen_manifests/reverb.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `FreeVerb` | `free_verb_ar` | 0..4 positional | `in` (signal; default `0`)<br>`mix` (float; default `0.33`)<br>`room` (float; default `0.5`)<br>`damp` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `GVerb` | `g_verb_ar` | 0..10 positional | `in` (signal; default `0`)<br>`roomsize` (float; default `10`)<br>`revtime` (float; default `3`)<br>`damping` (float; default `0.5`)<br>`inputbw` (float; default `0.5`)<br>`spread` (float; default `15`)<br>`drylevel` (float; default `1`)<br>`earlyreflevel` (float; default `0.7`)<br>`taillevel` (float; default `0.5`)<br>`maxroomsize` (float; default `300`) | 2 channels | backend UGen must be installed |
| `FreeVerb2` | `free_verb2_ar` | 0..5 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`mix` (float; default `0.33`)<br>`room` (float; default `0.5`)<br>`damp` (float; default `0.5`) | 2 channels | backend UGen must be installed |

### sc3_aa_oscillators.json

Manifest: [`sc3_aa_oscillators.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_aa_oscillators.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `BlitB3` | `blit_b3_ar` | 0..1 positional | `freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `BlitB3Saw` | `blit_b3_saw_ar` | 0..2 positional | `freq` (float; default `440`)<br>`leak` (float; default `0.99`) | 1 channel | backend UGen must be installed |
| `BlitB3Square` | `blit_b3_square_ar` | 0..2 positional | `freq` (float; default `440`)<br>`leak` (float; default `0.99`) | 1 channel | backend UGen must be installed |
| `BlitB3Tri` | `blit_b3_tri_ar` | 0..3 positional | `freq` (float; default `440`)<br>`leak` (float; default `0.99`)<br>`leak2` (float; default `0.99`) | 1 channel | backend UGen must be installed |
| `DPW3Tri` | `dpw3_tri_ar` | 0..1 positional | `freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `DPW4Saw` | `dpw4_saw_ar` | 0..1 positional | `freq` (float; default `440`) | 1 channel | backend UGen must be installed |

### sc3_auditory.json

Manifest: [`sc3_auditory.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_auditory.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Gammatone` | `gammatone_ar` | 0..3 positional | `input` (signal; default `0`)<br>`centrefrequency` (float; default `440`)<br>`bandwidth` (float; default `200`) | 1 channel | backend UGen must be installed |
| `HairCell` | `hair_cell_ar`<br>`hair_cell_kr` | 0..5 positional | `input` (signal; default `0`)<br>`spontaneousrate` (float; default `0`)<br>`boostrate` (float; default `200`)<br>`restorerate` (float; default `1000`)<br>`loss` (float; default `0.99`) | 1 channel | backend UGen must be installed |
| `Meddis` | `meddis_ar`<br>`meddis_kr` | 0..1 positional | `input` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_ay.json

Manifest: [`sc3_ay.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_ay.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `AY` | `ay_ar` | 0..11 positional | `tonea` (int; default `1777`)<br>`toneb` (int; default `1666`)<br>`tonec` (int; default `1555`)<br>`noise` (int; default `1`)<br>`control` (int; default `7`)<br>`vola` (int; default `15`)<br>`volb` (int; default `15`)<br>`volc` (int; default `15`)<br>`envfreq` (int; default `4`)<br>`envstyle` (int; default `1`)<br>`chiptype` (int; default `0`) | 1 channel | backend UGen must be installed |

### sc3_bat.json

Manifest: [`sc3_bat.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_bat.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Coyote` | `coyote_kr` | 0..7 positional | `in` (signal; default `0`)<br>`trackFall` (signal; default `0.2`)<br>`slowLag` (signal; default `0.2`)<br>`fastLag` (signal; default `0.01`)<br>`fastMul` (signal; default `0.5`)<br>`thresh` (signal; default `0.05`)<br>`minDur` (signal; default `0.1`) | 1 channel | backend UGen must be installed |
| `TrigAvg` | `trig_avg_kr` | 0..2 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `WAmp` | `w_amp_kr` | 0..2 positional | `in` (signal; default `0`)<br>`winSize` (signal; default `0.1`) | 1 channel | backend UGen must be installed |
| `MarkovSynth` | `markov_synth_ar` | 0..4 positional | `in` (signal; default `0`)<br>`isRecording` (signal; default `1`)<br>`waitTime` (signal; default `2`)<br>`tableSize` (signal; default `10`) | 1 channel | backend UGen must be installed |
| `FrameCompare` | `frame_compare_kr` | 0..3 positional | `buffer1` (signal; default `0`)<br>`buffer2` (signal; default `0`)<br>`wAmount` (signal; default `0.5`) | 1 channel | backend UGen must be installed |
| `NeedleRect` | `needle_rect_ar` | 0..7 positional | `rate` (signal; default `1`)<br>`imgWidth` (signal; default `100`)<br>`imgHeight` (signal; default `100`)<br>`rectX` (signal; default `0`)<br>`rectY` (signal; default `0`)<br>`rectW` (signal; default `100`)<br>`rectH` (signal; default `100`) | 1 channel | backend UGen must be installed |
| `SkipNeedle` | `skip_needle_ar` | 0..3 positional | `range` (signal; default `44100`)<br>`rate` (signal; default `10`)<br>`offset` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_bbcut2.json

Manifest: [`sc3_bbcut2.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_bbcut2.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `AnalyseEvents2` | `analyse_events2_ar` | 0..6 positional | `in` (signal; default `0`)<br>`bufnum` (int; default `0`)<br>`threshold` (float; default `0.34`)<br>`triggerid` (int; default `101`)<br>`circular` (int; default `0`)<br>`pitch` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `DrumTrack` | `drum_track_kr` | 0..11 positional | `in` (signal; default `0`)<br>`lock` (float; default `0`)<br>`dynleak` (float; default `0`)<br>`tempowt` (float; default `0`)<br>`phasewt` (float; default `0`)<br>`basswt` (float; default `0`)<br>`patternwt` (float; default `1`)<br>`prior` (int; default `-10`)<br>`kicksensitivity` (float; default `1`)<br>`snaresensitivity` (float; default `1`)<br>`debugmode` (int; default `0`) | 4 channels | backend UGen must be installed |

### sc3_berlach.json

Manifest: [`sc3_berlach.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_berlach.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `LPF1` | `lpf1_ar`<br>`lpf1_kr` | 0..2 positional | `in` (signal; default `0`)<br>`freq` (signal; default `1000`) | 1 channel | backend UGen must be installed |
| `LPF18` | `lpf18_ar` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (signal; default `100`)<br>`res` (signal; default `1`)<br>`dist` (signal; default `0.4`) | 1 channel | backend UGen must be installed |
| `LPFVS6` | `lpfvs6_ar`<br>`lpfvs6_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (signal; default `1000`)<br>`slope` (signal; default `0.5`) | 1 channel | backend UGen must be installed |
| `BLBufRd` | `bl_buf_rd_ar`<br>`bl_buf_rd_kr` | 0..3 positional | `bufnum` (signal; default `0`)<br>`phase` (signal; default `0`)<br>`ratio` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `Clipper4` | `clipper4_ar` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (signal; default `-0.8`)<br>`hi` (signal; default `0.8`) | 1 channel | backend UGen must be installed |
| `Clipper8` | `clipper8_ar` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (signal; default `-0.8`)<br>`hi` (signal; default `0.8`) | 1 channel | backend UGen must be installed |
| `SoftClipper4` | `soft_clipper4_ar` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `SoftClipper8` | `soft_clipper8_ar` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `SoftClipAmp4` | `soft_clip_amp4_ar` | 0..2 positional | `in` (signal; default `0`)<br>`pregain` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `SoftClipAmp8` | `soft_clip_amp8_ar` | 0..2 positional | `in` (signal; default `0`)<br>`pregain` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `OSWrap4` | `os_wrap4_ar` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (signal; default `-1`)<br>`hi` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `OSWrap8` | `os_wrap8_ar` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (signal; default `-1`)<br>`hi` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `OSFold4` | `os_fold4_ar` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (signal; default `-1`)<br>`hi` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `OSFold8` | `os_fold8_ar` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (signal; default `-1`)<br>`hi` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `OSTrunc4` | `os_trunc4_ar` | 0..2 positional | `in` (signal; default `0`)<br>`quant` (signal; default `0.5`) | 1 channel | backend UGen must be installed |
| `OSTrunc8` | `os_trunc8_ar` | 0..2 positional | `in` (signal; default `0`)<br>`quant` (signal; default `0.5`) | 1 channel | backend UGen must be installed |
| `DriveNoise` | `drive_noise_ar` | 0..3 positional | `in` (signal; default `0`)<br>`amount` (signal; default `1`)<br>`multi` (signal; default `5`) | 1 channel | backend UGen must be installed |
| `PeakEQ4` | `peak_eq4_ar` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (signal; default `1200`)<br>`rs` (signal; default `1`)<br>`db` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PeakEQ2` | `peak_eq2_ar` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (signal; default `1200`)<br>`rs` (signal; default `1`)<br>`db` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_betablocker.json

Manifest: [`sc3_betablocker.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_betablocker.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `BBlockerBuf` | `b_blocker_buf_ar` | 0..3 positional | `freq` (signal; default `440`)<br>`bufnum` (signal; default `0`)<br>`startpoint` (signal; default `0`) | 9 channels | backend UGen must be installed |
| `DetaBlockerBuf` | `deta_blocker_buf_demand` | 0..2 positional | `bufnum` (signal; default `0`)<br>`startpoint` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_bhob.json

Manifest: [`sc3_bhob.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_bhob.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `MoogLadder` | `moog_ladder_ar`<br>`moog_ladder_kr` | 0..3 positional | `in` (signal; default `0`)<br>`ffreq` (float; default `440`)<br>`res` (float; default `0`) | 1 channel | backend UGen must be installed |
| `RLPFD` | `rlpfd_ar`<br>`rlpfd_kr` | 0..4 positional | `in` (signal; default `0`)<br>`ffreq` (float; default `440`)<br>`res` (float; default `0`)<br>`dist` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Streson` | `streson_ar`<br>`streson_kr` | 0..3 positional | `input` (signal; default `0`)<br>`delayTime` (float; default `0.003`)<br>`res` (float; default `0.9`) | 1 channel | backend UGen must be installed |
| `NLFiltN` | `nl_filt_n_ar`<br>`nl_filt_n_kr` | 0..6 positional | `input` (signal; default `0`)<br>`a` (float; default `0`)<br>`b` (float; default `0`)<br>`d` (float; default `0`)<br>`c` (float; default `0`)<br>`l` (float; default `0`) | 1 channel | backend UGen must be installed |
| `NLFiltL` | `nl_filt_l_ar`<br>`nl_filt_l_kr` | 0..6 positional | `input` (signal; default `0`)<br>`a` (float; default `0`)<br>`b` (float; default `0`)<br>`d` (float; default `0`)<br>`c` (float; default `0`)<br>`l` (float; default `0`) | 1 channel | backend UGen must be installed |
| `NLFiltC` | `nl_filt_c_ar`<br>`nl_filt_c_kr` | 0..6 positional | `input` (signal; default `0`)<br>`a` (float; default `0`)<br>`b` (float; default `0`)<br>`d` (float; default `0`)<br>`c` (float; default `0`)<br>`l` (float; default `0`) | 1 channel | backend UGen must be installed |
| `TGrains2` | `t_grains2_ar` | 0..11 positional | `numChannels` (float; default `2`)<br>`trigger` (signal; default `0`)<br>`bufnum` (float; default `0`)<br>`rate` (float; default `1`)<br>`centerPos` (float; default `0`)<br>`dur` (float; default `0.1`)<br>`pan` (float; default `0`)<br>`amp` (float; default `0.1`)<br>`att` (float; default `0.5`)<br>`dec` (float; default `0.5`)<br>`interp` (float; default `4`) | 1 channel | backend UGen must be installed |
| `TGrains3` | `t_grains3_ar` | 0..12 positional | `numChannels` (float; default `2`)<br>`trigger` (signal; default `0`)<br>`bufnum` (float; default `0`)<br>`rate` (float; default `1`)<br>`centerPos` (float; default `0`)<br>`dur` (float; default `0.1`)<br>`pan` (float; default `0`)<br>`amp` (float; default `0.1`)<br>`att` (float; default `0.5`)<br>`dec` (float; default `0.5`)<br>`window` (float; default `1`)<br>`interp` (float; default `4`) | 1 channel | backend UGen must be installed |
| `Dbrown2` | `dbrown2_demand` | 0..5 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`step` (float; default `0.01`)<br>`dist` (float; default `0`)<br>`length` (float; default `1000000000`) | 1 channel | backend UGen must be installed |
| `Dgauss` | `dgauss_demand` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`length` (float; default `1000000000`) | 1 channel | backend UGen must be installed |
| `GaussTrig` | `gauss_trig_ar`<br>`gauss_trig_kr` | 0..2 positional | `freq` (float; default `440`)<br>`dev` (float; default `0.3`) | 1 channel | backend UGen must be installed |
| `TBetaRand` | `t_beta_rand_ar`<br>`t_beta_rand_kr` | 0..5 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`prob1` (float; default `1`)<br>`prob2` (float; default `1`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `TBrownRand` | `t_brown_rand_ar`<br>`t_brown_rand_kr` | 0..5 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`dev` (float; default `1`)<br>`dist` (float; default `0`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `TGaussRand` | `t_gauss_rand_ar`<br>`t_gauss_rand_kr` | 0..3 positional | `lo` (float; default `0`)<br>`hi` (float; default `1`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Gendy4` | `gendy4_ar`<br>`gendy4_kr` | 0..10 positional | `ampdist` (float; default `1`)<br>`durdist` (float; default `1`)<br>`adparam` (float; default `1`)<br>`ddparam` (float; default `1`)<br>`minfreq` (float; default `440`)<br>`maxfreq` (float; default `660`)<br>`ampscale` (float; default `0.5`)<br>`durscale` (float; default `0.5`)<br>`initCPs` (float; default `12`)<br>`knum` (float; default `12`) | 1 channel | backend UGen must be installed |
| `Gendy5` | `gendy5_ar`<br>`gendy5_kr` | 0..10 positional | `ampdist` (float; default `1`)<br>`durdist` (float; default `1`)<br>`adparam` (float; default `1`)<br>`ddparam` (float; default `1`)<br>`minfreq` (float; default `440`)<br>`maxfreq` (float; default `660`)<br>`ampscale` (float; default `0.5`)<br>`durscale` (float; default `0.5`)<br>`initCPs` (float; default `12`)<br>`knum` (float; default `12`) | 1 channel | backend UGen must be installed |
| `Henon2DN` | `henon2dn_ar`<br>`henon2dn_kr` | 0..6 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`a` (float; default `1.4`)<br>`b` (float; default `0.3`)<br>`x0` (float; default `0.30501993062401`)<br>`y0` (float; default `0.20938865431933`) | 1 channel | backend UGen must be installed |
| `Henon2DL` | `henon2dl_ar`<br>`henon2dl_kr` | 0..6 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`a` (float; default `1.4`)<br>`b` (float; default `0.3`)<br>`x0` (float; default `0.30501993062401`)<br>`y0` (float; default `0.20938865431933`) | 1 channel | backend UGen must be installed |
| `Henon2DC` | `henon2dc_ar`<br>`henon2dc_kr` | 0..6 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`a` (float; default `1.4`)<br>`b` (float; default `0.3`)<br>`x0` (float; default `0.30501993062401`)<br>`y0` (float; default `0.20938865431933`) | 1 channel | backend UGen must be installed |
| `HenonTrig` | `henon_trig_ar`<br>`henon_trig_kr` | 0..6 positional | `minfreq` (float; default `5`)<br>`maxfreq` (float; default `10`)<br>`a` (float; default `1.4`)<br>`b` (float; default `0.3`)<br>`x0` (float; default `0.30501993062401`)<br>`y0` (float; default `0.20938865431933`) | 1 channel | backend UGen must be installed |
| `Gbman2DN` | `gbman2dn_ar`<br>`gbman2dn_kr` | 0..4 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`x0` (float; default `1.2`)<br>`y0` (float; default `2.1`) | 1 channel | backend UGen must be installed |
| `Gbman2DL` | `gbman2dl_ar`<br>`gbman2dl_kr` | 0..4 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`x0` (float; default `1.2`)<br>`y0` (float; default `2.1`) | 1 channel | backend UGen must be installed |
| `Gbman2DC` | `gbman2dc_ar`<br>`gbman2dc_kr` | 0..4 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`x0` (float; default `1.2`)<br>`y0` (float; default `2.1`) | 1 channel | backend UGen must be installed |
| `GbmanTrig` | `gbman_trig_ar`<br>`gbman_trig_kr` | 0..4 positional | `minfreq` (float; default `5`)<br>`maxfreq` (float; default `10`)<br>`x0` (float; default `1.2`)<br>`y0` (float; default `2.1`) | 1 channel | backend UGen must be installed |
| `Standard2DN` | `standard2dn_ar`<br>`standard2dn_kr` | 0..5 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`k` (float; default `1.4`)<br>`x0` (float; default `4.9789799812499`)<br>`y0` (float; default `5.7473416156381`) | 1 channel | backend UGen must be installed |
| `Standard2DL` | `standard2dl_ar`<br>`standard2dl_kr` | 0..5 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`k` (float; default `1.4`)<br>`x0` (float; default `4.9789799812499`)<br>`y0` (float; default `5.7473416156381`) | 1 channel | backend UGen must be installed |
| `Standard2DC` | `standard2dc_ar`<br>`standard2dc_kr` | 0..5 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`k` (float; default `1.4`)<br>`x0` (float; default `4.9789799812499`)<br>`y0` (float; default `5.7473416156381`) | 1 channel | backend UGen must be installed |
| `StandardTrig` | `standard_trig_ar`<br>`standard_trig_kr` | 0..5 positional | `minfreq` (float; default `5`)<br>`maxfreq` (float; default `10`)<br>`k` (float; default `1.4`)<br>`x0` (float; default `4.9789799812499`)<br>`y0` (float; default `5.7473416156381`) | 1 channel | backend UGen must be installed |
| `Latoocarfian2DN` | `latoocarfian2dn_ar`<br>`latoocarfian2dn_kr` | 0..8 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `3`)<br>`c` (float; default `0.5`)<br>`d` (float; default `0.5`)<br>`x0` (float; default `0.34082301375036`)<br>`y0` (float; default `-0.38270086971332`) | 1 channel | backend UGen must be installed |
| `Latoocarfian2DL` | `latoocarfian2dl_ar`<br>`latoocarfian2dl_kr` | 0..8 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `3`)<br>`c` (float; default `0.5`)<br>`d` (float; default `0.5`)<br>`x0` (float; default `0.34082301375036`)<br>`y0` (float; default `-0.38270086971332`) | 1 channel | backend UGen must be installed |
| `Latoocarfian2DC` | `latoocarfian2dc_ar`<br>`latoocarfian2dc_kr` | 0..8 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`a` (float; default `1`)<br>`b` (float; default `3`)<br>`c` (float; default `0.5`)<br>`d` (float; default `0.5`)<br>`x0` (float; default `0.34082301375036`)<br>`y0` (float; default `-0.38270086971332`) | 1 channel | backend UGen must be installed |
| `LatoocarfianTrig` | `latoocarfian_trig_ar`<br>`latoocarfian_trig_kr` | 0..8 positional | `minfreq` (float; default `5`)<br>`maxfreq` (float; default `10`)<br>`a` (float; default `1`)<br>`b` (float; default `3`)<br>`c` (float; default `0.5`)<br>`d` (float; default `0.5`)<br>`x0` (float; default `0.34082301375036`)<br>`y0` (float; default `-0.38270086971332`) | 1 channel | backend UGen must be installed |
| `Lorenz2DN` | `lorenz2dn_ar`<br>`lorenz2dn_kr` | 0..9 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`s` (float; default `10`)<br>`r` (float; default `28`)<br>`b` (float; default `2.6666667`)<br>`h` (float; default `0.02`)<br>`x0` (float; default `0.090879182417163`)<br>`y0` (float; default `2.97077458055`)<br>`z0` (float; default `24.282041054363`) | 1 channel | backend UGen must be installed |
| `Lorenz2DL` | `lorenz2dl_ar`<br>`lorenz2dl_kr` | 0..9 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`s` (float; default `10`)<br>`r` (float; default `28`)<br>`b` (float; default `2.6666667`)<br>`h` (float; default `0.02`)<br>`x0` (float; default `0.090879182417163`)<br>`y0` (float; default `2.97077458055`)<br>`z0` (float; default `24.282041054363`) | 1 channel | backend UGen must be installed |
| `Lorenz2DC` | `lorenz2dc_ar`<br>`lorenz2dc_kr` | 0..9 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`s` (float; default `10`)<br>`r` (float; default `28`)<br>`b` (float; default `2.6666667`)<br>`h` (float; default `0.02`)<br>`x0` (float; default `0.090879182417163`)<br>`y0` (float; default `2.97077458055`)<br>`z0` (float; default `24.282041054363`) | 1 channel | backend UGen must be installed |
| `LorenzTrig` | `lorenz_trig_ar`<br>`lorenz_trig_kr` | 0..9 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`s` (float; default `10`)<br>`r` (float; default `28`)<br>`b` (float; default `2.6666667`)<br>`h` (float; default `0.02`)<br>`x0` (float; default `0.090879182417163`)<br>`y0` (float; default `2.97077458055`)<br>`z0` (float; default `24.282041054363`) | 1 channel | backend UGen must be installed |
| `Fhn2DN` | `fhn2dn_ar`<br>`fhn2dn_kr` | 0..9 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`urate` (float; default `0.1`)<br>`wrate` (float; default `0.1`)<br>`b0` (float; default `0.6`)<br>`b1` (float; default `0.8`)<br>`i` (float; default `0`)<br>`u0` (float; default `0`)<br>`w0` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Fhn2DL` | `fhn2dl_ar`<br>`fhn2dl_kr` | 0..9 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`urate` (float; default `0.1`)<br>`wrate` (float; default `0.1`)<br>`b0` (float; default `0.6`)<br>`b1` (float; default `0.8`)<br>`i` (float; default `0`)<br>`u0` (float; default `0`)<br>`w0` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Fhn2DC` | `fhn2dc_ar`<br>`fhn2dc_kr` | 0..9 positional | `minfreq` (float; default `11025`)<br>`maxfreq` (float; default `22050`)<br>`urate` (float; default `0.1`)<br>`wrate` (float; default `0.1`)<br>`b0` (float; default `0.6`)<br>`b1` (float; default `0.8`)<br>`i` (float; default `0`)<br>`u0` (float; default `0`)<br>`w0` (float; default `0`) | 1 channel | backend UGen must be installed |
| `FhnTrig` | `fhn_trig_ar`<br>`fhn_trig_kr` | 0..9 positional | `minfreq` (float; default `4`)<br>`maxfreq` (float; default `10`)<br>`urate` (float; default `0.1`)<br>`wrate` (float; default `0.1`)<br>`b0` (float; default `0.6`)<br>`b1` (float; default `0.8`)<br>`i` (float; default `0`)<br>`u0` (float; default `0`)<br>`w0` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LFBrownNoise0` | `lf_brown_noise0_ar`<br>`lf_brown_noise0_kr` | 0..3 positional | `freq` (float; default `20`)<br>`dev` (float; default `1`)<br>`dist` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LFBrownNoise1` | `lf_brown_noise1_ar`<br>`lf_brown_noise1_kr` | 0..3 positional | `freq` (float; default `20`)<br>`dev` (float; default `1`)<br>`dist` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LFBrownNoise2` | `lf_brown_noise2_ar`<br>`lf_brown_noise2_kr` | 0..3 positional | `freq` (float; default `20`)<br>`dev` (float; default `1`)<br>`dist` (float; default `0`) | 1 channel | backend UGen must be installed |
| `NestedAllpassN` | `nested_allpass_n_ar` | 0..7 positional | `in` (signal; default `0`)<br>`maxdelay1` (float; default `0.036`)<br>`delay1` (float; default `0.036`)<br>`gain1` (float; default `0.08`)<br>`maxdelay2` (float; default `0.03`)<br>`delay2` (float; default `0.03`)<br>`gain2` (float; default `0.3`) | 1 channel | backend UGen must be installed |
| `NestedAllpassL` | `nested_allpass_l_ar` | 0..7 positional | `in` (signal; default `0`)<br>`maxdelay1` (float; default `0.036`)<br>`delay1` (float; default `0.036`)<br>`gain1` (float; default `0.08`)<br>`maxdelay2` (float; default `0.03`)<br>`delay2` (float; default `0.03`)<br>`gain2` (float; default `0.3`) | 1 channel | backend UGen must be installed |
| `NestedAllpassC` | `nested_allpass_c_ar` | 0..7 positional | `in` (signal; default `0`)<br>`maxdelay1` (float; default `0.036`)<br>`delay1` (float; default `0.036`)<br>`gain1` (float; default `0.08`)<br>`maxdelay2` (float; default `0.03`)<br>`delay2` (float; default `0.03`)<br>`gain2` (float; default `0.3`) | 1 channel | backend UGen must be installed |
| `DoubleNestedAllpassN` | `double_nested_allpass_n_ar` | 0..10 positional | `in` (signal; default `0`)<br>`maxdelay1` (float; default `0.0047`)<br>`delay1` (float; default `0.0047`)<br>`gain1` (float; default `0.15`)<br>`maxdelay2` (float; default `0.022`)<br>`delay2` (float; default `0.022`)<br>`gain2` (float; default `0.25`)<br>`maxdelay3` (float; default `0.0083`)<br>`delay3` (float; default `0.0083`)<br>`gain3` (float; default `0.3`) | 1 channel | backend UGen must be installed |
| `DoubleNestedAllpassL` | `double_nested_allpass_l_ar` | 0..10 positional | `in` (signal; default `0`)<br>`maxdelay1` (float; default `0.0047`)<br>`delay1` (float; default `0.0047`)<br>`gain1` (float; default `0.15`)<br>`maxdelay2` (float; default `0.022`)<br>`delay2` (float; default `0.022`)<br>`gain2` (float; default `0.25`)<br>`maxdelay3` (float; default `0.0083`)<br>`delay3` (float; default `0.0083`)<br>`gain3` (float; default `0.3`) | 1 channel | backend UGen must be installed |
| `DoubleNestedAllpassC` | `double_nested_allpass_c_ar` | 0..10 positional | `in` (signal; default `0`)<br>`maxdelay1` (float; default `0.0047`)<br>`delay1` (float; default `0.0047`)<br>`gain1` (float; default `0.15`)<br>`maxdelay2` (float; default `0.022`)<br>`delay2` (float; default `0.022`)<br>`gain2` (float; default `0.25`)<br>`maxdelay3` (float; default `0.0083`)<br>`delay3` (float; default `0.0083`)<br>`gain3` (float; default `0.3`) | 1 channel | backend UGen must be installed |
| `PV_CommonMag` | `pv_common_mag_kr` | 0..4 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`tolerance` (float; default `0`)<br>`remove` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_CommonMul` | `pv_common_mul_kr` | 0..4 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`tolerance` (float; default `0`)<br>`remove` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Compander` | `pv_compander_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`thresh` (float; default `50`)<br>`slopeBelow` (float; default `1`)<br>`slopeAbove` (float; default `1`) | 1 channel | backend UGen must be installed |
| `PV_Cutoff` | `pv_cutoff_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`wipe` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagGate` | `pv_mag_gate_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`thresh` (float; default `1`)<br>`remove` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagMinus` | `pv_mag_minus_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`remove` (float; default `1`) | 1 channel | backend UGen must be installed |
| `PV_MagScale` | `pv_mag_scale_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Morph` | `pv_morph_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`morph` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_SoftWipe` | `pv_soft_wipe_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`wipe` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_XFade` | `pv_x_fade_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`fade` (float; default `0`) | 1 channel | backend UGen must be installed |

### sc3_blackrain.json

Manifest: [`sc3_blackrain.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_blackrain.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `AmplitudeMod` | `amplitude_mod_ar`<br>`amplitude_mod_kr` | 0..3 positional | `in` (signal; default `0`)<br>`attackTime` (signal; default `0.01`)<br>`releaseTime` (signal; default `0.01`) | 1 channel | backend UGen must be installed |
| `BMoog` | `b_moog_ar` | 0..5 positional | `in` (signal; default `0`)<br>`freq` (signal; default `440`)<br>`q` (signal; default `0.2`)<br>`mode` (signal; default `0`)<br>`saturation` (signal; default `0.95`) | 1 channel | backend UGen must be installed |
| `IIRFilter` | `iir_filter_ar` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (signal; default `440`)<br>`rq` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `SVF` | `svf_ar`<br>`svf_kr` | 0..8 positional | `signal` (signal; default `0`)<br>`cutoff` (signal; default `2200`)<br>`res` (signal; default `0.1`)<br>`lowpass` (signal; default `1`)<br>`bandpass` (signal; default `0`)<br>`highpass` (signal; default `0`)<br>`notch` (signal; default `0`)<br>`peak` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_chaos.json

Manifest: [`sc3_chaos.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_chaos.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `LotkaVolterra` | `lotka_volterra_ar` | 0..8 positional | `freq` (float; default `22050`)<br>`a` (float; default `1.5`)<br>`b` (float; default `1.5`)<br>`c` (float; default `0.5`)<br>`d` (float; default `1.5`)<br>`h` (float; default `0.05`)<br>`xi` (float; default `1`)<br>`yi` (float; default `0.2`) | 2 channels | backend UGen must be installed |
| `ArneodoCoulletTresser` | `arneodo_coullet_tresser_ar` | 0..6 positional | `freq` (float; default `22050`)<br>`alpha` (float; default `1.5`)<br>`h` (float; default `0.05`)<br>`xi` (float; default `0.5`)<br>`yi` (float; default `0.5`)<br>`zi` (float; default `0.5`) | 3 channels | backend UGen must be installed |
| `DNoiseRing` | `d_noise_ring_demand` | 0..5 positional | `change` (signal; default `0.5`)<br>`chance` (signal; default `0.5`)<br>`shift` (signal; default `1`)<br>`numBits` (signal; default `8`)<br>`resetval` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_concat.json

Manifest: [`sc3_concat.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_concat.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Concat` | `concat_ar` | 0..12 positional | `control` (signal; default `0`)<br>`source` (signal; default `0`)<br>`storesize` (float; default `1`)<br>`seektime` (float; default `1`)<br>`seekdur` (float; default `1`)<br>`matchlength` (float; default `0.05`)<br>`freezestore` (float; default `0`)<br>`zcr` (float; default `1`)<br>`lms` (float; default `1`)<br>`sc` (float; default `1`)<br>`st` (float; default `0`)<br>`randscore` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Concat2` | `concat2_ar` | 0..13 positional | `control` (signal; default `0`)<br>`source` (signal; default `0`)<br>`storesize` (float; default `1`)<br>`seektime` (float; default `1`)<br>`seekdur` (float; default `1`)<br>`matchlength` (float; default `0.05`)<br>`freezestore` (float; default `0`)<br>`zcr` (float; default `1`)<br>`lms` (float; default `1`)<br>`sc` (float; default `1`)<br>`st` (float; default `0`)<br>`randscore` (float; default `0`)<br>`threshold` (float; default `0.01`) | 1 channel | backend UGen must be installed |

### sc3_deind.json

Manifest: [`sc3_deind.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_deind.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `JPverb` | `j_pverb_ar` | 0..12 positional | `in` (signal; default `0`)<br>`t60` (float; default `1`)<br>`damp` (float; default `0`)<br>`size` (float; default `1`)<br>`earlyDiff` (float; default `0.707`)<br>`modDepth` (float; default `0.1`)<br>`modFreq` (float; default `2`)<br>`low` (float; default `1`)<br>`mid` (float; default `1`)<br>`high` (float; default `1`)<br>`lowcut` (float; default `500`)<br>`highcut` (float; default `2000`) | 2 channels; handwritten pseudo-lowering; registered return is Dynamic | backend UGen must be installed |
| `JPverbRaw` | `j_pverb_raw_ar`<br>`j_pverb_raw_kr` | 0..13 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`damp` (float; default `0`)<br>`earlydiff` (float; default `0.707`)<br>`highband` (float; default `2000`)<br>`highx` (float; default `1`)<br>`lowband` (float; default `500`)<br>`lowx` (float; default `1`)<br>`mdepth` (float; default `0.1`)<br>`mfreq` (float; default `2`)<br>`midx` (float; default `1`)<br>`size` (float; default `1`)<br>`t60` (float; default `1`) | 2 channels | backend UGen must be installed |
| `Greyhole` | `greyhole_ar` | 0..8 positional | `in` (signal; default `0`)<br>`delayTime` (float; default `2`)<br>`damp` (float; default `0`)<br>`size` (float; default `1`)<br>`diff` (float; default `0.707`)<br>`feedback` (float; default `0.9`)<br>`modDepth` (float; default `0.1`)<br>`modFreq` (float; default `2`) | 2 channels; handwritten pseudo-lowering; registered return is Dynamic | backend UGen must be installed |
| `GreyholeRaw` | `greyhole_raw_ar` | 0..9 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`damping` (float; default `0`)<br>`delaytime` (float; default `2`)<br>`diffusion` (float; default `0.5`)<br>`feedback` (float; default `0.9`)<br>`moddepth` (float; default `0.1`)<br>`modfreq` (float; default `2`)<br>`size` (float; default `1`) | 2 channels | backend UGen must be installed |
| `ComplexRes` | `complex_res_ar` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `100`)<br>`decay` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `DiodeRingMod` | `diode_ring_mod_ar` | 0..2 positional | `car` (signal; default `0`)<br>`mod` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `RMS` | `rms_ar`<br>`rms_kr` | 0..2 positional | `in` (signal; default `0`)<br>`lpFreq` (float; default `10`) | 1 channel | backend UGen must be installed |

### sc3_dfm1.json

Manifest: [`sc3_dfm1.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_dfm1.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `DFM1` | `dfm1_ar` | 0..6 positional | `in` (signal; default `0`)<br>`freq` (signal; default `1000`)<br>`res` (signal; default `0.1`)<br>`inputgain` (signal; default `1`)<br>`type` (signal; default `0`)<br>`noiselevel` (signal; default `0.0003`) | 1 channel | backend UGen must be installed |

### sc3_distortion.json

Manifest: [`sc3_distortion.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_distortion.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `CrossoverDistortion` | `crossover_distortion_ar` | 0..3 positional | `in` (signal; default `0`)<br>`amp` (signal; default `0.5`)<br>`smooth` (signal; default `0.5`) | 1 channel | backend UGen must be installed |
| `Decimator` | `decimator_ar` | 0..3 positional | `in` (signal; default `0`)<br>`rate` (signal; default `44100`)<br>`bits` (signal; default `24`) | 1 channel | backend UGen must be installed |
| `SmoothDecimator` | `smooth_decimator_ar` | 0..3 positional | `in` (signal; default `0`)<br>`rate` (signal; default `44100`)<br>`smoothing` (signal; default `0.5`) | 1 channel | backend UGen must be installed |
| `SineShaper` | `sine_shaper_ar` | 0..2 positional | `in` (signal; default `0`)<br>`limit` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `Disintegrator` | `disintegrator_ar` | 0..3 positional | `in` (signal; default `0`)<br>`probability` (signal; default `0.5`)<br>`multiplier` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_dwg.json

Manifest: [`sc3_dwg.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_dwg.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `DWGPlucked` | `dwg_plucked_ar` | 0..8 positional | `freq` (float; default `440`)<br>`amp` (float; default `0.5`)<br>`gate` (float; default `1`)<br>`pos` (float; default `0.14`)<br>`c1` (float; default `1`)<br>`c3` (float; default `30`)<br>`inp` (signal; default `0`)<br>`release` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `DWGPluckedStiff` | `dwg_plucked_stiff_ar` | 0..9 positional | `freq` (float; default `440`)<br>`amp` (float; default `0.5`)<br>`gate` (float; default `1`)<br>`pos` (float; default `0.14`)<br>`c1` (float; default `1`)<br>`c3` (float; default `30`)<br>`inp` (signal; default `0`)<br>`release` (float; default `0.1`)<br>`fB` (float; default `2`) | 1 channel | backend UGen must be installed |
| `DWGPlucked2` | `dwg_plucked2_ar` | 0..11 positional | `freq` (float; default `440`)<br>`amp` (float; default `0.5`)<br>`gate` (float; default `1`)<br>`pos` (float; default `0.14`)<br>`c1` (float; default `1`)<br>`c3` (float; default `30`)<br>`inp` (signal; default `0`)<br>`release` (float; default `0.1`)<br>`mistune` (float; default `1.008`)<br>`mp` (float; default `0.55`)<br>`gc` (float; default `0.01`) | 1 channel | backend UGen must be installed |
| `DWGBowedSimple` | `dwg_bowed_simple_ar` | 0..8 positional | `freq` (float; default `440`)<br>`velb` (float; default `0.5`)<br>`force` (float; default `1`)<br>`gate` (float; default `1`)<br>`pos` (float; default `0.14`)<br>`release` (float; default `0.1`)<br>`c1` (float; default `1`)<br>`c3` (float; default `30`) | 1 channel | backend UGen must be installed |
| `DWGBowed` | `dwg_bowed_ar` | 0..10 positional | `freq` (float; default `440`)<br>`velb` (float; default `0.5`)<br>`force` (float; default `1`)<br>`gate` (float; default `1`)<br>`pos` (float; default `0.14`)<br>`release` (float; default `0.1`)<br>`c1` (float; default `1`)<br>`c3` (float; default `3`)<br>`impZ` (float; default `0.55`)<br>`fB` (float; default `2`) | 1 channel | backend UGen must be installed |
| `DWGBowedTor` | `dwg_bowed_tor_ar` | 0..14 positional | `freq` (float; default `440`)<br>`velb` (float; default `0.5`)<br>`force` (float; default `1`)<br>`gate` (float; default `1`)<br>`pos` (float; default `0.14`)<br>`release` (float; default `0.1`)<br>`c1` (float; default `1`)<br>`c3` (float; default `3`)<br>`impZ` (float; default `0.55`)<br>`fB` (float; default `2`)<br>`mistune` (float; default `5.2`)<br>`c1tor` (float; default `1`)<br>`c3tor` (float; default `3000`)<br>`iZtor` (float; default `1.8`) | 1 channel | backend UGen must be installed |
| `DWGSoundBoard` | `dwg_sound_board_ar` | 0..12 positional | `inp` (signal; default `0`)<br>`c1` (float; default `20`)<br>`c3` (float; default `20`)<br>`mix` (float; default `0.8`)<br>`d1` (float; default `199`)<br>`d2` (float; default `211`)<br>`d3` (float; default `223`)<br>`d4` (float; default `227`)<br>`d5` (float; default `229`)<br>`d6` (float; default `233`)<br>`d7` (float; default `239`)<br>`d8` (float; default `241`) | 1 channel | backend UGen must be installed |

### sc3_glitch.json

Manifest: [`sc3_glitch.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_glitch.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `GlitchHPF` | `glitch_hpf_ar`<br>`glitch_hpf_kr` | 0..2 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `GlitchRHPF` | `glitch_rhpf_ar`<br>`glitch_rhpf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `GlitchBPF` | `glitch_bpf_ar`<br>`glitch_bpf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `GlitchBRF` | `glitch_brf_ar`<br>`glitch_brf_kr` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |

### sc3_josh_granular.json

Manifest: [`sc3_josh_granular.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_granular.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `MonoGrain` | `mono_grain_ar` | 0..4 positional | `in` (signal; default `0`)<br>`winsize` (float; default `0.1`)<br>`grainrate` (float; default `10`)<br>`winrandpct` (float; default `0`) | 1 channel | backend UGen must be installed |
| `MonoGrainBF` | `mono_grain_bf_ar` | 0..9 positional | `in` (signal; default `0`)<br>`winsize` (float; default `0.1`)<br>`grainrate` (float; default `10`)<br>`winrandpct` (float; default `0`)<br>`azimuth` (float; default `0`)<br>`azrand` (float; default `0`)<br>`elevation` (float; default `0`)<br>`elrand` (float; default `0`)<br>`rho` (float; default `1`) | 4 channels | backend UGen must be installed |
| `SinGrain` | `sin_grain_ar` | 0..3 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `SinGrainB` | `sin_grain_b_ar` | 0..4 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`)<br>`envbuf` (float; default `0`) | 1 channel | backend UGen must be installed |
| `SinGrainI` | `sin_grain_i_ar` | 0..6 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `SinGrainBF` | `sin_grain_bf_ar` | 0..7 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `SinGrainBBF` | `sin_grain_bbf_ar` | 0..8 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`)<br>`envbuf` (float; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `SinGrainIBF` | `sin_grain_ibf_ar` | 0..10 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FMGrain` | `fm_grain_ar` | 0..5 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`) | 1 channel | backend UGen must be installed |
| `FMGrainB` | `fm_grain_b_ar` | 0..6 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`)<br>`envbuf` (float; default `0`) | 1 channel | backend UGen must be installed |
| `FMGrainI` | `fm_grain_i_ar` | 0..8 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `FMGrainBF` | `fm_grain_bf_ar` | 0..9 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FMGrainBBF` | `fm_grain_bbf_ar` | 0..10 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`)<br>`envbuf` (float; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FMGrainIBF` | `fm_grain_ibf_ar` | 0..12 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `BufGrain` | `buf_grain_ar` | 0..6 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`) | 1 channel | backend UGen must be installed |
| `BufGrainB` | `buf_grain_b_ar` | 0..7 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`)<br>`envbuf` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BufGrainI` | `buf_grain_i_ar` | 0..9 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `BufGrainBF` | `buf_grain_bf_ar` | 0..10 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `BufGrainBBF` | `buf_grain_bbf_ar` | 0..11 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`)<br>`envbuf` (float; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `BufGrainIBF` | `buf_grain_ibf_ar` | 0..13 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `InGrain` | `in_grain_ar` | 0..3 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `InGrainB` | `in_grain_b_ar` | 0..4 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`)<br>`envbuf` (float; default `0`) | 1 channel | backend UGen must be installed |
| `InGrainI` | `in_grain_i_ar` | 0..6 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `InGrainBF` | `in_grain_bf_ar` | 0..7 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `InGrainBBF` | `in_grain_bbf_ar` | 0..8 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`)<br>`envbuf` (float; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `InGrainIBF` | `in_grain_ibf_ar` | 0..10 positional | `trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`)<br>`envbuf1` (float; default `0`)<br>`envbuf2` (float; default `0`)<br>`ifac` (float; default `0.5`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `GrainSinJ` | `grain_sin_j_ar` | 0..8 positional | `numChannels` (float; default `1`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`freq` (float; default `440`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`)<br>`grainAmp` (float; default `1`) | 1 channel | backend UGen must be installed |
| `GrainFMJ` | `grain_fmj_ar` | 0..10 positional | `numChannels` (float; default `1`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`carfreq` (float; default `440`)<br>`modfreq` (float; default `200`)<br>`index` (float; default `1`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`)<br>`grainAmp` (float; default `1`) | 1 channel | backend UGen must be installed |
| `GrainBufJ` | `grain_buf_j_ar` | 0..12 positional | `numChannels` (float; default `1`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`sndbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`pos` (float; default `0`)<br>`interp` (float; default `2`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`)<br>`grainAmp` (float; default `1`)<br>`loop` (float; default `0`) | 1 channel | backend UGen must be installed |
| `GrainInJ` | `grain_in_j_ar` | 0..8 positional | `numChannels` (float; default `1`)<br>`trigger` (signal; default `0`)<br>`dur` (float; default `1`)<br>`in` (signal; default `0`)<br>`pan` (float; default `0`)<br>`envbufnum` (float; default `-1`)<br>`maxGrains` (float; default `512`)<br>`grainAmp` (float; default `1`) | 1 channel | backend UGen must be installed |

### sc3_josh_spectral.json

Manifest: [`sc3_josh_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `PV_BinBufRd` | `pv_bin_buf_rd_kr` | 0..7 positional | `buffer` (signal; default `0`)<br>`playbuf` (float; default `0`)<br>`point` (float; default `1`)<br>`binStart` (float; default `0`)<br>`binSkip` (float; default `1`)<br>`numBins` (float; default `1`)<br>`clear` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_BinDelay` | `pv_bin_delay_kr` | 0..5 positional | `buffer` (signal; default `0`)<br>`maxdelay` (float; default `1`)<br>`delaybuf` (float; default `0`)<br>`fbbuf` (float; default `0`)<br>`hop` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `PV_BinFilter` | `pv_bin_filter_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`start` (float; default `0`)<br>`end` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_BinPlayBuf` | `pv_bin_play_buf_kr` | 0..9 positional | `buffer` (signal; default `0`)<br>`playbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`offset` (float; default `0`)<br>`loop` (float; default `0`)<br>`binStart` (float; default `0`)<br>`binSkip` (float; default `1`)<br>`numBins` (float; default `1`)<br>`clear` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_BufRd` | `pv_buf_rd_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`playbuf` (float; default `0`)<br>`point` (float; default `1`) | 1 channel | backend UGen must be installed |
| `PV_EvenBin` | `pv_even_bin_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_OddBin` | `pv_odd_bin_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Freeze` | `pv_freeze_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`freeze` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_FreqBuffer` | `pv_freq_buffer_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`databuffer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Invert` | `pv_invert_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagBuffer` | `pv_mag_buffer_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`databuffer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagMap` | `pv_mag_map_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`mapbuf` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MaxMagN` | `pv_max_mag_n_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`numbins` (float; default `8`) | 1 channel | backend UGen must be installed |
| `PV_MinMagN` | `pv_min_mag_n_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`numbins` (float; default `8`) | 1 channel | backend UGen must be installed |
| `PV_NoiseSynthF` | `pv_noise_synth_f_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0.1`)<br>`numFrames` (float; default `2`)<br>`initflag` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_NoiseSynthP` | `pv_noise_synth_p_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0.1`)<br>`numFrames` (float; default `2`)<br>`initflag` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_PartialSynthF` | `pv_partial_synth_f_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0.1`)<br>`numFrames` (float; default `2`)<br>`initflag` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_PartialSynthP` | `pv_partial_synth_p_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`threshold` (float; default `0.1`)<br>`numFrames` (float; default `2`)<br>`initflag` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_PitchShift` | `pv_pitch_shift_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`ratio` (float; default `1`) | 1 channel | backend UGen must be installed |
| `PV_PlayBuf` | `pv_play_buf_kr` | 0..5 positional | `buffer` (signal; default `0`)<br>`playbuf` (float; default `0`)<br>`rate` (float; default `1`)<br>`offset` (float; default `0`)<br>`loop` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_RecordBuf` | `pv_record_buf_kr` | 0..7 positional | `buffer` (signal; default `0`)<br>`recbuf` (float; default `0`)<br>`offset` (float; default `0`)<br>`run` (float; default `0`)<br>`loop` (float; default `0`)<br>`hop` (float; default `0.5`)<br>`wintype` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_SpectralEnhance` | `pv_spectral_enhance_kr` | 0..4 positional | `buffer` (signal; default `0`)<br>`numPartials` (float; default `8`)<br>`ratio` (float; default `2`)<br>`strength` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `PV_SpectralMap` | `pv_spectral_map_kr` | 0..7 positional | `buffer` (signal; default `0`)<br>`specBuffer` (float; default `0`)<br>`floor` (float; default `0`)<br>`freeze` (float; default `0`)<br>`mode` (float; default `0`)<br>`norm` (float; default `0`)<br>`window` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PVInfo` | `pv_info_ar`<br>`pv_info_kr` | 0..3 positional | `pvbuffer` (float; default `0`)<br>`binNum` (float; default `0`)<br>`filePointer` (float; default `0`) | 2 channels | backend UGen must be installed |
| `PVSynth` | `pv_synth_ar` | 0..7 positional | `pvbuffer` (float; default `0`)<br>`numBins` (float; default `0`)<br>`binStart` (float; default `0`)<br>`binSkip` (float; default `1`)<br>`filePointer` (float; default `0`)<br>`freqMul` (float; default `1`)<br>`freqAdd` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BinData` | `bin_data_ar`<br>`bin_data_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`bin` (float; default `0`)<br>`overlaps` (float; default `0.5`) | 2 channels | backend UGen must be installed |
| `AtsSynth` | `ats_synth_ar` | 0..7 positional | `atsbuffer` (float; default `0`)<br>`numPartials` (float; default `0`)<br>`partialStart` (float; default `0`)<br>`partialSkip` (float; default `1`)<br>`filePointer` (float; default `0`)<br>`freqMul` (float; default `1`)<br>`freqAdd` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AtsNoiSynth` | `ats_noi_synth_ar` | 0..12 positional | `atsbuffer` (float; default `0`)<br>`numPartials` (float; default `0`)<br>`partialStart` (float; default `0`)<br>`partialSkip` (float; default `1`)<br>`filePointer` (float; default `0`)<br>`sinePct` (float; default `1`)<br>`noisePct` (float; default `1`)<br>`freqMul` (float; default `1`)<br>`freqAdd` (float; default `0`)<br>`numBands` (float; default `25`)<br>`bandStart` (float; default `0`)<br>`bandSkip` (float; default `1`) | 1 channel | backend UGen must be installed |
| `AtsPartial` | `ats_partial_ar` | 0..5 positional | `atsbuffer` (float; default `0`)<br>`partial` (float; default `0`)<br>`filePointer` (float; default `0`)<br>`freqMul` (float; default `1`)<br>`freqAdd` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AtsBand` | `ats_band_ar` | 0..3 positional | `atsbuffer` (float; default `0`)<br>`band` (float; default `0`)<br>`filePointer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AtsFreq` | `ats_freq_ar`<br>`ats_freq_kr` | 0..3 positional | `atsbuffer` (float; default `0`)<br>`partialNum` (float; default `0`)<br>`filePointer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AtsAmp` | `ats_amp_ar`<br>`ats_amp_kr` | 0..3 positional | `atsbuffer` (float; default `0`)<br>`partialNum` (float; default `0`)<br>`filePointer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AtsNoise` | `ats_noise_ar`<br>`ats_noise_kr` | 0..3 positional | `atsbuffer` (float; default `0`)<br>`bandNum` (float; default `0`)<br>`filePointer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AtsParInfo` | `ats_par_info_ar`<br>`ats_par_info_kr` | 0..3 positional | `atsbuffer` (float; default `0`)<br>`partialNum` (float; default `0`)<br>`filePointer` (float; default `0`) | 2 channels | backend UGen must be installed |
| `LPCSynth` | `lpc_synth_ar` | 0..3 positional | `buffer` (float; default `0`)<br>`signal` (signal; default `0`)<br>`pointer` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LPCVals` | `lpc_vals_ar`<br>`lpc_vals_kr` | 0..2 positional | `buffer` (float; default `0`)<br>`pointer` (float; default `0`) | 3 channels | backend UGen must be installed |
| `BFEncode1` | `bf_encode1_ar` | 0..6 positional | `in` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`gain` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `BFEncode2` | `bf_encode2_ar` | 0..6 positional | `in` (signal; default `0`)<br>`point_x` (float; default `1`)<br>`point_y` (float; default `1`)<br>`elevation` (float; default `0`)<br>`gain` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `BFEncodeSter` | `bf_encode_ster_ar` | 0..8 positional | `l` (signal; default `0`)<br>`r` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`width` (float; default `1.5707963`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`gain` (float; default `1`)<br>`wComp` (float; default `0`) | 4 channels | backend UGen must be installed |
| `BFDecode1` | `bf_decode1_ar` | 0..7 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`wComp` (float; default `0`) | 1 channel | backend UGen must be installed |
| `BFManipulate` | `bf_manipulate_ar` | 0..7 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`rotate` (float; default `0`)<br>`tilt` (float; default `0`)<br>`tumble` (float; default `0`) | 4 channels | backend UGen must be installed |
| `FMHEncode0` | `fmh_encode0_ar` | 0..4 positional | `in` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`gain` (float; default `1`) | 9 channels | backend UGen must be installed |
| `FMHEncode1` | `fmh_encode1_ar` | 0..6 positional | `in` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`rho` (float; default `1`)<br>`gain` (float; default `1`)<br>`wComp` (float; default `0`) | 9 channels | backend UGen must be installed |
| `FMHEncode2` | `fmh_encode2_ar` | 0..6 positional | `in` (signal; default `0`)<br>`point_x` (float; default `0`)<br>`point_y` (float; default `0`)<br>`elevation` (float; default `0`)<br>`gain` (float; default `1`)<br>`wComp` (float; default `0`) | 9 channels | backend UGen must be installed |
| `FMHDecode1` | `fmh_decode1_ar` | 0..11 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`)<br>`r` (signal; default `0`)<br>`s` (signal; default `0`)<br>`t` (signal; default `0`)<br>`u` (signal; default `0`)<br>`v` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`) | 1 channel | backend UGen must be installed |
| `A2B` | `a2b_ar` | 0..4 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`)<br>`c` (signal; default `0`)<br>`d` (signal; default `0`) | 4 channels | backend UGen must be installed |
| `B2A` | `b2a_ar` | 0..4 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`) | 4 channels | backend UGen must be installed |
| `B2Ster` | `b2_ster_ar` | 0..3 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`) | 2 channels | backend UGen must be installed |
| `B2UHJ` | `b2uhj_ar` | 0..3 positional | `w` (signal; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`) | 2 channels | backend UGen must be installed |
| `UHJ2B` | `uhj2b_ar` | 0..2 positional | `ls` (signal; default `0`)<br>`rs` (signal; default `0`) | 3 channels | backend UGen must be installed |
| `Balance` | `balance_ar` | 0..4 positional | `in` (signal; default `0`)<br>`test` (signal; default `0`)<br>`hp` (float; default `10`)<br>`stor` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Maxamp` | `maxamp_ar` | 0..2 positional | `in` (signal; default `0`)<br>`numSamps` (float; default `1000`) | 1 channel | backend UGen must be installed |
| `Metro` | `metro_ar`<br>`metro_kr` | 0..2 positional | `bpm` (float; default `120`)<br>`numBeats` (float; default `4`) | 1 channel | backend UGen must be installed |
| `MoogVCF` | `moog_vcf_ar` | 0..3 positional | `in` (signal; default `0`)<br>`fco` (float; default `440`)<br>`res` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PanX` | `pan_x_ar`<br>`pan_x_kr` | 0..5 positional | `numChans` (int; default `4`)<br>`in` (signal; default `0`)<br>`pos` (float; default `0`)<br>`level` (float; default `1`)<br>`width` (float; default `2`) | 4 channels | backend UGen must be installed |
| `PermMod` | `perm_mod_ar` | 0..2 positional | `in` (signal; default `0`)<br>`freq` (float; default `100`) | 1 channel | backend UGen must be installed |
| `PermModT` | `perm_mod_t_ar` | 0..3 positional | `in` (signal; default `0`)<br>`outfreq` (float; default `440`)<br>`infreq` (float; default `5000`) | 1 channel | backend UGen must be installed |
| `PermModArray` | `perm_mod_array_ar` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `50`)<br>`pattern` (float; default `0`) | 1 channel | backend UGen must be installed |
| `AudioMSG` | `audio_msg_ar` | 0..2 positional | `in` (signal; default `0`)<br>`index` (float; default `0`) | 1 channel | backend UGen must be installed |
| `CombLP` | `comb_lp_ar` | 0..6 positional | `in` (signal; default `0`)<br>`gate` (float; default `1`)<br>`maxdelaytime` (float; default `0.2`)<br>`delaytime` (float; default `0.2`)<br>`decaytime` (float; default `1`)<br>`coef` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `PosRatio` | `pos_ratio_ar` | 0..3 positional | `in` (signal; default `0`)<br>`period` (float; default `100`)<br>`thresh` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `SinTone` | `sin_tone_ar` | 0..2 positional | `freq` (float; default `440`)<br>`phase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `TTendency` | `t_tendency_ar`<br>`t_tendency_kr` | 0..6 positional | `trigger` (signal; default `0`)<br>`dist` (float; default `0`)<br>`parX` (float; default `0`)<br>`parY` (float; default `1`)<br>`parA` (float; default `0`)<br>`parB` (float; default `0`) | 1 channel | backend UGen must be installed |
| `WarpZ` | `warp_z_ar` | 0..11 positional | `numChannels` (int; default `1`)<br>`bufnum` (float; default `0`)<br>`pointer` (float; default `0`)<br>`freqScale` (float; default `1`)<br>`windowSize` (float; default `0.2`)<br>`envbufnum` (float; default `-1`)<br>`overlaps` (float; default `8`)<br>`windowRandRatio` (float; default `0`)<br>`interp` (float; default `1`)<br>`zeroSearch` (float; default `0`)<br>`zeroStart` (float; default `0`) | 1 channel | backend UGen must be installed |

### sc3_loopbuf.json

Manifest: [`sc3_loopbuf.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_loopbuf.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `LoopBuf` | `loop_buf_ar` | 0..8 positional | `numChannels` (int; default `1`)<br>`bufnum` (int; default `0`)<br>`rate` (signal; default `1`)<br>`gate` (signal; default `1`)<br>`startPos` (float; default `0`)<br>`startLoop` (float; default `0`)<br>`endLoop` (float; default `0`)<br>`interpolation` (int; default `2`) | 1 channel | backend UGen must be installed |

### sc3_mcld.json

Manifest: [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `ArrayMax` | `array_max_ar`<br>`array_max_kr` | 0..1 positional | `array` (signal; default `0`) | 2 channels | backend UGen must be installed |
| `ArrayMin` | `array_min_ar`<br>`array_min_kr` | 0..1 positional | `array` (signal; default `0`) | 2 channels | backend UGen must be installed |
| `BufMax` | `buf_max_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`gate` (float; default `1`) | 2 channels | backend UGen must be installed |
| `BufMin` | `buf_min_kr` | 0..2 positional | `bufnum` (float; default `0`)<br>`gate` (float; default `1`) | 2 channels | backend UGen must be installed |
| `Cepstrum` | `cepstrum_kr` | 0..2 positional | `cepbuf` (float; default `0`)<br>`fftchain` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `ICepstrum` | `i_cepstrum_kr` | 0..2 positional | `cepchain` (signal; default `0`)<br>`fftbuf` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_DiffMags` | `pv_diff_mags_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`zerolimit` (float; default `0`) | 1 channel; emits `PV_MagSubtract`; manifest pseudo metadata | backend UGen must be installed |
| `PV_MagSubtract` | `pv_mag_subtract_kr` | 0..3 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`)<br>`zerolimit` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagLog` | `pv_mag_log_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagExp` | `pv_mag_exp_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PV_Whiten` | `pv_whiten_kr` | 0..6 positional | `chain` (signal; default `0`)<br>`trackbufnum` (float; default `0`)<br>`relaxtime` (float; default `2`)<br>`floor` (float; default `0.1`)<br>`smear` (float; default `0`)<br>`bindownsample` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_MagSmooth` | `pv_mag_smooth_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`factor` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `PV_MagMulAdd` | `pv_mag_mul_add_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`mul` (float; default `1`)<br>`add` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PV_ExtractRepeat` | `pv_extract_repeat_kr` | 0..7 positional | `buffer` (signal; default `0`)<br>`loopbuf` (float; default `0`)<br>`loopdur` (float; default `1`)<br>`memorytime` (float; default `30`)<br>`which` (float; default `0`)<br>`ffthop` (float; default `0.5`)<br>`thresh` (float; default `1`) | 1 channel | backend UGen must be installed |
| `FFTPower` | `fft_power_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`square` (float; default `1`) | 1 channel | backend UGen must be installed |
| `FFTDiffMags` | `fft_diff_mags_kr` | 0..2 positional | `bufferA` (signal; default `0`)<br>`bufferB` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `FFTFlux` | `fft_flux_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`normalise` (float; default `1`) | 1 channel | backend UGen must be installed |
| `FFTFluxPos` | `fft_flux_pos_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`normalise` (float; default `1`) | 1 channel | backend UGen must be installed |
| `FFTSubbandPower` | `fft_subband_power_kr` | 0..4 positional | `chain` (signal; default `0`)<br>`cutfreqs` (signal; default `0`)<br>`square` (float; default `1`)<br>`scalemode` (float; default `1`) | 1 channel | backend UGen must be installed |
| `FFTPhaseDev` | `fft_phase_dev_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`weight` (float; default `0`)<br>`powthresh` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `FFTComplexDev` | `fft_complex_dev_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`rectify` (float; default `0`)<br>`powthresh` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `FFTMKL` | `fftmkl_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`epsilon` (float; default `1.0e-06`) | 1 channel | backend UGen must be installed |
| `FFTCentroid` | `fft_centroid_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel; emits `SpecCentroid`; manifest pseudo metadata | backend UGen must be installed |
| `FFTSubbandFlatness` | `fft_subband_flatness_kr` | 0..2 positional | `chain` (signal; default `0`)<br>`cutfreqs` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `FFTCrest` | `fft_crest_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`freqlo` (float; default `0`)<br>`freqhi` (float; default `50000`) | 1 channel | backend UGen must be installed |
| `FFTSpread` | `fft_spread_kr` | 0..2 positional | `buffer` (signal; default `0`)<br>`centroid` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `FFTSlope` | `fft_slope_kr` | 0..1 positional | `buffer` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `FFTPeak` | `fft_peak_kr` | 0..3 positional | `buffer` (signal; default `0`)<br>`freqlo` (float; default `0`)<br>`freqhi` (float; default `50000`) | 2 channels | backend UGen must be installed |
| `RosslerL` | `rossler_l_ar` | 0..8 positional | `freq` (float; default `22050`)<br>`a` (float; default `0.2`)<br>`b` (float; default `0.2`)<br>`c` (float; default `5.7`)<br>`h` (float; default `0.05`)<br>`xi` (float; default `0.1`)<br>`yi` (float; default `0`)<br>`zi` (float; default `0`) | 3 channels | backend UGen must be installed |
| `FincoSprottL` | `finco_sprott_l_ar` | 0..6 positional | `freq` (float; default `22050`)<br>`a` (float; default `2.45`)<br>`h` (float; default `0.05`)<br>`xi` (float; default `0`)<br>`yi` (float; default `0`)<br>`zi` (float; default `0`) | 3 channels | backend UGen must be installed |
| `FincoSprottM` | `finco_sprott_m_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`a` (float; default `-7`)<br>`b` (float; default `4`)<br>`h` (float; default `0.05`)<br>`xi` (float; default `0`)<br>`yi` (float; default `0`)<br>`zi` (float; default `0`) | 3 channels | backend UGen must be installed |
| `FincoSprottS` | `finco_sprott_s_ar` | 0..7 positional | `freq` (float; default `22050`)<br>`a` (float; default `8`)<br>`b` (float; default `2`)<br>`h` (float; default `0.05`)<br>`xi` (float; default `0`)<br>`yi` (float; default `0`)<br>`zi` (float; default `0`) | 3 channels | backend UGen must be installed |
| `Perlin3` | `perlin3_ar`<br>`perlin3_kr` | 0..3 positional | `x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`z` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `InsideOut` | `inside_out_ar`<br>`inside_out_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `WaveLoss` | `wave_loss_ar`<br>`wave_loss_kr` | 0..4 positional | `in` (signal; default `0`)<br>`drop` (float; default `20`)<br>`outof` (float; default `40`)<br>`mode` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Squiz` | `squiz_ar`<br>`squiz_kr` | 0..4 positional | `in` (signal; default `0`)<br>`pitchratio` (float; default `2`)<br>`zcperchunk` (float; default `1`)<br>`memlen` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `Logger` | `logger_kr` | 0..4 positional | `bufnum` (float; default `0`)<br>`trig` (signal; default `0`)<br>`reset` (signal; default `0`)<br>`inputArray` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `ListTrig` | `list_trig_kr` | 0..4 positional | `bufnum` (float; default `0`)<br>`reset` (signal; default `0`)<br>`offset` (float; default `0`)<br>`numframes` (float; default `0`) | 1 channel | backend UGen must be installed |
| `ListTrig2` | `list_trig2_kr` | 0..3 positional | `bufnum` (float; default `0`)<br>`reset` (signal; default `0`)<br>`numframes` (float; default `0`) | 1 channel | backend UGen must be installed |
| `GaussClass` | `gauss_class_kr` | 0..3 positional | `bufnum` (float; default `0`)<br>`gate` (signal; default `0`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Friction` | `friction_ar`<br>`friction_kr` | 0..6 positional | `in` (signal; default `0`)<br>`friction` (float; default `0.5`)<br>`spring` (float; default `0.414`)<br>`damp` (float; default `0.313`)<br>`mass` (float; default `0.1`)<br>`beltmass` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Crest` | `crest_kr` | 0..3 positional | `in` (signal; default `0`)<br>`numsamps` (float; default `400`)<br>`gate` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `Goertzel` | `goertzel_kr` | 0..4 positional | `in` (signal; default `0`)<br>`bufsize` (float; default `1024`)<br>`freq` (float; default `440`)<br>`hop` (float; default `1`) | 2 channels | backend UGen must be installed |
| `SawDPW` | `saw_dpw_ar`<br>`saw_dpw_kr` | 0..2 positional | `freq` (float; default `440`)<br>`iphase` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Getenv` | `getenv_ir` | 0..2 positional | `key` (float; default `0`)<br>`defaultval` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Clockmus` | `clockmus_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `TextVU` | `text_vu_ar`<br>`text_vu_kr` | 0..6 positional | `trig` (signal; default `2`)<br>`in` (signal; default `0`)<br>`label` (float; default `0`)<br>`width` (float; default `21`)<br>`reset` (signal; default `0`)<br>`ana` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `MeanTriggered` | `mean_triggered_ar`<br>`mean_triggered_kr` | 0..3 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`)<br>`length` (float; default `10`) | 1 channel | backend UGen must be installed |
| `MedianTriggered` | `median_triggered_ar`<br>`median_triggered_kr` | 0..3 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`)<br>`length` (float; default `10`) | 1 channel | backend UGen must be installed |
| `KMeansRT` | `k_means_rt_kr` | 0..6 positional | `bufnum` (float; default `0`)<br>`k` (float; default `5`)<br>`gate` (signal; default `1`)<br>`reset` (signal; default `0`)<br>`learn` (float; default `1`)<br>`inputdata` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PlaneTree` | `plane_tree_kr` | 0..3 positional | `treebuf` (float; default `0`)<br>`gate` (signal; default `1`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `NearestN` | `nearest_n_kr` | 0..4 positional | `treebuf` (float; default `0`)<br>`gate` (signal; default `1`)<br>`num` (float; default `1`)<br>`in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `MatchingP` | `matching_p_ar`<br>`matching_p_kr` | 0..6 positional | `dict` (float; default `0`)<br>`in` (signal; default `0`)<br>`dictsize` (float; default `1`)<br>`ntofind` (float; default `1`)<br>`hop` (float; default `1`)<br>`method` (float; default `0`) | 1 channel | backend UGen must be installed |
| `MatchingPResynth` | `matching_p_resynth_ar`<br>`matching_p_resynth_kr` | 0..5 positional | `dict` (float; default `0`)<br>`method` (float; default `0`)<br>`trigger` (signal; default `0`)<br>`residual` (signal; default `0`)<br>`activs` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `SOMTrain` | `som_train_kr` | 0..8 positional | `bufnum` (float; default `0`)<br>`netsize` (float; default `10`)<br>`numdims` (float; default `2`)<br>`traindur` (float; default `5000`)<br>`nhood` (float; default `0.5`)<br>`gate` (signal; default `1`)<br>`initweight` (float; default `1`)<br>`inputdata` (signal; default `0`) | 3 channels | backend UGen must be installed |
| `SOMRd` | `som_rd_ar`<br>`som_rd_kr` | 0..5 positional | `bufnum` (float; default `0`)<br>`netsize` (float; default `10`)<br>`numdims` (float; default `2`)<br>`gate` (signal; default `1`)<br>`inputdata` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `SOMAreaWr` | `som_area_wr_kr` | 0..6 positional | `bufnum` (float; default `0`)<br>`netsize` (float; default `10`)<br>`numdims` (float; default `2`)<br>`nhood` (float; default `0.5`)<br>`gate` (signal; default `1`)<br>`inputdata` (signal; default `0`) | 1 channel | backend UGen must be installed |

### sc3_mda.json

Manifest: [`sc3_mda.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mda.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `MdaPiano` | `mda_piano_ar` | 0..15 positional | `freq` (signal; default `440`)<br>`gate` (signal; default `1`)<br>`vel` (signal; default `100`)<br>`decay` (float; default `0.8`)<br>`release` (float; default `0.8`)<br>`hard` (float; default `0.8`)<br>`velhard` (float; default `0.8`)<br>`muffle` (float; default `0.8`)<br>`velmuff` (float; default `0.8`)<br>`velcurve` (float; default `0.8`)<br>`stereo` (float; default `0.2`)<br>`tune` (float; default `0.5`)<br>`random` (float; default `0.1`)<br>`stretch` (float; default `0.1`)<br>`sustain` (signal; default `0`) | 2 channels | backend UGen must be installed |

### sc3_membrane.json

Manifest: [`sc3_membrane.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_membrane.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `MembraneCircle` | `membrane_circle_ar` | 0..3 positional | `excitation` (signal; default `0`)<br>`tension` (float; default `0.05`)<br>`loss` (float; default `0.99999`) | 1 channel | backend UGen must be installed |
| `MembraneHexagon` | `membrane_hexagon_ar` | 0..3 positional | `excitation` (signal; default `0`)<br>`tension` (float; default `0.05`)<br>`loss` (float; default `0.99999`) | 1 channel | backend UGen must be installed |

### sc3_ncanalysis.json

Manifest: [`sc3_ncanalysis.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_ncanalysis.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `LPCAnalyzer` | `lpc_analyzer_ar` | 0..7 positional | `input` (signal; default `0`)<br>`source` (signal; default `0.01`)<br>`n` (int; default `256`)<br>`p` (int; default `10`)<br>`testE` (int; default `0`)<br>`delta` (float; default `0.999`)<br>`windowtype` (int; default `0`) | 1 channel | backend UGen must be installed |
| `MedianSeparation` | `median_separation_kr` | 0..8 positional | `fft` (signal; default `0`)<br>`fftharmonic` (int; default `0`)<br>`fftpercussive` (int; default `0`)<br>`fftsize` (int; default `1024`)<br>`mediansize` (int; default `17`)<br>`hardorsoft` (int; default `0`)<br>`p` (float; default `2`)<br>`medianormax` (int; default `0`) | 2 channels | backend UGen must be installed |
| `SMS` | `sms_ar` | 0..11 positional | `input` (signal; default `0`)<br>`maxpeaks` (int; default `80`)<br>`currentpeaks` (int; default `80`)<br>`tolerance` (int; default `4`)<br>`noisefloor` (float; default `0.2`)<br>`freqmult` (float; default `1`)<br>`freqadd` (float; default `0`)<br>`formantpreserve` (int; default `0`)<br>`useifft` (int; default `0`)<br>`ampmult` (float; default `1`)<br>`graphicsbufnum` (int; default `-1`) | 2 channels | backend UGen must be installed |
| `TPV` | `tpv_ar` | 0..8 positional | `chain` (signal; default `0`)<br>`windowsize` (int; default `1024`)<br>`hopsize` (int; default `512`)<br>`maxpeaks` (int; default `80`)<br>`currentpeaks` (int; default `80`)<br>`freqmult` (float; default `1`)<br>`tolerance` (int; default `4`)<br>`noisefloor` (float; default `0.2`) | 1 channel | backend UGen must be installed |
| `WalshHadamard` | `walsh_hadamard_ar` | 0..2 positional | `input` (signal; default `0`)<br>`which` (int; default `0`) | 1 channel | backend UGen must be installed |
| `WaveletDaub` | `wavelet_daub_ar` | 0..3 positional | `input` (signal; default `0`)<br>`n` (int; default `64`)<br>`which` (int; default `0`) | 1 channel | backend UGen must be installed |

### sc3_nh_hall.json

Manifest: [`sc3_nh_hall.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_nh_hall.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `NHHall` | `nh_hall_ar` | 0..12 positional | `inLeft` (signal; default `0`)<br>`inRight` (signal; default `0`)<br>`rt60` (float; default `1`)<br>`stereo` (float; default `0.5`)<br>`lowFreq` (float; default `200`)<br>`lowRatio` (float; default `0.5`)<br>`hiFreq` (float; default `4000`)<br>`hiRatio` (float; default `0.5`)<br>`earlyDiffusion` (float; default `0.5`)<br>`lateDiffusion` (float; default `0.5`)<br>`modRate` (float; default `0.2`)<br>`modDepth` (float; default `0.3`) | 2 channels | backend UGen must be installed |

### sc3_otey_piano.json

Manifest: [`sc3_otey_piano.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_otey_piano.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `OteyPiano` | `otey_piano_ar` | 0..20 positional; `Array[24]` | `freq` (float; default `440`)<br>`vel` (float; default `1`)<br>`t_gate` (signal; default `0`)<br>`rmin` (float; default `0.35`)<br>`rmax` (float; default `2`)<br>`rampl` (float; default `4`)<br>`rampr` (float; default `8`)<br>`rcore` (float; default `1`)<br>`lmin` (float; default `0.07`)<br>`lmax` (float; default `1.4`)<br>`lampl` (float; default `-4`)<br>`lampr` (float; default `4`)<br>`rho` (float; default `1`)<br>`e` (float; default `1`)<br>`zb` (float; default `1`)<br>`zh` (float; default `0`)<br>`mh` (float; default `1`)<br>`k` (float; default `0.2`)<br>`alpha` (float; default `1`)<br>`p` (float; default `1`)<br>`hpos` (float; default `0.142`)<br>`loss` (float; default `1`)<br>`detune` (float; default `0.0003`)<br>`hammer_type` (float; default `1`) | 1 channel | backend UGen must be installed |

### sc3_pitch_detection.json

Manifest: [`sc3_pitch_detection.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_pitch_detection.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Qitch` | `qitch_kr` | 0..7 positional | `in` (signal; default `0`)<br>`databufnum` (float; default `0`)<br>`ampThreshold` (float; default `0.01`)<br>`algoflag` (float; default `1`)<br>`ampbufnum` (float; default `-1`)<br>`minfreq` (float; default `0`)<br>`maxfreq` (float; default `2500`) | 2 channels | backend UGen must be installed |
| `Tartini` | `tartini_kr` | 0..6 positional | `in` (signal; default `0`)<br>`threshold` (float; default `0.93`)<br>`n` (float; default `2048`)<br>`k` (float; default `0`)<br>`overlap` (float; default `1024`)<br>`smallCutoff` (float; default `0.5`) | 2 channels | backend UGen must be installed |

### sc3_quantity.json

Manifest: [`sc3_quantity.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_quantity.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `MovingAverage` | `moving_average_ar`<br>`moving_average_kr` | 0..3 positional | `in` (signal; default `0`)<br>`numsamp` (float; default `40`)<br>`maxsamp` (int; default `400`) | 1 channel | backend UGen must be installed |
| `MovingSum` | `moving_sum_ar`<br>`moving_sum_kr` | 0..3 positional | `in` (signal; default `0`)<br>`numsamp` (float; default `40`)<br>`maxsamp` (int; default `400`) | 1 channel | backend UGen must be installed |

### sc3_rfw.json

Manifest: [`sc3_rfw.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_rfw.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `AverageOutput` | `average_output_ar`<br>`average_output_kr` | 0..2 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `SwitchDelay` | `switch_delay_ar` | 0..6 positional | `in` (signal; default `0`)<br>`drylevel` (float; default `1`)<br>`wetlevel` (float; default `1`)<br>`delaytime` (signal; default `1`)<br>`delayfactor` (float; default `0.7`)<br>`maxdelaytime` (float; default `20`) | 1 channel | backend UGen must be installed |

### sc3_rmeqsuite.json

Manifest: [`sc3_rmeqsuite.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_rmeqsuite.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Allpass1` | `allpass1_ar` | 0..2 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`) | 1 channel | backend UGen must be installed |
| `Allpass2` | `allpass2_ar` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `1200`)<br>`rq` (float; default `1`) | 1 channel | backend UGen must be installed |
| `RMEQ` | `rmeq_ar` | 0..4 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`rq` (float; default `0.1`)<br>`k` (float; default `0`) | 1 channel | backend UGen must be installed |
| `RMShelf` | `rm_shelf_ar` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`k` (float; default `0`) | 1 channel | backend UGen must be installed |
| `RMShelf2` | `rm_shelf2_ar` | 0..3 positional | `in` (signal; default `0`)<br>`freq` (float; default `440`)<br>`k` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Spreader` | `spreader_ar` | 0..3 positional | `in` (signal; default `0`)<br>`theta` (float; default `1.5707963267949`)<br>`filtsPerOctave` (int; default `8`) | 2 channels | backend UGen must be installed |

### sc3_scmir.json

Manifest: [`sc3_scmir.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_scmir.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `AttackSlope` | `attack_slope_kr` | 0..8 positional | `input` (signal; default `0`)<br>`windowsize` (int; default `1024`)<br>`peakpicksize` (int; default `20`)<br>`leak` (float; default `0.999`)<br>`energythreshold` (float; default `0.01`)<br>`sumthreshold` (float; default `20`)<br>`mingap` (int; default `30`)<br>`numslopesaveraged` (int; default `10`) | 6 channels | backend UGen must be installed |
| `BeatStatistics` | `beat_statistics_kr` | 0..3 positional | `fft` (signal; default `0`)<br>`leak` (float; default `0.995`)<br>`numpreviousbeats` (int; default `4`) | 4 channels | backend UGen must be installed |
| `Chromagram` | `chromagram_kr` | 0..9 positional | `fft` (signal; default `0`)<br>`fftsize` (int; default `2048`)<br>`n` (int; default `12`)<br>`tuningbase` (float; default `32.703195662575`)<br>`octaves` (int; default `8`)<br>`integrationflag` (int; default `0`)<br>`coeff` (float; default `0.9`)<br>`octaveratio` (float; default `2`)<br>`perframenormalize` (int; default `0`) | 12 channels | backend UGen must be installed |
| `KeyClarity` | `key_clarity_kr` | 0..3 positional | `chain` (signal; default `0`)<br>`keydecay` (float; default `2`)<br>`chromaleak` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `KeyMode` | `key_mode_kr` | 0..3 positional | `chain` (signal; default `0`)<br>`keydecay` (float; default `2`)<br>`chromaleak` (float; default `0.5`) | 1 channel | backend UGen must be installed |
| `OnsetStatistics` | `onset_statistics_kr` | 0..3 positional | `input` (signal; default `0`)<br>`windowsize` (float; default `1`)<br>`hopsize` (float; default `0.1`) | 3 channels | backend UGen must be installed |
| `SensoryDissonance` | `sensory_dissonance_kr` | 0..5 positional | `fft` (signal; default `0`)<br>`maxpeaks` (int; default `100`)<br>`peakthreshold` (float; default `0.1`)<br>`norm` (float; default `0.0001`)<br>`clamp` (float; default `1`) | 1 channel | backend UGen must be installed |
| `SpectralEntropy` | `spectral_entropy_kr` | 0..3 positional | `fft` (signal; default `0`)<br>`fftsize` (int; default `2048`)<br>`numbands` (int; default `1`) | 1 channel | backend UGen must be installed |

### sc3_sl.json

Manifest: [`sc3_sl.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_sl.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `SortBuf` | `sort_buf_ar` | 0..3 positional | `bufnum` (float; default `0`)<br>`sortrate` (float; default `10`)<br>`reset` (float; default `0`) | 1 channel | backend UGen must be installed |
| `GravityGrid` | `gravity_grid_ar` | 0..5 positional | `reset` (float; default `0`)<br>`rate` (float; default `0.1`)<br>`newx` (float; default `0`)<br>`newy` (float; default `0`)<br>`bufnum` (float; default `-1`) | 1 channel | backend UGen must be installed |
| `GravityGrid2` | `gravity_grid2_ar` | 0..5 positional | `reset` (float; default `0`)<br>`rate` (float; default `0.1`)<br>`newx` (float; default `0`)<br>`newy` (float; default `0`)<br>`bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Breakcore` | `breakcore_ar` | 0..5 positional | `bufnum` (float; default `0`)<br>`capturein` (signal; default `0`)<br>`capturetrigger` (signal; default `0`)<br>`duration` (float; default `0.1`)<br>`ampdropout` (float; default `0`) | 1 channel | backend UGen must be installed |
| `Max` | `max_kr` | 0..2 positional | `in` (signal; default `0`)<br>`numsamp` (float; default `64`) | 1 channel | backend UGen must be installed |
| `PrintVal` | `print_val_kr` | 0..3 positional | `in` (signal; default `0`)<br>`numblocks` (float; default `100`)<br>`id` (float; default `0`) | 1 channel | backend UGen must be installed |
| `EnvDetect` | `env_detect_ar` | 0..3 positional | `in` (signal; default `0`)<br>`attack` (float; default `100`)<br>`release` (float; default `0`) | 1 channel | backend UGen must be installed |
| `FitzHughNagumo` | `fitz_hugh_nagumo_ar` | 0..7 positional | `reset` (float; default `0`)<br>`rateu` (float; default `0.01`)<br>`ratew` (float; default `0.01`)<br>`b0` (float; default `1`)<br>`b1` (float; default `1`)<br>`initu` (float; default `0`)<br>`initw` (float; default `0`) | 1 channel | backend UGen must be installed |
| `DoubleWell` | `double_well_ar` | 0..8 positional | `reset` (float; default `0`)<br>`ratex` (float; default `0.01`)<br>`ratey` (float; default `0.01`)<br>`f` (float; default `1`)<br>`w` (float; default `0.001`)<br>`delta` (float; default `1`)<br>`initx` (float; default `0`)<br>`inity` (float; default `0`) | 1 channel | backend UGen must be installed |
| `DoubleWell2` | `double_well2_ar` | 0..8 positional | `reset` (float; default `0`)<br>`ratex` (float; default `0.01`)<br>`ratey` (float; default `0.01`)<br>`f` (float; default `1`)<br>`w` (float; default `0.001`)<br>`delta` (float; default `1`)<br>`initx` (float; default `0`)<br>`inity` (float; default `0`) | 1 channel | backend UGen must be installed |
| `DoubleWell3` | `double_well3_ar` | 0..6 positional | `reset` (float; default `0`)<br>`rate` (float; default `0.01`)<br>`f` (float; default `0`)<br>`delta` (float; default `0.25`)<br>`initx` (float; default `0`)<br>`inity` (float; default `0`) | 1 channel | backend UGen must be installed |
| `WeaklyNonlinear` | `weakly_nonlinear_ar` | 0..11 positional | `input` (signal; default `0`)<br>`reset` (float; default `0`)<br>`ratex` (float; default `1`)<br>`ratey` (float; default `1`)<br>`freq` (float; default `440`)<br>`initx` (float; default `0`)<br>`inity` (float; default `0`)<br>`alpha` (float; default `0`)<br>`xexponent` (float; default `0`)<br>`beta` (float; default `0`)<br>`yexponent` (float; default `0`) | 1 channel | backend UGen must be installed |
| `WeaklyNonlinear2` | `weakly_nonlinear2_ar` | 0..11 positional | `input` (signal; default `0`)<br>`reset` (float; default `0`)<br>`ratex` (float; default `1`)<br>`ratey` (float; default `1`)<br>`freq` (float; default `440`)<br>`initx` (float; default `0`)<br>`inity` (float; default `0`)<br>`alpha` (float; default `0`)<br>`xexponent` (float; default `0`)<br>`beta` (float; default `0`)<br>`yexponent` (float; default `0`) | 1 channel | backend UGen must be installed |
| `TermanWang` | `terman_wang_ar` | 0..9 positional | `input` (signal; default `0`)<br>`reset` (float; default `0`)<br>`ratex` (float; default `0.01`)<br>`ratey` (float; default `0.01`)<br>`alpha` (float; default `1`)<br>`beta` (float; default `1`)<br>`eta` (float; default `1`)<br>`initx` (float; default `0`)<br>`inity` (float; default `0`) | 1 channel | backend UGen must be installed |
| `LTI` | `lti_ar` | 0..3 positional | `input` (signal; default `0`)<br>`bufnuma` (float; default `0`)<br>`bufnumb` (float; default `1`) | 1 channel | backend UGen must be installed |
| `NL` | `nl_ar` | 0..5 positional | `input` (signal; default `0`)<br>`bufnuma` (float; default `0`)<br>`bufnumb` (float; default `1`)<br>`guard1` (float; default `1000`)<br>`guard2` (float; default `100`) | 1 channel | backend UGen must be installed |
| `NL2` | `nl2_ar` | 0..6 positional | `input` (signal; default `0`)<br>`bufnum` (float; default `0`)<br>`maxsizea` (float; default `10`)<br>`maxsizeb` (float; default `10`)<br>`guard1` (float; default `1000`)<br>`guard2` (float; default `100`) | 1 channel | backend UGen must be installed |
| `LPCError` | `lpc_error_ar` | 0..2 positional | `input` (signal; default `0`)<br>`p` (float; default `10`) | 1 channel | backend UGen must be installed |
| `KmeansToBPSet1` | `kmeans_to_bp_set1_ar` | 0..8 positional | `freq` (float; default `440`)<br>`numdatapoints` (float; default `20`)<br>`maxnummeans` (float; default `4`)<br>`nummeans` (float; default `4`)<br>`tnewdata` (float; default `1`)<br>`tnewmeans` (float; default `1`)<br>`soft` (float; default `1`)<br>`bufnum` (float; default `-1`) | 1 channel | backend UGen must be installed |
| `Instruction` | `instruction_ar` | 0..1 positional | `bufnum` (float; default `0`) | 1 channel | backend UGen must be installed |
| `WaveTerrain` | `wave_terrain_ar` | 0..5 positional | `bufnum` (float; default `0`)<br>`x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`xsize` (float; default `100`)<br>`ysize` (float; default `100`) | 1 channel | backend UGen must be installed |
| `VMScan2D` | `vm_scan2d_ar` | 0..1 positional | `bufnum` (float; default `0`) | 2 channels | backend UGen must be installed |
| `SLOnset` | `sl_onset_kr` | 0..6 positional | `input` (signal; default `0`)<br>`memorysize1` (float; default `20`)<br>`before` (float; default `5`)<br>`after` (float; default `5`)<br>`threshold` (float; default `10`)<br>`hysteresis` (float; default `10`) | 1 channel | backend UGen must be installed |
| `TwoTube` | `two_tube_ar` | 0..5 positional | `input` (signal; default `0`)<br>`k` (float; default `0.01`)<br>`loss` (float; default `1`)<br>`d1length` (float; default `100`)<br>`d2length` (float; default `100`) | 1 channel | backend UGen must be installed |
| `NTube` | `n_tube_ar` | 0..4 positional | `input` (signal; default `0`)<br>`lossarray` (signal; default `1`)<br>`karray` (signal; default `0`)<br>`delaylengtharray` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `EnvFollow` | `env_follow_ar`<br>`env_follow_kr` | 0..2 positional | `input` (signal; default `0`)<br>`decaycoeff` (float; default `0.99`) | 1 channel | backend UGen must be installed |
| `Sieve1` | `sieve1_ar`<br>`sieve1_kr` | 0..3 positional | `bufnum` (float; default `0`)<br>`gap` (float; default `2`)<br>`alternate` (float; default `1`) | 1 channel | backend UGen must be installed |
| `Oregonator` | `oregonator_ar` | 0..8 positional | `reset` (float; default `0`)<br>`rate` (float; default `0.01`)<br>`epsilon` (float; default `1`)<br>`mu` (float; default `1`)<br>`q` (float; default `1`)<br>`initx` (float; default `0.5`)<br>`inity` (float; default `0.5`)<br>`initz` (float; default `0.5`) | 3 channels | backend UGen must be installed |
| `Brusselator` | `brusselator_ar` | 0..6 positional | `reset` (float; default `0`)<br>`rate` (float; default `0.01`)<br>`mu` (float; default `1`)<br>`gamma` (float; default `1`)<br>`initx` (float; default `0.5`)<br>`inity` (float; default `0.5`) | 2 channels | backend UGen must be installed |
| `SpruceBudworm` | `spruce_budworm_ar` | 0..10 positional | `reset` (float; default `0`)<br>`rate` (float; default `0.1`)<br>`k1` (float; default `27.9`)<br>`k2` (float; default `1.5`)<br>`alpha` (float; default `0.1`)<br>`beta` (float; default `10.1`)<br>`mu` (float; default `0.3`)<br>`rho` (float; default `10.1`)<br>`initx` (float; default `0.9`)<br>`inity` (float; default `0.1`) | 2 channels | backend UGen must be installed |

### sc3_stk.json

Manifest: [`sc3_stk.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_stk.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `StkPluck` | `stk_pluck_ar`<br>`stk_pluck_kr` | 0..2 positional | `freq` (float; default `440`)<br>`decay` (float; default `0.99`) | 1 channel | backend UGen must be installed |
| `StkFlute` | `stk_flute_ar`<br>`stk_flute_kr` | 0..4 positional | `freq` (float; default `440`)<br>`jetDelay` (float; default `49`)<br>`noisegain` (float; default `0.15`)<br>`jetRatio` (float; default `0.32`) | 1 channel | backend UGen must be installed |
| `StkBowed` | `stk_bowed_ar`<br>`stk_bowed_kr` | 0..7 positional | `freq` (float; default `220`)<br>`bowpressure` (float; default `64`)<br>`bowposition` (float; default `64`)<br>`vibfreq` (float; default `64`)<br>`vibgain` (float; default `64`)<br>`loudness` (float; default `64`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkMandolin` | `stk_mandolin_ar`<br>`stk_mandolin_kr` | 0..7 positional | `freq` (float; default `520`)<br>`bodysize` (float; default `64`)<br>`pickposition` (float; default `64`)<br>`stringdamping` (float; default `69`)<br>`stringdetune` (float; default `10`)<br>`aftertouch` (float; default `64`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkSaxofony` | `stk_saxofony_ar`<br>`stk_saxofony_kr` | 0..9 positional | `freq` (float; default `220`)<br>`reedstiffness` (float; default `64`)<br>`reedaperture` (float; default `64`)<br>`noisegain` (float; default `20`)<br>`blowposition` (float; default `26`)<br>`vibratofrequency` (float; default `20`)<br>`vibratogain` (float; default `20`)<br>`breathpressure` (float; default `128`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkShakers` | `stk_shakers_ar`<br>`stk_shakers_kr` | 0..5 positional | `instr` (float; default `0`)<br>`energy` (float; default `64`)<br>`decay` (float; default `64`)<br>`objects` (float; default `64`)<br>`resfreq` (float; default `64`) | 1 channel | backend UGen must be installed |
| `StkBandedWG` | `stk_banded_wg_ar`<br>`stk_banded_wg_kr` | 0..9 positional | `freq` (float; default `440`)<br>`instr` (float; default `0`)<br>`bowpressure` (float; default `0`)<br>`bowmotion` (float; default `0`)<br>`integration` (float; default `0`)<br>`modalresonance` (float; default `64`)<br>`bowvelocity` (float; default `0`)<br>`setstriking` (float; default `0`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkVoicForm` | `stk_voic_form_ar`<br>`stk_voic_form_kr` | 0..7 positional | `freq` (float; default `440`)<br>`vuvmix` (float; default `64`)<br>`vowelphon` (float; default `64`)<br>`vibfreq` (float; default `64`)<br>`vibgain` (float; default `20`)<br>`loudness` (float; default `64`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkModalBar` | `stk_modal_bar_ar`<br>`stk_modal_bar_kr` | 0..9 positional | `freq` (float; default `440`)<br>`instrument` (float; default `0`)<br>`stickhardness` (float; default `64`)<br>`stickposition` (float; default `64`)<br>`vibratogain` (float; default `20`)<br>`vibratofreq` (float; default `20`)<br>`directstickmix` (float; default `64`)<br>`volume` (float; default `64`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkClarinet` | `stk_clarinet_ar`<br>`stk_clarinet_kr` | 0..7 positional | `freq` (float; default `440`)<br>`reedstiffness` (float; default `64`)<br>`noisegain` (float; default `4`)<br>`vibfreq` (float; default `64`)<br>`vibgain` (float; default `11`)<br>`breathpressure` (float; default `64`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkBlowHole` | `stk_blow_hole_ar`<br>`stk_blow_hole_kr` | 0..6 positional | `freq` (float; default `440`)<br>`reedstiffness` (float; default `64`)<br>`noisegain` (float; default `20`)<br>`tonehole` (float; default `64`)<br>`register` (float; default `11`)<br>`breathpressure` (float; default `64`) | 1 channel | backend UGen must be installed |
| `StkMoog` | `stk_moog_ar`<br>`stk_moog_kr` | 0..7 positional | `freq` (float; default `440`)<br>`filterQ` (float; default `10`)<br>`sweeprate` (float; default `20`)<br>`vibfreq` (float; default `64`)<br>`vibgain` (float; default `0`)<br>`gain` (float; default `64`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkBeeThree` | `stk_bee_three_ar`<br>`stk_bee_three_kr` | 0..7 positional | `freq` (float; default `440`)<br>`op4gain` (float; default `10`)<br>`op3gain` (float; default `20`)<br>`lfospeed` (float; default `64`)<br>`lfodepth` (float; default `0`)<br>`adsrtarget` (float; default `64`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkSitar` | `stk_sitar_ar`<br>`stk_sitar_kr` | 0..2 positional | `freq` (float; default `440`)<br>`trig` (signal; default `1`) | 1 channel | backend UGen must be installed |
| `StkStifKarp` | `stk_stif_karp_ar`<br>`stk_stif_karp_kr` | 0..5 positional | `freq` (float; default `440`)<br>`gain` (float; default `1`)<br>`pickuppos` (float; default `0`)<br>`stringsustain` (float; default `0`)<br>`stringstretch` (float; default `0`) | 1 channel | backend UGen must be installed |
| `StkTubeBell` | `stk_tube_bell_ar`<br>`stk_tube_bell_kr` | 0..1 positional | `freq` (float; default `440`) | 1 channel | backend UGen must be installed |
| `Sflute` | `sflute_ar`<br>`sflute_kr` | 0..7 positional | `freq` (float; default `440`)<br>`pressure` (float; default `0.5`)<br>`randamp` (float; default `0.1`)<br>`dampcoef` (float; default `0.0001`)<br>`lipopen` (float; default `20`)<br>`jetstream` (float; default `0.5`)<br>`fullwave` (float; default `1`) | 1 channel | backend UGen must be installed |

### sc3_summer.json

Manifest: [`sc3_summer.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_summer.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Summer` | `summer_ar`<br>`summer_kr` | 0..4 positional | `trig` (signal; default `0`)<br>`step` (float; default `1`)<br>`reset` (signal; default `0`)<br>`resetval` (float; default `0`) | 1 channel | backend UGen must be installed |
| `WrapSummer` | `wrap_summer_ar`<br>`wrap_summer_kr` | 0..6 positional | `trig` (signal; default `0`)<br>`step` (float; default `1`)<br>`min` (float; default `0`)<br>`max` (float; default `1`)<br>`reset` (signal; default `0`)<br>`resetval` (float; default `0`) | 1 channel | backend UGen must be installed |

### sc3_vbap.json

Manifest: [`sc3_vbap.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_vbap.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `VBAP` | `vbap_ar`<br>`vbap_kr` | 0..6 positional | `numChans` (float; default `4`)<br>`in` (signal; default `0`)<br>`bufnum` (float; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`spread` (float; default `0`) | 4 channels | backend UGen must be installed |
| `CircleRamp` | `circle_ramp_ar`<br>`circle_ramp_kr` | 0..4 positional | `in` (signal; default `0`)<br>`lagTime` (float; default `0.1`)<br>`circmin` (float; default `-180`)<br>`circmax` (float; default `180`) | 1 channel | backend UGen must be installed |

### sc3_vosim.json

Manifest: [`sc3_vosim.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_vosim.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `VOSIM` | `vosim_ar` | 0..4 positional | `trig` (signal; default `0.1`)<br>`freq` (signal; default `400`)<br>`nCycles` (int; default `1`)<br>`decay` (float; default `0.9`) | 1 channel | backend UGen must be installed |

### sc_hoa.json

Manifest: [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `HOAEncoder1` | `hoa_encoder1_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`gain_0` (float; default `0`)<br>`radius_0` (float; default `2`)<br>`azimuth_0` (float; default `0`)<br>`elevation_0` (float; default `0`)<br>`plane_spherical` (float; default `0`)<br>`speaker_radius_0` (float; default `1.07`) | 4 channels | backend UGen must be installed |
| `HOAEncoder2` | `hoa_encoder2_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`gain_0` (float; default `0`)<br>`radius_0` (float; default `2`)<br>`azimuth_0` (float; default `0`)<br>`elevation_0` (float; default `0`)<br>`plane_spherical` (float; default `0`)<br>`speaker_radius_0` (float; default `1.07`) | 9 channels | backend UGen must be installed |
| `HOAEncoder3` | `hoa_encoder3_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`gain_0` (float; default `0`)<br>`radius_0` (float; default `2`)<br>`azimuth_0` (float; default `0`)<br>`elevation_0` (float; default `0`)<br>`plane_spherical` (float; default `0`)<br>`speaker_radius_0` (float; default `1.07`) | 16 channels | backend UGen must be installed |
| `HOAEncoder4` | `hoa_encoder4_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`gain_0` (float; default `0`)<br>`radius_0` (float; default `2`)<br>`azimuth_0` (float; default `0`)<br>`elevation_0` (float; default `0`)<br>`plane_spherical` (float; default `0`)<br>`speaker_radius_0` (float; default `1.07`) | 25 channels | backend UGen must be installed |
| `HOAEncoder5` | `hoa_encoder5_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`gain_0` (float; default `0`)<br>`radius_0` (float; default `2`)<br>`azimuth_0` (float; default `0`)<br>`elevation_0` (float; default `0`)<br>`plane_spherical` (float; default `0`)<br>`speaker_radius_0` (float; default `1.07`) | 36 channels | backend UGen must be installed |
| `HOADecLebedev061` | `hoa_dec_lebedev061_ar` | 0..8 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`inputs_gain` (float; default `0`)<br>`outputs_gain` (float; default `0`)<br>`yes` (float; default `1`)<br>`speakers_radius` (float; default `1`) | 6 channels | backend UGen must be installed |
| `HOADecLebedev262` | `hoa_dec_lebedev262_ar` | 0..13 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`inputs_gain` (float; default `0`)<br>`outputs_gain` (float; default `0`)<br>`yes` (float; default `1`)<br>`speakers_radius` (float; default `1`) | 26 channels | backend UGen must be installed |
| `HOADecLebedev501` | `hoa_dec_lebedev501_ar` | 0..8 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`inputs_gain` (float; default `0`)<br>`outputs_gain` (float; default `0`)<br>`yes` (float; default `1`)<br>`speakers_radius` (float; default `1`) | 50 channels | backend UGen must be installed |
| `HOADecLebedev502` | `hoa_dec_lebedev502_ar` | 0..13 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`inputs_gain` (float; default `0`)<br>`outputs_gain` (float; default `0`)<br>`yes` (float; default `1`)<br>`speakers_radius` (float; default `1`) | 50 channels | backend UGen must be installed |
| `HOAAzimuthRotator1` | `hoa_azimuth_rotator1_ar` | 0..5 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`az` (float; default `0`) | 4 channels | backend UGen must be installed |
| `HOAAzimuthRotator2` | `hoa_azimuth_rotator2_ar` | 0..10 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`az` (float; default `0`) | 9 channels | backend UGen must be installed |
| `HOARotator1` | `hoa_rotator1_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`pitch` (float; default `0`)<br>`roll` (float; default `0`)<br>`yaw` (float; default `0`) | 4 channels | backend UGen must be installed |
| `HOARotator2` | `hoa_rotator2_ar` | 0..12 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`pitch` (float; default `0`)<br>`roll` (float; default `0`)<br>`yaw` (float; default `0`) | 9 channels | backend UGen must be installed |
| `HOAMirror1` | `hoa_mirror1_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`front_back` (float; default `0`)<br>`left_right` (float; default `0`)<br>`up_down` (float; default `0`) | 4 channels | backend UGen must be installed |
| `HOAMirror2` | `hoa_mirror2_ar` | 0..12 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`front_back` (float; default `0`)<br>`left_right` (float; default `0`)<br>`up_down` (float; default `0`) | 9 channels | backend UGen must be installed |
| `HOABeamDirac2HOA1` | `hoa_beam_dirac2hoa1_ar` | 0..11 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`gain` (float; default `0`)<br>`on` (float; default `1`)<br>`timer_manual` (float; default `0`)<br>`crossfade` (float; default `1`)<br>`focus` (float; default `0`) | 4 channels | backend UGen must be installed |
| `HOABeamDirac2HOA2` | `hoa_beam_dirac2hoa2_ar` | 0..16 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`gain` (float; default `0`)<br>`on` (float; default `1`)<br>`timer_manual` (float; default `0`)<br>`crossfade` (float; default `1`)<br>`focus` (float; default `0`) | 9 channels | backend UGen must be installed |
| `HOABeamHCardio2HOA1` | `hoa_beam_h_cardio2hoa1_ar` | 0..8 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`int_float` (float; default `0`)<br>`order` (float; default `1`) | 4 channels | backend UGen must be installed |
| `HOABeamHCardio2HOA2` | `hoa_beam_h_cardio2hoa2_ar` | 0..13 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`int_float` (float; default `0`)<br>`order` (float; default `1`) | 9 channels | backend UGen must be installed |
| `HOABeamHCardio2Mono1` | `hoa_beam_h_cardio2_mono1_ar` | 0..7 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`output_gain` (float; default `0`) | 1 channel | backend UGen must be installed |
| `HOABeamHCardio2Mono2` | `hoa_beam_h_cardio2_mono2_ar` | 0..12 positional | `in1` (signal; default `0`)<br>`in2` (signal; default `0`)<br>`in3` (signal; default `0`)<br>`in4` (signal; default `0`)<br>`in5` (signal; default `0`)<br>`in6` (signal; default `0`)<br>`in7` (signal; default `0`)<br>`in8` (signal; default `0`)<br>`in9` (signal; default `0`)<br>`azimuth` (float; default `0`)<br>`elevation` (float; default `0`)<br>`output_gain` (float; default `0`) | 1 channel | backend UGen must be installed |

### triggers.json

Manifest: [`triggers.json`](../../../crates/vibelang-dsp/ugen_manifests/triggers.json)

| Class | Registered callable name(s) | Exact arities | Ordered inputs and omission defaults | Output / lowering | Availability |
|---|---|---|---|---|---|
| `Trig` | `trig_ar`<br>`trig_kr` | 0..2 positional | `in` (signal; default `0`)<br>`dur` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `Trig1` | `trig1_ar`<br>`trig1_kr` | 0..2 positional | `in` (signal; default `0`)<br>`dur` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `SendTrig` | `send_trig_ar`<br>`send_trig_kr` | 0..3 positional | `in` (signal; default `0`)<br>`id` (float; default `0`)<br>`value` (float; default `0`) | 0 channels | backend UGen must be installed |
| `SendReply` | `send_reply_ar`<br>`send_reply_kr` | 0..4 positional | `trig` (signal; default `0`)<br>`cmdName` (float; default `0`)<br>`values` (signal; default `0`)<br>`replyID` (float; default `-1`) | 0 channels | backend UGen must be installed |
| `Latch` | `latch_ar`<br>`latch_kr` | 0..2 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Gate` | `gate_ar`<br>`gate_kr` | 0..2 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PulseDivider` | `pulse_divider_ar`<br>`pulse_divider_kr` | 0..3 positional | `trig` (signal; default `0`)<br>`div` (float; default `2`)<br>`start` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PulseCount` | `pulse_count_ar`<br>`pulse_count_kr` | 0..2 positional | `trig` (signal; default `0`)<br>`reset` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Peak` | `peak_ar`<br>`peak_kr` | 0..2 positional | `in` (signal; default `0`)<br>`trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Schmidt` | `schmidt_ar`<br>`schmidt_kr` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (float; default `0`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `SetResetFF` | `set_reset_ff_ar`<br>`set_reset_ff_kr` | 0..2 positional | `trig` (signal; default `0`)<br>`reset` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `ToggleFF` | `toggle_ff_ar`<br>`toggle_ff_kr` | 0..1 positional | `trig` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Stepper` | `stepper_ar`<br>`stepper_kr` | 0..6 positional | `trig` (signal; default `0`)<br>`reset` (signal; default `0`)<br>`min` (float; default `0`)<br>`max` (float; default `7`)<br>`step` (float; default `1`)<br>`resetval` (float; default `0`) | 1 channel | backend UGen must be installed |
| `TrigControl` | `trig_control_kr` | 0 | — | 1 channel | backend UGen must be installed |
| `InRange` | `in_range_ar`<br>`in_range_kr`<br>`in_range_ir` | 0..3 positional | `in` (signal; default `0`)<br>`lo` (float; default `0`)<br>`hi` (float; default `1`) | 1 channel | backend UGen must be installed |
| `InRect` | `in_rect_ar`<br>`in_rect_kr`<br>`in_rect_ir` | 0..3 positional | `x` (signal; default `0`)<br>`y` (signal; default `0`)<br>`rect` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Changed` | `changed_ar`<br>`changed_kr` | 0..2 positional | `in` (signal; default `0`)<br>`threshold` (float; default `0`) | 1 channel; handwritten pseudo-lowering; registered return is Dynamic | backend UGen must be installed |
| `Done` | `done_kr` | 0..1 positional | `src` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Free` | `free_kr` | 0..2 positional | `trig` (signal; default `0`)<br>`id` (float; default `0`) | 1 channel | backend UGen must be installed |
| `FreeSelf` | `free_self_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `FreeSelfWhenDone` | `free_self_when_done_kr` | 0..1 positional | `src` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `Pause` | `pause_kr` | 0..2 positional | `gate` (signal; default `1`)<br>`id` (float; default `0`) | 1 channel | backend UGen must be installed |
| `PauseSelf` | `pause_self_kr` | 0..1 positional | `in` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `PauseSelfWhenDone` | `pause_self_when_done_kr` | 0..1 positional | `src` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `LeastChange` | `least_change_ar`<br>`least_change_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `MostChange` | `most_change_ar`<br>`most_change_kr` | 0..2 positional | `a` (signal; default `0`)<br>`b` (signal; default `0`) | 1 channel | backend UGen must be installed |
| `LastValue` | `last_value_ar`<br>`last_value_kr` | 0..2 positional | `in` (signal; default `0`)<br>`diff` (float; default `0.01`) | 1 channel | backend UGen must be installed |
| `TDelay` | `t_delay_ar`<br>`t_delay_kr` | 0..2 positional | `in` (signal; default `0`)<br>`dur` (float; default `0.1`) | 1 channel | backend UGen must be installed |
| `SendPeakRMS` | `send_peak_rms_ar`<br>`send_peak_rms_kr` | 0..5 positional | `sig` (signal; default `0`)<br>`replyRate` (float; default `20`)<br>`peakLag` (float; default `3`)<br>`cmdName` (float; default `0`)<br>`replyID` (float; default `-1`) | 0 channels | backend UGen must be installed |
| `Poll` | `poll_ar`<br>`poll_kr` | 0..4 positional | `trig` (signal; default `0`)<br>`in` (signal; default `0`)<br>`label` (float; default `0`)<br>`trigid` (float; default `-1`) | 1 channel | backend UGen must be installed |

## Builder-only manifest records (not generated callables)

A `rates: ["builder"]` record is documentation metadata. It does **not** create a rate-suffixed Rhai function. A few concepts have separate handwritten helpers documented in [DSP](../dsp.md); otherwise the record is unavailable until a lowering or builder is implemented.

| Manifest | Class/concept | Historical function metadata | Reason / availability |
|---|---|---|---|
| [`buffers.json`](../../../crates/vibelang-dsp/ugen_manifests/buffers.json) | `SimpleLoopBuf` | `simple_loop_buf_ar` | Removed/commented-out upstream UGen; no installed binary registers SimpleLoopBuf. |
| [`control.json`](../../../crates/vibelang-dsp/ugen_manifests/control.json) | `SelectX` | `select_x_ar`, `select_x_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`conversion.json`](../../../crates/vibelang-dsp/ugen_manifests/conversion.json) | `Silence` | `silence_ar`, `silence_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`envelopes.json`](../../../crates/vibelang-dsp/ugen_manifests/envelopes.json) | `envelope` | `envelope` | VibeLang fluent envelope builder; no literal server UGen named envelope is emitted. |
| [`filters.json`](../../../crates/vibelang-dsp/ugen_manifests/filters.json) | `BLowPass4` | `b_low_pass4_ar`, `b_low_pass4_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`filters.json`](../../../crates/vibelang-dsp/ugen_manifests/filters.json) | `BHiPass4` | `b_hi_pass4_ar`, `b_hi_pass4_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`inout.json`](../../../crates/vibelang-dsp/ugen_manifests/inout.json) | `SoundIn` | `sound_in_ar`, `sound_in_channel`, `sound_in` | Sclang input helper; VibeLang provides manual lowering through sound_in_ar/sound_in_channel. |
| [`link.json`](../../../crates/vibelang-dsp/ugen_manifests/link.json) | `LinkTempo` | `link_tempo_kr` | Ableton Link UGen plugin/source not installed or verified on this host. |
| [`link.json`](../../../crates/vibelang-dsp/ugen_manifests/link.json) | `LinkPhase` | `link_phase_kr` | Ableton Link UGen plugin/source not installed or verified on this host. |
| [`link.json`](../../../crates/vibelang-dsp/ugen_manifests/link.json) | `LinkJump` | `link_jump_kr` | Ableton Link UGen plugin/source not installed or verified on this host. |
| [`math.json`](../../../crates/vibelang-dsp/ugen_manifests/math.json) | `ExpLin` | `exp_lin_ar`, `exp_lin_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`math.json`](../../../crates/vibelang-dsp/ugen_manifests/math.json) | `ExpExp` | `exp_exp_ar`, `exp_exp_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`oscillators.json`](../../../crates/vibelang-dsp/ugen_manifests/oscillators.json) | `PMOsc` | `pm_osc_ar`, `pm_osc_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`random.json`](../../../crates/vibelang-dsp/ugen_manifests/random.json) | `TWChoose` | `tw_choose_ar`, `tw_choose_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`sc3_deind.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_deind.json) | `FaustGreyholeRaw` | — | No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| [`sc3_fm7.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_fm7.json) | `FM7` | — | Phase-modulation oscillator matrix (6x6) from sc3-plugins SkUGens. Each of 6 oscillators has frequency, phase and amplitude controls; any oscillator's output can phase-modulate any other (including itself for feedback). Produces 6 channels — one per operator. Documented as a builder-only entry because the underlying UGen is variadic (18 control + 36 modulation values flattened into one input list) and does not fit the fixed-arity manifest signature; runtime invocation requires special-case wiring outside the auto-generated bindings. |
| [`sc3_josh_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json) | `AtsFile` | `ats_file` | Client-side data/helper class; no scsynth UGen is emitted. |
| [`sc3_josh_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json) | `Rotate` | `rotate_ar` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`sc3_josh_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json) | `Tilt` | `tilt_ar` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`sc3_josh_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json) | `Tumble` | `tumble_ar` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`sc3_josh_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json) | `PanX2D` | `pan_x_2_d_ar`, `pan_x_2_d_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json) | `FFTSubbandFlux` | `fft_subband_flux_kr` | No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json) | `RosslerResL` | `rossler_res_l_ar` | No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json) | `RMAFoodChainL` | `rma_food_chain_l_ar` | No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json) | `MIDelay` | `mi_delay_kr` | No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json) | `PulseDPW` | `pulse_dpw_ar`, `pulse_dpw_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json) | `OnsetsDS` | `onsets_ds_kr` | Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| [`sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json) | `CQ_Diff` | `cq_diff_kr` | No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| [`sc3_neuromodules.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_neuromodules.json) | `Dneuromodule` | — | Demand-rate discrete-time neurodynamics simulation. Models the dynamics of a small recurrent neural network with n nodes — initial state x[i], bias theta[i], and an n*n weight matrix between nodes. Transfer function is tanh. Implementation follows Pasemann (2002) 'Complex dynamics and the structure of small neural networks'. From sc3-plugins Neuromodules. Documented as a builder-only entry because the input list is variadic (numChannels, plus n thetas, n initial states, and n*n weights flattened); runtime invocation requires special-case wiring outside the auto-generated bindings. |
| [`sc3_scmir.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_scmir.json) | `FeatureSave` | — | Stores feature vectors to a file in NRT mode (or low-load RT). On each trigger, the current features array is sampled and appended. Use unit commands createfile/closefile to control the file. From sc3-plugins SCMIRUGens. Documented as a builder-only entry because the features input is a variadic array flattened into the input list. |
| [`sc3_tag_system.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_tag_system.json) | `Dtag` | — | Demand-rate tag system — Emil Post's tag system as a demand UGen. Reads symbols from an axiom array, applies production rules indexed by integer symbols, and deletes v symbols at each step. The tape size is bounded; recycle/mode controls behaviour on overrun and empty conditions. From sc3-plugins TagSystemUGens. Documented as a builder-only entry because the input list is variadic (axiom and rules arrays are flattened into the input vector). |
| [`sc3_tag_system.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_tag_system.json) | `DbufTag` | — | Demand-rate tag system that runs on an external Buffer instead of a fixed-size internal tape. Like Dtag but uses Buffer.alloc, allowing multiple tag processes to share or overwrite each other's output on a single buffer. From sc3-plugins TagSystemUGens. Documented as a builder-only entry because the input list is variadic. |
| [`sc3_tag_system.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_tag_system.json) | `Dfsm` | — | Demand-rate finite-state machine — a Markov chain in UGen form. Each state holds a number of next-state choices; one is randomly selected per step from a user-provided RNG. Similar to Pfsm but evaluable as a UGen at demand rate. From sc3-plugins TagSystemUGens. Documented as a builder-only entry because the rules input is variadic and structured. |
| [`sc3_vbap.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_vbap.json) | `VBAPSpeakerArray` | `vbap_speaker_array` | Client-side buffer-geometry helper; no scsynth UGen is emitted. |
| [`sc3_vbap.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_vbap.json) | `VBAPSpeaker` | `vbap_speaker` | Client-side data/helper class for VBAPSpeakerArray; no scsynth UGen is emitted. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOAEncLebedev061` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOALibEnc3D1` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOALibEnc3D2` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOALibEnc3D3` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOALibEnc3D4` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOALibEnc3D5` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOAmbiPanner1` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOAmbiPanner2` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOAmbiPanner3` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOAmbiPanner4` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `HOAmbiPanner5` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `ITU5001` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| [`sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json) | `ITU5002` | — | Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |

## Regeneration strategy

The checked-in page is the discoverability artifact. The target pipeline should call the same name/arity transformation as `build.rs`, emit the registration manifest, and fail CI if counts, names, defaults, rates, shapes, plugin metadata, or this Markdown drift. See the [API roadmap](../../roadmap/api-improvement-roadmap.md#publication-and-generation-order).
