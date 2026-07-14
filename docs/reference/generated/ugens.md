# Generated UGen function index

> Generated from `api/public-api-manifest-v1.json`; edit canonical DSP manifests and regenerate instead of editing this file.

This index contains **1174 runtime-callable identities**, **25 quarantined identities**, and **48 documentation-only builder records**. Availability is copied from the registration manifest, so an unregistered demand identity cannot appear as callable.

| Availability | Meaning |
|---|---|
| `available` | Registered without a host feature condition; backend plugins may still be required |
| `conditional` | Registration or execution depends on the listed feature/target/plugin condition |
| `quarantined` | Canonical source record retained but no runtime overload registered |
| `documentation_only` | Builder model, not a generated rate-suffixed callable |

## `analysis.json`

Source: [`crates/vibelang-dsp/ugen_manifests/analysis.json`](../../../crates/vibelang-dsp/ugen_manifests/analysis.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BeatTrack` | `beat_track_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`lock` (`float`; default `0`) | 4 | `available` |
| `BeatTrack2` | `beat_track2_kr` | `kr` / `control` | `busindex` (`float`; default `0`)<br>`numfeatures` (`float`; default `1`)<br>`windowsize` (`float`; default `2.0`)<br>`phaseaccuracy` (`float`; default `0.02`)<br>`lock` (`float`; default `0`)<br>`weightingscheme` (`float`; default `-2.1`) | 6 | `available` |
| `KeyTrack` | `key_track_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`keydecay` (`float`; default `2.0`)<br>`chromaleak` (`float`; default `0.5`) | 1 | `available` |
| `Loudness` | `loudness_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`smask` (`float`; default `0.25`)<br>`tmask` (`float`; default `1`) | 1 | `available` |
| `MFCC` | `mfcc_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`numcoeff` (`int`; default `13`) | 13 | `available` |
| `Onsets` | `onsets_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`threshold` (`float`; default `0.5`)<br>`odftype` (`int`; default `3`)<br>`relaxtime` (`float`; default `1`)<br>`floor` (`float`; default `0.1`)<br>`mingap` (`int`; default `10`)<br>`medianspan` (`int`; default `11`)<br>`whtype` (`int`; default `1`)<br>`rawodf` (`int`; default `0`) | 1 | `available` |
| `PV_HainsworthFoote` | `pv_hainsworth_foote_ar` | `ar` / `audio` | `buffer` (`signal`; default `0`)<br>`proph` (`float`; default `0`)<br>`propf` (`float`; default `0`)<br>`threshold` (`float`; default `1.0`)<br>`waittime` (`float`; default `0.04`) | 1 | `available` |
| `PV_JensenAndersen` | `pv_jensen_andersen_ar` | `ar` / `audio` | `buffer` (`signal`; default `0`)<br>`propsc` (`float`; default `0.25`)<br>`prophfe` (`float`; default `0.25`)<br>`prophfc` (`float`; default `0.25`)<br>`propsf` (`float`; default `0.25`)<br>`threshold` (`float`; default `1.0`)<br>`waittime` (`float`; default `0.04`) | 1 | `available` |
| `PeakFollower` | `peak_follower_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`decay` (`float`; default `0.999`) | 1 | `available` |
| `PeakFollower` | `peak_follower_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`decay` (`float`; default `0.999`) | 1 | `available` |
| `Pitch` | `pitch_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`initFreq` (`float`; default `440`)<br>`minFreq` (`float`; default `60`)<br>`maxFreq` (`float`; default `4000`)<br>`execFreq` (`float`; default `100`)<br>`maxBinsPerOctave` (`float`; default `16`)<br>`median` (`float`; default `1`)<br>`ampThreshold` (`float`; default `0.01`)<br>`peakThreshold` (`float`; default `0.5`)<br>`downSample` (`float`; default `1`) | 2 | `available` |
| `RunningMax` | `running_max_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`) | 1 | `available` |
| `RunningMax` | `running_max_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`) | 1 | `available` |
| `RunningMin` | `running_min_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`) | 1 | `available` |
| `RunningMin` | `running_min_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`) | 1 | `available` |
| `RunningSum` | `running_sum_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`) | 1 | `available` |
| `RunningSum` | `running_sum_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`) | 1 | `available` |
| `SpecCentroid` | `spec_centroid_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `SpecFlatness` | `spec_flatness_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `SpecPcile` | `spec_pcile_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`fraction` (`float`; default `0.5`)<br>`interpolate` (`int`; default `0`)<br>`binout` (`int`; default `0`) | 1 | `available` |
| `ZeroCrossing` | `zero_crossing_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `ZeroCrossing` | `zero_crossing_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |

## `atk_foa.json`

Source: [`crates/vibelang-dsp/ugen_manifests/atk_foa.json`](../../../crates/vibelang-dsp/ugen_manifests/atk_foa.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `FoaAsymmetry` | `foa_asymmetry_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaAsymmetry` | `foa_asymmetry_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectO` | `foa_direct_o_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectO` | `foa_direct_o_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectX` | `foa_direct_x_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectX` | `foa_direct_x_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectY` | `foa_direct_y_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectY` | `foa_direct_y_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectZ` | `foa_direct_z_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDirectZ` | `foa_direct_z_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaDominateX` | `foa_dominate_x_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`gain` (`float`; default `0`) | 4 | `available` |
| `FoaDominateX` | `foa_dominate_x_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`gain` (`float`; default `0`) | 4 | `available` |
| `FoaDominateY` | `foa_dominate_y_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`gain` (`float`; default `0`) | 4 | `available` |
| `FoaDominateY` | `foa_dominate_y_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`gain` (`float`; default `0`) | 4 | `available` |
| `FoaDominateZ` | `foa_dominate_z_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`gain` (`float`; default `0`) | 4 | `available` |
| `FoaDominateZ` | `foa_dominate_z_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`gain` (`float`; default `0`) | 4 | `available` |
| `FoaFocusX` | `foa_focus_x_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaFocusX` | `foa_focus_x_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaFocusY` | `foa_focus_y_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaFocusY` | `foa_focus_y_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaFocusZ` | `foa_focus_z_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaFocusZ` | `foa_focus_z_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaNFC` | `foa_nfc_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`distance` (`float`; default `1.0`)<br>`speedOfSound` (`float`; default `343.0`) | 4 | `available` |
| `FoaNFC` | `foa_nfc_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`distance` (`float`; default `1.0`)<br>`speedOfSound` (`float`; default `343.0`) | 4 | `available` |
| `FoaPanB` | `foa_pan_b_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`) | 4 | `available` |
| `FoaPanB` | `foa_pan_b_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`) | 4 | `available` |
| `FoaPressX` | `foa_press_x_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPressX` | `foa_press_x_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPressY` | `foa_press_y_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPressY` | `foa_press_y_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPressZ` | `foa_press_z_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPressZ` | `foa_press_z_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaProximity` | `foa_proximity_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`distance` (`float`; default `1.0`)<br>`speedOfSound` (`float`; default `343.0`) | 4 | `available` |
| `FoaProximity` | `foa_proximity_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`distance` (`float`; default `1.0`)<br>`speedOfSound` (`float`; default `343.0`) | 4 | `available` |
| `FoaPsychoShelf` | `foa_psycho_shelf_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`freq` (`float`; default `400.0`)<br>`k0` (`float`; default `1.0`)<br>`k1` (`float`; default `1.0`) | 4 | `available` |
| `FoaPsychoShelf` | `foa_psycho_shelf_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`freq` (`float`; default `400.0`)<br>`k0` (`float`; default `1.0`)<br>`k1` (`float`; default `1.0`) | 4 | `available` |
| `FoaPushX` | `foa_push_x_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPushX` | `foa_push_x_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPushY` | `foa_push_y_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPushY` | `foa_push_y_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPushZ` | `foa_push_z_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaPushZ` | `foa_push_z_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaRotate` | `foa_rotate_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaRotate` | `foa_rotate_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaTilt` | `foa_tilt_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaTilt` | `foa_tilt_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaTumble` | `foa_tumble_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaTumble` | `foa_tumble_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaZoomX` | `foa_zoom_x_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaZoomX` | `foa_zoom_x_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaZoomY` | `foa_zoom_y_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaZoomY` | `foa_zoom_y_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaZoomZ` | `foa_zoom_z_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |
| `FoaZoomZ` | `foa_zoom_z_kr` | `kr` / `control` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`angle` (`float`; default `0`) | 4 | `available` |

## `bufdelays.json`

Source: [`crates/vibelang-dsp/ugen_manifests/bufdelays.json`](../../../crates/vibelang-dsp/ugen_manifests/bufdelays.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BufAllpassC` | `buf_allpass_c_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `BufAllpassL` | `buf_allpass_l_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `BufAllpassN` | `buf_allpass_n_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `BufCombC` | `buf_comb_c_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `BufCombL` | `buf_comb_l_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `BufCombN` | `buf_comb_n_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `BufDelayC` | `buf_delay_c_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `BufDelayC` | `buf_delay_c_kr` | `kr` / `control` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `BufDelayL` | `buf_delay_l_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `BufDelayL` | `buf_delay_l_kr` | `kr` / `control` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `BufDelayN` | `buf_delay_n_ar` | `ar` / `audio` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `BufDelayN` | `buf_delay_n_kr` | `kr` / `control` | `buf` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |

## `buffers.json`

Source: [`crates/vibelang-dsp/ugen_manifests/buffers.json`](../../../crates/vibelang-dsp/ugen_manifests/buffers.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BufChannels` | `buf_channels_ir` | `ir` / `scalar` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufChannels` | `buf_channels_kr` | `kr` / `control` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufDur` | `buf_dur_ir` | `ir` / `scalar` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufDur` | `buf_dur_kr` | `kr` / `control` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufFrames` | `buf_frames_ir` | `ir` / `scalar` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufFrames` | `buf_frames_kr` | `kr` / `control` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufRateScale` | `buf_rate_scale_ir` | `ir` / `scalar` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufRateScale` | `buf_rate_scale_kr` | `kr` / `control` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufRd` | `buf_rd_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`phase` (`signal`; default `0`)<br>`loop` (`float`; default `1`)<br>`interpolation` (`float`; default `2`) | 1 | `available` |
| `BufRd` | `buf_rd_kr` | `kr` / `control` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`phase` (`signal`; default `0`)<br>`loop` (`float`; default `1`)<br>`interpolation` (`float`; default `2`) | 1 | `available` |
| `BufSampleRate` | `buf_sample_rate_ir` | `ir` / `scalar` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufSampleRate` | `buf_sample_rate_kr` | `kr` / `control` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufSamples` | `buf_samples_ir` | `ir` / `scalar` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufSamples` | `buf_samples_kr` | `kr` / `control` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `BufWr` | `buf_wr_ar` | `ar` / `audio` | `inputArray` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`phase` (`signal`; default `0`)<br>`loop` (`float`; default `1`) | 0 | `available` |
| `BufWr` | `buf_wr_kr` | `kr` / `control` | `inputArray` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`phase` (`signal`; default `0`)<br>`loop` (`float`; default `1`) | 0 | `available` |
| `ClearBuf` | `clear_buf_ir` | `ir` / `scalar` | `buffer` (`float`; default `0`) | 1 | `available` |
| `LocalBuf` | `local_buf_ir` | `ir` / `scalar` | `numChannels` (`float`; default `1`)<br>`numFrames` (`float`; default `1`) | 1 | `available` |
| `MaxLocalBufs` | `max_local_bufs_ir` | `ir` / `scalar` | `numBufs` (`float`; default `0`) | 1 | `available` |
| `PlayBuf` | `play_buf_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`trigger` (`signal`; default `1`)<br>`startPos` (`float`; default `0`)<br>`loop` (`float`; default `0`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `PlayBuf` | `play_buf_kr` | `kr` / `control` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`trigger` (`signal`; default `1`)<br>`startPos` (`float`; default `0`)<br>`loop` (`float`; default `0`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `RecordBuf` | `record_buf_ar` | `ar` / `audio` | `inputArray` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`offset` (`float`; default `0`)<br>`recLevel` (`float`; default `1`)<br>`preLevel` (`float`; default `0`)<br>`run` (`float`; default `1`)<br>`loop` (`float`; default `1`)<br>`trigger` (`signal`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `RecordBuf` | `record_buf_kr` | `kr` / `control` | `inputArray` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`offset` (`float`; default `0`)<br>`recLevel` (`float`; default `1`)<br>`preLevel` (`float`; default `0`)<br>`run` (`float`; default `1`)<br>`loop` (`float`; default `1`)<br>`trigger` (`signal`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `ScopeOut` | `scope_out_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`inputArray` (`signal`; default `0`) | 1 | `available` |
| `ScopeOut` | `scope_out_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`inputArray` (`signal`; default `0`) | 1 | `available` |
| `ScopeOut2` | `scope_out2_ar` | `ar` / `audio` | `scopeNum` (`float`; default `0`)<br>`maxFrames` (`float`; default `4096`)<br>`scopeFrames` (`float`; default `4096`)<br>`inputArray` (`signal`; default `0`) | 1 | `available` |
| `ScopeOut2` | `scope_out2_kr` | `kr` / `control` | `scopeNum` (`float`; default `0`)<br>`maxFrames` (`float`; default `4096`)<br>`scopeFrames` (`float`; default `4096`)<br>`inputArray` (`signal`; default `0`) | 1 | `available` |
| `SetBuf` | `set_buf_ir` | `ir` / `scalar` | `buffer` (`float`; default `0`)<br>`offset` (`float`; default `0`)<br>`values` (`signal`; default `0`) | 1 | `available` |
| `SimpleLoopBuf` | `simple_loop_buf` | `builder` / `audio` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`loopStart` (`float`; default `0`)<br>`loopEnd` (`float`; default `99999`)<br>`trigger` (`signal`; default `0`) | 1 | `documentation_only` — Removed/commented-out upstream UGen; no installed binary registers SimpleLoopBuf. |
| `Warp1` | `warp1_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`pointer` (`signal`; default `0`)<br>`freqScale` (`float`; default `1`)<br>`windowSize` (`float`; default `0.2`)<br>`envbufnum` (`float`; default `-1`)<br>`overlaps` (`float`; default `8`)<br>`windowRandRatio` (`float`; default `0`)<br>`interp` (`float`; default `1`) | 1 | `available` |

## `chaos.json`

Source: [`crates/vibelang-dsp/ugen_manifests/chaos.json`](../../../crates/vibelang-dsp/ugen_manifests/chaos.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `CuspL` | `cusp_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.0`)<br>`b` (`float`; default `1.9`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `CuspN` | `cusp_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.0`)<br>`b` (`float`; default `1.9`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `FBSineC` | `fb_sine_c_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`im` (`float`; default `1`)<br>`fb` (`float`; default `0.1`)<br>`a` (`float`; default `1.1`)<br>`c` (`float`; default `0.5`)<br>`xi` (`float`; default `0.1`)<br>`yi` (`float`; default `0.1`) | 1 | `available` |
| `FBSineL` | `fb_sine_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`im` (`float`; default `1`)<br>`fb` (`float`; default `0.1`)<br>`a` (`float`; default `1.1`)<br>`c` (`float`; default `0.5`)<br>`xi` (`float`; default `0.1`)<br>`yi` (`float`; default `0.1`) | 1 | `available` |
| `FBSineN` | `fb_sine_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`im` (`float`; default `1`)<br>`fb` (`float`; default `0.1`)<br>`a` (`float`; default `1.1`)<br>`c` (`float`; default `0.5`)<br>`xi` (`float`; default `0.1`)<br>`yi` (`float`; default `0.1`) | 1 | `available` |
| `GbmanL` | `gbman_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`xi` (`float`; default `1.2`)<br>`yi` (`float`; default `2.1`) | 1 | `available` |
| `GbmanN` | `gbman_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`xi` (`float`; default `1.2`)<br>`yi` (`float`; default `2.1`) | 1 | `available` |
| `HenonC` | `henon_c_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0`)<br>`x1` (`float`; default `0`) | 1 | `available` |
| `HenonL` | `henon_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0`)<br>`x1` (`float`; default `0`) | 1 | `available` |
| `HenonN` | `henon_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0`)<br>`x1` (`float`; default `0`) | 1 | `available` |
| `LatoocarfianC` | `latoocarfian_c_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`xi` (`float`; default `0.5`)<br>`yi` (`float`; default `0.5`) | 1 | `available` |
| `LatoocarfianL` | `latoocarfian_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`xi` (`float`; default `0.5`)<br>`yi` (`float`; default `0.5`) | 1 | `available` |
| `LatoocarfianN` | `latoocarfian_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`xi` (`float`; default `0.5`)<br>`yi` (`float`; default `0.5`) | 1 | `available` |
| `LinCongC` | `lin_cong_c_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.1`)<br>`c` (`float`; default `0.13`)<br>`m` (`float`; default `1.0`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `LinCongL` | `lin_cong_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.1`)<br>`c` (`float`; default `0.13`)<br>`m` (`float`; default `1.0`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `LinCongN` | `lin_cong_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.1`)<br>`c` (`float`; default `0.13`)<br>`m` (`float`; default `1.0`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `LorenzL` | `lorenz_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.667`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0.1`)<br>`yi` (`float`; default `0`)<br>`zi` (`float`; default `0`) | 1 | `available` |
| `QuadC` | `quad_c_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `-1`)<br>`c` (`float`; default `-0.75`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `QuadL` | `quad_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `-1`)<br>`c` (`float`; default `-0.75`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `QuadN` | `quad_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `-1`)<br>`c` (`float`; default `-0.75`)<br>`xi` (`float`; default `0`) | 1 | `available` |
| `StandardL` | `standard_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`k` (`float`; default `1.0`)<br>`xi` (`float`; default `0.5`)<br>`yi` (`float`; default `0`) | 1 | `available` |
| `StandardN` | `standard_n_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`k` (`float`; default `1.0`)<br>`xi` (`float`; default `0.5`)<br>`yi` (`float`; default `0`) | 1 | `available` |

## `control.json`

Source: [`crates/vibelang-dsp/ugen_manifests/control.json`](../../../crates/vibelang-dsp/ugen_manifests/control.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `KeyState` | `key_state_kr` | `kr` / `control` | `keycode` (`float`; default `0`)<br>`minval` (`float`; default `0`)<br>`maxval` (`float`; default `1`)<br>`lag` (`float`; default `0.2`) | 1 | `available` |
| `LFGauss` | `lf_gauss_ar` | `ar` / `audio` | `duration` (`float`; default `1`)<br>`width` (`float`; default `0.1`)<br>`iphase` (`float`; default `0`)<br>`loop` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `LFGauss` | `lf_gauss_kr` | `kr` / `control` | `duration` (`float`; default `1`)<br>`width` (`float`; default `0.1`)<br>`iphase` (`float`; default `0`)<br>`loop` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `MouseButton` | `mouse_button_kr` | `kr` / `control` | `minval` (`float`; default `0`)<br>`maxval` (`float`; default `1`)<br>`lag` (`float`; default `0.2`) | 1 | `available` |
| `MouseX` | `mouse_x_kr` | `kr` / `control` | `minval` (`float`; default `0`)<br>`maxval` (`float`; default `1`)<br>`warp` (`float`; default `0`)<br>`lag` (`float`; default `0.2`) | 1 | `available` |
| `MouseY` | `mouse_y_kr` | `kr` / `control` | `minval` (`float`; default `0`)<br>`maxval` (`float`; default `1`)<br>`warp` (`float`; default `0`)<br>`lag` (`float`; default `0.2`) | 1 | `available` |
| `Phasor` | `phasor_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`rate` (`float`; default `1`)<br>`start` (`float`; default `0`)<br>`end` (`float`; default `1`)<br>`resetPos` (`float`; default `0`) | 1 | `available` |
| `Phasor` | `phasor_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`rate` (`float`; default `1`)<br>`start` (`float`; default `0`)<br>`end` (`float`; default `1`)<br>`resetPos` (`float`; default `0`) | 1 | `available` |
| `Select` | `select_ar` | `ar` / `audio` | `which` (`float`; default `0`)<br>`array` (`signal`; default `0`) | 1 | `available` |
| `Select` | `select_kr` | `kr` / `control` | `which` (`float`; default `0`)<br>`array` (`signal`; default `0`) | 1 | `available` |
| `SelectX` | `select_x` | `builder` / `audio` | `which` (`float`; default `0`)<br>`array` (`signal`; default `0`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `Slope` | `slope_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `Slope` | `slope_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `Sweep` | `sweep_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`rate` (`float`; default `1`) | 1 | `available` |
| `Sweep` | `sweep_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`rate` (`float`; default `1`) | 1 | `available` |
| `Timer` | `timer_ar` | `ar` / `audio` | `trig` (`signal`; default `0`) | 1 | `available` |
| `Timer` | `timer_kr` | `kr` / `control` | `trig` (`signal`; default `0`) | 1 | `available` |

## `conversion.json`

Source: [`crates/vibelang-dsp/ugen_manifests/conversion.json`](../../../crates/vibelang-dsp/ugen_manifests/conversion.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `A2K` | `a2k_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `DC` | `dc_ar` | `ar` / `audio` | `in` (`float`; default `0`) | 1 | `available` |
| `DC` | `dc_kr` | `kr` / `control` | `in` (`float`; default `0`) | 1 | `available` |
| `K2A` | `k2a_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `Silence` | `silence` | `builder` / `audio` | `numChannels` (`float`; default `1`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `T2A` | `t2a_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`offset` (`float`; default `0`) | 1 | `available` |
| `T2K` | `t2k_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |

## `convolution.json`

Source: [`crates/vibelang-dsp/ugen_manifests/convolution.json`](../../../crates/vibelang-dsp/ugen_manifests/convolution.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Convolution` | `convolution_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`kernel` (`signal`; default `0`)<br>`framesize` (`float`; default `1024`) | 1 | `available` |
| `Convolution2` | `convolution2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`kernel` (`float`; default `0`)<br>`trigger` (`signal`; default `0`)<br>`framesize` (`float`; default `2048`) | 1 | `available` |
| `Convolution2L` | `convolution2l_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`kernel` (`float`; default `0`)<br>`trigger` (`signal`; default `0`)<br>`framesize` (`float`; default `2048`)<br>`crossfade` (`float`; default `1`) | 1 | `available` |
| `Convolution3` | `convolution3_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`kernel` (`float`; default `0`)<br>`trigger` (`signal`; default `0`)<br>`framesize` (`float`; default `2048`) | 1 | `available` |
| `Convolution3` | `convolution3_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`kernel` (`float`; default `0`)<br>`trigger` (`signal`; default `0`)<br>`framesize` (`float`; default `2048`) | 1 | `available` |
| `PartConv` | `part_conv_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`fftsize` (`float`; default `2048`)<br>`irbufnum` (`float`; default `0`) | 1 | `available` |
| `StereoConvolution2L` | `stereo_convolution2l_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`kernelL` (`float`; default `0`)<br>`kernelR` (`float`; default `0`)<br>`trigger` (`signal`; default `0`)<br>`framesize` (`float`; default `2048`)<br>`crossfade` (`float`; default `1`) | 2 | `available` |

## `delays.json`

Source: [`crates/vibelang-dsp/ugen_manifests/delays.json`](../../../crates/vibelang-dsp/ugen_manifests/delays.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AllpassC` | `allpass_c_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `AllpassC` | `allpass_c_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `AllpassL` | `allpass_l_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `AllpassL` | `allpass_l_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `AllpassN` | `allpass_n_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `AllpassN` | `allpass_n_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `CombC` | `comb_c_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `CombC` | `comb_c_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `CombL` | `comb_l_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `CombL` | `comb_l_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `CombN` | `comb_n_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `CombN` | `comb_n_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `DelTapRd` | `del_tap_rd_ar` | `ar` / `audio` | `buffer` (`float`; default `0`)<br>`phase` (`signal`; default `0`)<br>`delTime` (`float`; default `0.2`)<br>`interp` (`float`; default `1`) | 1 | `available` |
| `DelTapRd` | `del_tap_rd_kr` | `kr` / `control` | `buffer` (`float`; default `0`)<br>`phase` (`signal`; default `0`)<br>`delTime` (`float`; default `0.2`)<br>`interp` (`float`; default `1`) | 1 | `available` |
| `DelTapWr` | `del_tap_wr_ar` | `ar` / `audio` | `buffer` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `DelTapWr` | `del_tap_wr_kr` | `kr` / `control` | `buffer` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `Delay1` | `delay1_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `Delay1` | `delay1_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `Delay2` | `delay2_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `Delay2` | `delay2_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `DelayC` | `delay_c_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `DelayC` | `delay_c_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `DelayL` | `delay_l_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `DelayL` | `delay_l_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `DelayN` | `delay_n_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `DelayN` | `delay_n_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`) | 1 | `available` |
| `GrainTap` | `grain_tap_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`grainDur` (`float`; default `0.2`)<br>`pchRatio` (`float`; default `1`)<br>`pchDispersion` (`float`; default `0`)<br>`timeDispersion` (`float`; default `0`)<br>`overlap` (`float`; default `2`) | 1 | `available` |

## `demand.json`

Source: [`crates/vibelang-dsp/ugen_manifests/demand.json`](../../../crates/vibelang-dsp/ugen_manifests/demand.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Dbrown` | `dbrown_demand` | `demand` / `unavailable` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`)<br>`step` (`float`; default `0.01`)<br>`length` (`float`; default `100000000`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dbufrd` | `dbufrd_demand` | `demand` / `unavailable` | `bufnum` (`signal`; default `0`)<br>`phase` (`signal`; default `0`)<br>`loop` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dbufwr` | `dbufwr_demand` | `demand` / `unavailable` | `input` (`signal`; default `0`)<br>`bufnum` (`signal`; default `0`)<br>`phase` (`signal`; default `0`)<br>`loop` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dconst` | `dconst_demand` | `demand` / `unavailable` | `sum` (`signal`; default `0`)<br>`in` (`signal`; default `0`)<br>`tolerance` (`float`; default `0.001`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Ddup` | `ddup_demand` | `demand` / `unavailable` | `n` (`signal`; default `2`)<br>`in` (`signal`; default `0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Demand` | `demand_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`)<br>`demandUGens` (`signal`; default `0`) | 1 | `available` |
| `Demand` | `demand_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`)<br>`demandUGens` (`signal`; default `0`) | 1 | `available` |
| `DemandEnvGen` | `demand_env_gen_ar` | `ar` / `audio` | `level` (`signal`; default `0`)<br>`dur` (`signal`; default `1`)<br>`shape` (`signal`; default `1`)<br>`curve` (`signal`; default `0`)<br>`gate` (`signal`; default `1`)<br>`reset` (`signal`; default `1`)<br>`levelScale` (`signal`; default `1`)<br>`levelBias` (`signal`; default `0`)<br>`timeScale` (`signal`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `DemandEnvGen` | `demand_env_gen_kr` | `kr` / `control` | `level` (`signal`; default `0`)<br>`dur` (`signal`; default `1`)<br>`shape` (`signal`; default `1`)<br>`curve` (`signal`; default `0`)<br>`gate` (`signal`; default `1`)<br>`reset` (`signal`; default `1`)<br>`levelScale` (`signal`; default `1`)<br>`levelBias` (`signal`; default `0`)<br>`timeScale` (`signal`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `Dgeom` | `dgeom_demand` | `demand` / `unavailable` | `start` (`float`; default `1`)<br>`grow` (`float`; default `2`)<br>`length` (`float`; default `100000000`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dibrown` | `dibrown_demand` | `demand` / `unavailable` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `12`)<br>`step` (`float`; default `1`)<br>`length` (`float`; default `100000000`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Diwhite` | `diwhite_demand` | `demand` / `unavailable` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`)<br>`length` (`float`; default `100000000`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dpoll` | `dpoll_demand` | `demand` / `unavailable` | `in` (`signal`; default `0`)<br>`label` (`signal`; default `0`)<br>`run` (`signal`; default `1`)<br>`trigid` (`float`; default `-1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Drand` | `drand_demand` | `demand` / `unavailable` | `array` (`signal`; default `0`)<br>`repeats` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dreset` | `dreset_demand` | `demand` / `unavailable` | `in` (`signal`; default `0`)<br>`reset` (`signal`; default `0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dseq` | `dseq_demand` | `demand` / `unavailable` | `array` (`signal`; default `0`)<br>`repeats` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dser` | `dser_demand` | `demand` / `unavailable` | `array` (`signal`; default `0`)<br>`repeats` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dseries` | `dseries_demand` | `demand` / `unavailable` | `start` (`float`; default `1`)<br>`step` (`float`; default `1`)<br>`length` (`float`; default `100000000`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dshuf` | `dshuf_demand` | `demand` / `unavailable` | `list` (`signal`; default `0`)<br>`repeats` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dstutter` | `dstutter_demand` | `demand` / `unavailable` | `n` (`signal`; default `2`)<br>`in` (`signal`; default `0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dswitch` | `dswitch_demand` | `demand` / `unavailable` | `list` (`signal`; default `0`)<br>`index` (`signal`; default `0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dswitch1` | `dswitch1_demand` | `demand` / `unavailable` | `list` (`signal`; default `0`)<br>`index` (`signal`; default `0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Duty` | `duty_ar` | `ar` / `audio` | `dur` (`signal`; default `1`)<br>`reset` (`signal`; default `0`)<br>`doneAction` (`float`; default `0`)<br>`level` (`signal`; default `1`) | 1 | `available` |
| `Duty` | `duty_kr` | `kr` / `control` | `dur` (`signal`; default `1`)<br>`reset` (`signal`; default `0`)<br>`doneAction` (`float`; default `0`)<br>`level` (`signal`; default `1`) | 1 | `available` |
| `Dwhite` | `dwhite_demand` | `demand` / `unavailable` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`)<br>`length` (`float`; default `100000000`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dwrand` | `dwrand_demand` | `demand` / `unavailable` | `list` (`signal`; default `0`)<br>`weights` (`signal`; default `0`)<br>`repeats` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dxrand` | `dxrand_demand` | `demand` / `unavailable` | `array` (`signal`; default `0`)<br>`repeats` (`float`; default `1`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `TDuty` | `t_duty_ar` | `ar` / `audio` | `dur` (`signal`; default `1`)<br>`reset` (`signal`; default `0`)<br>`doneAction` (`float`; default `0`)<br>`level` (`signal`; default `1`)<br>`gapFirst` (`float`; default `0`) | 1 | `available` |
| `TDuty` | `t_duty_kr` | `kr` / `control` | `dur` (`signal`; default `1`)<br>`reset` (`signal`; default `0`)<br>`doneAction` (`float`; default `0`)<br>`level` (`signal`; default `1`)<br>`gapFirst` (`float`; default `0`) | 1 | `available` |

## `disk_io.json`

Source: [`crates/vibelang-dsp/ugen_manifests/disk_io.json`](../../../crates/vibelang-dsp/ugen_manifests/disk_io.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `DiskIn` | `disk_in_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`loop` (`float`; default `0`) | 1 | `available` |
| `DiskOut` | `disk_out_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 1 | `available` |
| `VDiskIn` | `v_disk_in_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`rate` (`signal`; default `1`)<br>`loop` (`float`; default `0`)<br>`sendID` (`float`; default `0`) | 1 | `available` |

## `dynamics.json`

Source: [`crates/vibelang-dsp/ugen_manifests/dynamics.json`](../../../crates/vibelang-dsp/ugen_manifests/dynamics.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Amplitude` | `amplitude_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`attackTime` (`float`; default `0.01`)<br>`releaseTime` (`float`; default `0.01`) | 1 | `available` |
| `Amplitude` | `amplitude_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`attackTime` (`float`; default `0.01`)<br>`releaseTime` (`float`; default `0.01`) | 1 | `available` |
| `Compander` | `compander_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`control` (`signal`; default `0`)<br>`thresh` (`float`; default `0.5`)<br>`slopeBelow` (`float`; default `1`)<br>`slopeAbove` (`float`; default `1`)<br>`clampTime` (`float`; default `0.01`)<br>`relaxTime` (`float`; default `0.1`) | 1 | `available` |
| `Limiter` | `limiter_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`level` (`float`; default `1`)<br>`dur` (`float`; default `0.01`) | 1 | `available` |
| `Normalizer` | `normalizer_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`level` (`float`; default `1`)<br>`dur` (`float`; default `0.01`) | 1 | `available` |

## `envelopes.json`

Source: [`crates/vibelang-dsp/ugen_manifests/envelopes.json`](../../../crates/vibelang-dsp/ugen_manifests/envelopes.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Decay` | `decay_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`decayTime` (`float`; default `1`) | 1 | `available` |
| `Decay` | `decay_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`decayTime` (`float`; default `1`) | 1 | `available` |
| `Decay2` | `decay2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`attackTime` (`float`; default `0.01`)<br>`decayTime` (`float`; default `1`) | 1 | `available` |
| `Decay2` | `decay2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`attackTime` (`float`; default `0.01`)<br>`decayTime` (`float`; default `1`) | 1 | `available` |
| `EnvGen` | `env_gen_ar` | `ar` / `audio` | `gate` (`float`; default `1`)<br>`levelScale` (`float`; default `1`)<br>`levelBias` (`float`; default `0`)<br>`timeScale` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `EnvGen` | `env_gen_kr` | `kr` / `control` | `gate` (`float`; default `1`)<br>`levelScale` (`float`; default `1`)<br>`levelBias` (`float`; default `0`)<br>`timeScale` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `IEnvGen` | `i_env_gen_ar` | `ar` / `audio` | `index` (`signal`; default `0`)<br>`mul` (`float`; default `1`)<br>`add` (`float`; default `0`) | 1 | `available` |
| `IEnvGen` | `i_env_gen_kr` | `kr` / `control` | `index` (`signal`; default `0`)<br>`mul` (`float`; default `1`)<br>`add` (`float`; default `0`) | 1 | `available` |
| `Lag` | `lag_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `Lag` | `lag_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `LagUD` | `lag_ud_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTimeU` (`float`; default `0.1`)<br>`lagTimeD` (`float`; default `0.1`) | 1 | `available` |
| `LagUD` | `lag_ud_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTimeU` (`float`; default `0.1`)<br>`lagTimeD` (`float`; default `0.1`) | 1 | `available` |
| `Line` | `line_ar` | `ar` / `audio` | `start` (`float`; default `0`)<br>`end` (`float`; default `1`)<br>`dur` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `Line` | `line_kr` | `kr` / `control` | `start` (`float`; default `0`)<br>`end` (`float`; default `1`)<br>`dur` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `Linen` | `linen_kr` | `kr` / `control` | `gate` (`float`; default `1`)<br>`attackTime` (`float`; default `0.01`)<br>`susLevel` (`float`; default `1`)<br>`releaseTime` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `VarLag` | `var_lag_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`time` (`float`; default `0.1`)<br>`curvature` (`float`; default `0`)<br>`warp` (`float`; default `5`)<br>`start` (`float`; default `0`) | 1 | `available` |
| `VarLag` | `var_lag_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`time` (`float`; default `0.1`)<br>`curvature` (`float`; default `0`)<br>`warp` (`float`; default `5`)<br>`start` (`float`; default `0`) | 1 | `available` |
| `XLine` | `x_line_ar` | `ar` / `audio` | `start` (`float`; default `1`)<br>`end` (`float`; default `2`)<br>`dur` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `XLine` | `x_line_kr` | `kr` / `control` | `start` (`float`; default `1`)<br>`end` (`float`; default `2`)<br>`dur` (`float`; default `1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `envelope` | `envelope` | `builder` / `audio` | `.attack(time)` (`method`; default `0`)<br>`.decay(time)` (`method`; default `0`)<br>`.sustain(level)` (`method`; default `0`)<br>`.release(time)` (`method`; default `0`)<br>`.perc(attack, release)` (`method`; default `0`)<br>`.asr(attack, sustain, release)` (`method`; default `0`)<br>`.adsr(attack, decay, sustain, release)` (`method`; default `0`)<br>`.triangle(duration)` (`method`; default `0`)<br>`.gate(signal)` (`method`; default `"dc_ar(1.0)"`)<br>`.time_scale(factor)` (`method`; default `"1.0"`)<br>`.level_scale(factor)` (`method`; default `"1.0"`)<br>`.level_bias(offset)` (`method`; default `"0.0"`)<br>`.cleanup_on_finish()` (`method`; default `0`)<br>`.done_action(value)` (`method`; default `"0"`)<br>`.build()` (`method`; default `0`) | 1 | `documentation_only` — VibeLang fluent envelope builder; no literal server UGen named envelope is emitted. |

## `fft.json`

Source: [`crates/vibelang-dsp/ugen_manifests/fft.json`](../../../crates/vibelang-dsp/ugen_manifests/fft.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `FFT` | `fft_kr` | `kr` / `control` | `buffer` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`hop` (`float`; default `0.5`)<br>`wintype` (`float`; default `0`)<br>`active` (`float`; default `1`)<br>`winsize` (`float`; default `0`) | 1 | `available` |
| `IFFT` | `ifft_ar` | `ar` / `audio` | `buffer` (`signal`; default `0`)<br>`wintype` (`float`; default `0`)<br>`winsize` (`float`; default `0`) | 1 | `available` |

## `filters.json`

Source: [`crates/vibelang-dsp/ugen_manifests/filters.json`](../../../crates/vibelang-dsp/ugen_manifests/filters.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `APF` | `apf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`radius` (`float`; default `0.8`) | 1 | `available` |
| `APF` | `apf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`radius` (`float`; default `0.8`) | 1 | `available` |
| `BAllPass` | `b_all_pass_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BAllPass` | `b_all_pass_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BBandPass` | `b_band_pass_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`bw` (`float`; default `1`) | 1 | `available` |
| `BBandPass` | `b_band_pass_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`bw` (`float`; default `1`) | 1 | `available` |
| `BBandStop` | `b_band_stop_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`bw` (`float`; default `1`) | 1 | `available` |
| `BBandStop` | `b_band_stop_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`bw` (`float`; default `1`) | 1 | `available` |
| `BHiPass` | `b_hi_pass_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BHiPass` | `b_hi_pass_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BHiPass4` | `b_hi_pass4` | `builder` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `BHiShelf` | `b_hi_shelf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rs` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `BHiShelf` | `b_hi_shelf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rs` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `BLowPass` | `b_low_pass_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BLowPass` | `b_low_pass_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BLowPass4` | `b_low_pass4` | `builder` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `BLowShelf` | `b_low_shelf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rs` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `BLowShelf` | `b_low_shelf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rs` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `BPF` | `bpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BPF` | `bpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BPZ2` | `bpz2_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `BPZ2` | `bpz2_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `BPeakEQ` | `b_peak_eq_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `BPeakEQ` | `b_peak_eq_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200`)<br>`rq` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `BRF` | `brf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BRF` | `brf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `BRZ2` | `brz2_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `BRZ2` | `brz2_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `DetectSilence` | `detect_silence_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`amp` (`float`; default `0.0001`)<br>`time` (`float`; default `0.1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `DetectSilence` | `detect_silence_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`amp` (`float`; default `0.0001`)<br>`time` (`float`; default `0.1`)<br>`doneAction` (`float`; default `0`) | 1 | `available` |
| `FOS` | `fos_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`a0` (`float`; default `0`)<br>`a1` (`float`; default `0`)<br>`b1` (`float`; default `0`) | 1 | `available` |
| `FOS` | `fos_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`a0` (`float`; default `0`)<br>`a1` (`float`; default `0`)<br>`b1` (`float`; default `0`) | 1 | `available` |
| `Flip` | `flip_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `Formlet` | `formlet_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`attacktime` (`float`; default `1`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `Formlet` | `formlet_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`attacktime` (`float`; default `1`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `FreqShift` | `freq_shift_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `0`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `HPF` | `hpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`) | 1 | `available` |
| `HPF` | `hpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`) | 1 | `available` |
| `HPZ1` | `hpz1_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `HPZ1` | `hpz1_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `HPZ2` | `hpz2_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `HPZ2` | `hpz2_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `Hilbert` | `hilbert_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 2 | `available` |
| `Integrator` | `integrator_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `1`) | 1 | `available` |
| `Integrator` | `integrator_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `1`) | 1 | `available` |
| `LPF` | `lpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`) | 1 | `available` |
| `LPF` | `lpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`) | 1 | `available` |
| `LPZ1` | `lpz1_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `LPZ1` | `lpz1_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `LPZ2` | `lpz2_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `LPZ2` | `lpz2_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `Lag2` | `lag2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `Lag2` | `lag2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `Lag2UD` | `lag2ud_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTimeU` (`float`; default `0.1`)<br>`lagTimeD` (`float`; default `0.1`) | 1 | `available` |
| `Lag2UD` | `lag2ud_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTimeU` (`float`; default `0.1`)<br>`lagTimeD` (`float`; default `0.1`) | 1 | `available` |
| `Lag3` | `lag3_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `Lag3` | `lag3_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `Lag3UD` | `lag3ud_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTimeU` (`float`; default `0.1`)<br>`lagTimeD` (`float`; default `0.1`) | 1 | `available` |
| `Lag3UD` | `lag3ud_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTimeU` (`float`; default `0.1`)<br>`lagTimeD` (`float`; default `0.1`) | 1 | `available` |
| `LeakDC` | `leak_dc_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `0.995`) | 1 | `available` |
| `LeakDC` | `leak_dc_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `0.995`) | 1 | `available` |
| `Median` | `median_ar` | `ar` / `audio` | `length` (`float`; default `3`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `Median` | `median_kr` | `kr` / `control` | `length` (`float`; default `3`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `MidEQ` | `mid_eq_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `MidEQ` | `mid_eq_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`)<br>`db` (`float`; default `0`) | 1 | `available` |
| `MoogFF` | `moog_ff_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `100`)<br>`gain` (`float`; default `2`)<br>`reset` (`float`; default `0`) | 1 | `available` |
| `MoogFF` | `moog_ff_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `100`)<br>`gain` (`float`; default `2`)<br>`reset` (`float`; default `0`) | 1 | `available` |
| `OnePole` | `one_pole_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `0.5`) | 1 | `available` |
| `OnePole` | `one_pole_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `0.5`) | 1 | `available` |
| `OneZero` | `one_zero_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `0.5`) | 1 | `available` |
| `OneZero` | `one_zero_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`coef` (`float`; default `0.5`) | 1 | `available` |
| `RHPF` | `rhpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `RHPF` | `rhpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `RLPF` | `rlpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `RLPF` | `rlpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `Ramp` | `ramp_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `Ramp` | `ramp_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`) | 1 | `available` |
| `Resonz` | `resonz_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`bwr` (`float`; default `1`) | 1 | `available` |
| `Resonz` | `resonz_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`bwr` (`float`; default `1`) | 1 | `available` |
| `Ringz` | `ringz_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `Ringz` | `ringz_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`decaytime` (`float`; default `1`) | 1 | `available` |
| `SOS` | `sos_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`a0` (`float`; default `0`)<br>`a1` (`float`; default `0`)<br>`a2` (`float`; default `0`)<br>`b1` (`float`; default `0`)<br>`b2` (`float`; default `0`) | 1 | `available` |
| `SOS` | `sos_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`a0` (`float`; default `0`)<br>`a1` (`float`; default `0`)<br>`a2` (`float`; default `0`)<br>`b1` (`float`; default `0`)<br>`b2` (`float`; default `0`) | 1 | `available` |
| `Slew` | `slew_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`up` (`float`; default `1`)<br>`dn` (`float`; default `1`) | 1 | `available` |
| `Slew` | `slew_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`up` (`float`; default `1`)<br>`dn` (`float`; default `1`) | 1 | `available` |
| `TwoPole` | `two_pole_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`radius` (`float`; default `0.8`) | 1 | `available` |
| `TwoPole` | `two_pole_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`radius` (`float`; default `0.8`) | 1 | `available` |
| `TwoZero` | `two_zero_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`radius` (`float`; default `0.8`) | 1 | `available` |
| `TwoZero` | `two_zero_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`radius` (`float`; default `0.8`) | 1 | `available` |

## `granular.json`

Source: [`crates/vibelang-dsp/ugen_manifests/granular.json`](../../../crates/vibelang-dsp/ugen_manifests/granular.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `GrainBuf` | `grain_buf_ar` | `ar` / `audio` | `numChannels` (`float`; default `2`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`) | 2 | `available` |
| `GrainFM` | `grain_fm_ar` | `ar` / `audio` | `numChannels` (`float`; default `2`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`) | 2 | `available` |
| `GrainIn` | `grain_in_ar` | `ar` / `audio` | `numChannels` (`float`; default `2`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`) | 2 | `available` |
| `GrainSin` | `grain_sin_ar` | `ar` / `audio` | `numChannels` (`float`; default `2`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`) | 2 | `available` |
| `TGrains` | `t_grains_ar` | `ar` / `audio` | `numChannels` (`float`; default `2`)<br>`trigger` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`centerPos` (`float`; default `0`)<br>`dur` (`float`; default `0.1`)<br>`pan` (`float`; default `0`)<br>`amp` (`float`; default `0.1`)<br>`interp` (`float`; default `4`) | 2 | `available` |

## `info.json`

Source: [`crates/vibelang-dsp/ugen_manifests/info.json`](../../../crates/vibelang-dsp/ugen_manifests/info.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BlockSize` | `block_size_ir` | `ir` / `scalar` | none | 1 | `available` |
| `ControlDur` | `control_dur_ir` | `ir` / `scalar` | none | 1 | `available` |
| `ControlRate` | `control_rate_ir` | `ir` / `scalar` | none | 1 | `available` |
| `NodeID` | `node_id_ir` | `ir` / `scalar` | none | 1 | `available` |
| `NumAudioBuses` | `num_audio_buses_ir` | `ir` / `scalar` | none | 1 | `available` |
| `NumBuffers` | `num_buffers_ir` | `ir` / `scalar` | none | 1 | `available` |
| `NumControlBuses` | `num_control_buses_ir` | `ir` / `scalar` | none | 1 | `available` |
| `NumInputBuses` | `num_input_buses_ir` | `ir` / `scalar` | none | 1 | `available` |
| `NumOutputBuses` | `num_output_buses_ir` | `ir` / `scalar` | none | 1 | `available` |
| `NumRunningSynths` | `num_running_synths_kr` | `kr` / `control` | none | 1 | `available` |
| `RadiansPerSample` | `radians_per_sample_ir` | `ir` / `scalar` | none | 1 | `available` |
| `SampleDur` | `sample_dur_ir` | `ir` / `scalar` | none | 1 | `available` |
| `SampleRate` | `sample_rate_ir` | `ir` / `scalar` | none | 1 | `available` |
| `SubsampleOffset` | `subsample_offset_ir` | `ir` / `scalar` | none | 1 | `available` |

## `inout.json`

Source: [`crates/vibelang-dsp/ugen_manifests/inout.json`](../../../crates/vibelang-dsp/ugen_manifests/inout.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `In` | `in_ar` | `ar` / `audio` | `bus` (`float`; default `0`)<br>`numChannels` (`float`; default `1`) | 1 | `available` |
| `In` | `in_kr` | `kr` / `control` | `bus` (`float`; default `0`)<br>`numChannels` (`float`; default `1`) | 1 | `available` |
| `InFeedback` | `in_feedback_ar` | `ar` / `audio` | `bus` (`float`; default `0`)<br>`numChannels` (`float`; default `1`) | 1 | `available` |
| `LocalIn` | `local_in_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`default` (`signal`; default `0`) | 1 | `available` |
| `LocalIn` | `local_in_kr` | `kr` / `control` | `numChannels` (`float`; default `1`)<br>`default` (`signal`; default `0`) | 1 | `available` |
| `LocalOut` | `local_out_ar` | `ar` / `audio` | `channelsArray` (`signal`; default `0`) | 0 | `available` |
| `LocalOut` | `local_out_kr` | `kr` / `control` | `channelsArray` (`signal`; default `0`) | 0 | `available` |
| `OffsetOut` | `offset_out_ar` | `ar` / `audio` | `bus` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 0 | `available` |
| `Out` | `out_ar` | `ar` / `audio` | `bus` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 0 | `available` |
| `Out` | `out_kr` | `kr` / `control` | `bus` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 0 | `available` |
| `ReplaceOut` | `replace_out_ar` | `ar` / `audio` | `bus` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 0 | `available` |
| `ReplaceOut` | `replace_out_kr` | `kr` / `control` | `bus` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 0 | `available` |
| `SoundIn` | `sound_in` | `builder` / `audio` | `channel` (`float`; default `0`) | 1 | `documentation_only` — Sclang input helper; VibeLang provides manual lowering through sound_in_ar/sound_in_channel. |
| `XOut` | `x_out_ar` | `ar` / `audio` | `bus` (`float`; default `0`)<br>`xfade` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 0 | `available` |
| `XOut` | `x_out_kr` | `kr` / `control` | `bus` (`float`; default `0`)<br>`xfade` (`float`; default `0`)<br>`channelsArray` (`signal`; default `0`) | 0 | `available` |

## `link.json`

Source: [`crates/vibelang-dsp/ugen_manifests/link.json`](../../../crates/vibelang-dsp/ugen_manifests/link.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `LinkJump` | `link_jump` | `builder` / `audio` | `gate` (`float`; default `0`)<br>`beat` (`float`; default `0`)<br>`quantum` (`float`; default `1`)<br>`force` (`float`; default `0`) | 1 | `documentation_only` — Ableton Link UGen plugin/source not installed or verified on this host. |
| `LinkPhase` | `link_phase` | `builder` / `audio` | `quantum` (`float`; default `1`) | 1 | `documentation_only` — Ableton Link UGen plugin/source not installed or verified on this host. |
| `LinkTempo` | `link_tempo` | `builder` / `audio` | `gate` (`float`; default `0`)<br>`tempo` (`float`; default `1`) | 1 | `documentation_only` — Ableton Link UGen plugin/source not installed or verified on this host. |

## `math.json`

Source: [`crates/vibelang-dsp/ugen_manifests/math.json`](../../../crates/vibelang-dsp/ugen_manifests/math.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AMClip` | `am_clip_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `AMClip` | `am_clip_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `AbsDif` | `abs_dif_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `AbsDif` | `abs_dif_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Atan2` | `atan2_ar` | `ar` / `audio` | `y` (`signal`; default `0`)<br>`x` (`signal`; default `0`) | 1 | `available` |
| `Atan2` | `atan2_kr` | `kr` / `control` | `y` (`signal`; default `0`)<br>`x` (`signal`; default `0`) | 1 | `available` |
| `Clip` | `clip_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Clip` | `clip_ir` | `ir` / `scalar` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Clip` | `clip_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Clip2` | `clip2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |
| `Clip2` | `clip2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |
| `DifSqr` | `dif_sqr_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `DifSqr` | `dif_sqr_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Excess` | `excess_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |
| `Excess` | `excess_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |
| `ExpExp` | `exp_exp` | `builder` / `audio` | `in` (`signal`; default `1`)<br>`srclo` (`float`; default `0.01`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `0.01`)<br>`dsthi` (`float`; default `1`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `ExpLin` | `exp_lin` | `builder` / `audio` | `in` (`signal`; default `1`)<br>`srclo` (`float`; default `0.01`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `0`)<br>`dsthi` (`float`; default `1`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `FirstArg` | `first_arg_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `FirstArg` | `first_arg_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Fold` | `fold_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Fold` | `fold_ir` | `ir` / `scalar` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Fold` | `fold_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Fold2` | `fold2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |
| `Fold2` | `fold2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |
| `Hypot` | `hypot_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Hypot` | `hypot_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `HypotApx` | `hypot_apx_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `HypotApx` | `hypot_apx_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `LinExp` | `lin_exp_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`srclo` (`float`; default `0`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `1`)<br>`dsthi` (`float`; default `2`) | 1 | `available` |
| `LinExp` | `lin_exp_ir` | `ir` / `scalar` | `in` (`signal`; default `0`)<br>`srclo` (`float`; default `0`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `1`)<br>`dsthi` (`float`; default `2`) | 1 | `available` |
| `LinExp` | `lin_exp_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`srclo` (`float`; default `0`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `1`)<br>`dsthi` (`float`; default `2`) | 1 | `available` |
| `LinLin` | `lin_lin_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`srclo` (`float`; default `0`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `1`)<br>`dsthi` (`float`; default `2`) | 1 | `available` |
| `LinLin` | `lin_lin_ir` | `ir` / `scalar` | `in` (`signal`; default `0`)<br>`srclo` (`float`; default `0`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `1`)<br>`dsthi` (`float`; default `2`) | 1 | `available` |
| `LinLin` | `lin_lin_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`srclo` (`float`; default `0`)<br>`srchi` (`float`; default `1`)<br>`dstlo` (`float`; default `1`)<br>`dsthi` (`float`; default `2`) | 1 | `available` |
| `MulAdd` | `mul_add_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`mul` (`float`; default `1`)<br>`add` (`float`; default `0`) | 1 | `available` |
| `MulAdd` | `mul_add_ir` | `ir` / `scalar` | `in` (`signal`; default `0`)<br>`mul` (`float`; default `1`)<br>`add` (`float`; default `0`) | 1 | `available` |
| `MulAdd` | `mul_add_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`mul` (`float`; default `1`)<br>`add` (`float`; default `0`) | 1 | `available` |
| `Ring1` | `ring1_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Ring1` | `ring1_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Ring2` | `ring2_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Ring2` | `ring2_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Ring3` | `ring3_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Ring3` | `ring3_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Ring4` | `ring4_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Ring4` | `ring4_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `ScaleNeg` | `scale_neg_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`scale` (`float`; default `1`) | 1 | `available` |
| `ScaleNeg` | `scale_neg_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`scale` (`float`; default `1`) | 1 | `available` |
| `SqrDif` | `sqr_dif_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `SqrDif` | `sqr_dif_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `SqrSum` | `sqr_sum_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `SqrSum` | `sqr_sum_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Sum3` | `sum3_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`)<br>`c` (`signal`; default `0`) | 1 | `available` |
| `Sum3` | `sum3_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`)<br>`c` (`signal`; default `0`) | 1 | `available` |
| `Sum4` | `sum4_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`)<br>`c` (`signal`; default `0`)<br>`d` (`signal`; default `0`) | 1 | `available` |
| `Sum4` | `sum4_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`)<br>`c` (`signal`; default `0`)<br>`d` (`signal`; default `0`) | 1 | `available` |
| `SumSqr` | `sum_sqr_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `SumSqr` | `sum_sqr_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Tanh` | `tanh_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `Tanh` | `tanh_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `Thresh` | `thresh_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`thresh` (`float`; default `0`) | 1 | `available` |
| `Thresh` | `thresh_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`thresh` (`float`; default `0`) | 1 | `available` |
| `Wrap` | `wrap_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Wrap` | `wrap_ir` | `ir` / `scalar` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Wrap` | `wrap_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Wrap2` | `wrap2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |
| `Wrap2` | `wrap2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`max` (`float`; default `1`) | 1 | `available` |

## `mi_ugens.json`

Source: [`crates/vibelang-dsp/ugen_manifests/mi_ugens.json`](../../../crates/vibelang-dsp/ugen_manifests/mi_ugens.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `MiBraids` | `mi_braids_ar` | `ar` / `audio` | `pitch` (`float`; default `60`)<br>`timbre` (`float`; default `0.5`)<br>`color` (`float`; default `0.5`)<br>`model` (`int`; default `0`)<br>`trig` (`signal`; default `0`)<br>`resamp` (`int`; default `0`)<br>`decim` (`int`; default `1`)<br>`bits` (`int`; default `0`)<br>`ws` (`float`; default `0`)<br>`mul` (`float`; default `1`) | 1 | `conditional` — plugin: mi-UGens |
| `MiClouds` | `mi_clouds_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`pit` (`float`; default `0`)<br>`pos` (`float`; default `0`)<br>`size` (`float`; default `0.5`)<br>`dens` (`float`; default `0.5`)<br>`tex` (`float`; default `0.5`)<br>`drywet` (`float`; default `0.5`)<br>`in_gain` (`float`; default `1`)<br>`spread` (`float`; default `0`)<br>`rvb` (`float`; default `0`)<br>`fb` (`float`; default `0`)<br>`freeze` (`int`; default `0`)<br>`mode` (`int`; default `0`)<br>`lofi` (`int`; default `0`)<br>`trig` (`signal`; default `0`) | 2 | `conditional` — plugin: mi-UGens |
| `MiElements` | `mi_elements_ar` | `ar` / `audio` | `blow_in` (`signal`; default `0`)<br>`strike_in` (`signal`; default `0`)<br>`gate` (`signal`; default `0`)<br>`pit` (`float`; default `60`)<br>`strength` (`float`; default `0.5`)<br>`contour` (`float`; default `0.5`)<br>`bow_level` (`float`; default `0`)<br>`blow_level` (`float`; default `0`)<br>`strike_level` (`float`; default `0.5`)<br>`flow` (`float`; default `0.5`)<br>`mallet` (`float`; default `0.5`)<br>`bow_timb` (`float`; default `0.5`)<br>`blow_timb` (`float`; default `0.5`)<br>`strike_timb` (`float`; default `0.5`)<br>`geom` (`float`; default `0.5`)<br>`bright` (`float`; default `0.5`)<br>`damp` (`float`; default `0.7`)<br>`pos` (`float`; default `0.25`)<br>`space` (`float`; default `0.3`)<br>`model` (`int`; default `0`) | 2 | `conditional` — plugin: mi-UGens |
| `MiGrids` | `mi_grids_ar` | `ar` / `audio` | `on_off` (`int`; default `1`)<br>`bpm` (`float`; default `120`)<br>`map_x` (`float`; default `0.5`)<br>`map_y` (`float`; default `0.5`)<br>`chaos` (`float`; default `0`)<br>`bd_dens` (`float`; default `0.5`)<br>`sd_dens` (`float`; default `0.5`)<br>`hh_dens` (`float`; default `0.5`)<br>`clock_trig` (`signal`; default `0`)<br>`reset_trig` (`signal`; default `0`)<br>`ext_clock` (`int`; default `0`)<br>`mode` (`int`; default `0`)<br>`swing` (`int`; default `0`)<br>`config` (`int`; default `0`)<br>`reso` (`int`; default `2`) | 6 | `conditional` — plugin: mi-UGens |
| `MiMu` | `mi_mu_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`gain` (`float`; default `1`)<br>`bypass` (`int`; default `0`) | 1 | `conditional` — plugin: mi-UGens |
| `MiOmi` | `mi_omi_ar` | `ar` / `audio` | `audio_in` (`signal`; default `0`)<br>`gate` (`signal`; default `0`)<br>`pit` (`float`; default `60`)<br>`contour` (`float`; default `0.5`)<br>`detune` (`float`; default `0`)<br>`level1` (`float`; default `1`)<br>`level2` (`float`; default `0.5`)<br>`ratio1` (`float`; default `0.5`)<br>`ratio2` (`float`; default `0.5`)<br>`fm1` (`float`; default `0.5`)<br>`fm2` (`float`; default `0.5`)<br>`fb` (`float`; default `0`)<br>`xfb` (`float`; default `0`)<br>`filter_mode` (`float`; default `0`)<br>`cutoff` (`float`; default `0.8`)<br>`reson` (`float`; default `0.3`)<br>`strength` (`float`; default `0.5`)<br>`env` (`float`; default `0.5`)<br>`rotate` (`float`; default `0`)<br>`space` (`float`; default `0.3`) | 2 | `conditional` — plugin: mi-UGens |
| `MiPlaits` | `mi_plaits_ar` | `ar` / `audio` | `pitch` (`float`; default `60`)<br>`engine` (`int`; default `0`)<br>`harm` (`float`; default `0.5`)<br>`timbre` (`float`; default `0.5`)<br>`morph` (`float`; default `0.5`)<br>`trigger` (`signal`; default `0`)<br>`level` (`float`; default `1`)<br>`fm_mod` (`float`; default `0`)<br>`timb_mod` (`float`; default `0`)<br>`morph_mod` (`float`; default `0`)<br>`decay` (`float`; default `0.5`)<br>`lpg_colour` (`float`; default `0.5`)<br>`mul` (`float`; default `1`) | 2 | `conditional` — plugin: mi-UGens |
| `MiRings` | `mi_rings_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`)<br>`pit` (`float`; default `60`)<br>`struct` (`float`; default `0.25`)<br>`bright` (`float`; default `0.5`)<br>`damp` (`float`; default `0.7`)<br>`pos` (`float`; default `0.25`)<br>`model` (`int`; default `0`)<br>`poly` (`int`; default `1`)<br>`intern_exciter` (`int`; default `0`)<br>`easteregg` (`int`; default `0`)<br>`bypass` (`int`; default `0`)<br>`mul` (`float`; default `1`) | 2 | `conditional` — plugin: mi-UGens |
| `MiRipples` | `mi_ripples_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`cf` (`float`; default `0.5`)<br>`reson` (`float`; default `0.3`)<br>`drive` (`float`; default `1`)<br>`mul` (`float`; default `1`) | 1 | `conditional` — plugin: mi-UGens |
| `MiTides` | `mi_tides_ar` | `ar` / `audio` | `freq` (`float`; default `1`)<br>`shape` (`float`; default `0.5`)<br>`slope` (`float`; default `0.5`)<br>`smooth` (`float`; default `0.5`)<br>`shift` (`float`; default `0.5`)<br>`trig` (`signal`; default `0`)<br>`clock` (`signal`; default `0`)<br>`output_mode` (`int`; default `1`)<br>`ramp_mode` (`int`; default `1`)<br>`ratio` (`int`; default `9`)<br>`rate` (`int`; default `0`)<br>`mul` (`float`; default `1`) | 4 | `conditional` — plugin: mi-UGens |
| `MiVerb` | `mi_verb_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`time` (`float`; default `0.5`)<br>`drywet` (`float`; default `0.5`)<br>`damp` (`float`; default `0.5`)<br>`hp` (`float`; default `0`)<br>`freeze` (`int`; default `0`)<br>`diff` (`float`; default `0.625`)<br>`mul` (`float`; default `1`) | 2 | `conditional` — plugin: mi-UGens |
| `MiWarps` | `mi_warps_ar` | `ar` / `audio` | `carrier` (`signal`; default `0`)<br>`modulator` (`signal`; default `0`)<br>`lev1` (`float`; default `0.5`)<br>`lev2` (`float`; default `0.5`)<br>`algo` (`float`; default `0`)<br>`timb` (`float`; default `0.5`)<br>`osc` (`int`; default `0`)<br>`freq` (`float`; default `110`)<br>`vgain` (`float`; default `1`)<br>`easteregg` (`int`; default `0`) | 2 | `conditional` — plugin: mi-UGens |

## `multichannel.json`

Source: [`crates/vibelang-dsp/ugen_manifests/multichannel.json`](../../../crates/vibelang-dsp/ugen_manifests/multichannel.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Mix` | `mix_ar` | `ar` / `audio` | `array` (`signal`; default `0`) | 1 | `available` |
| `Mix` | `mix_kr` | `kr` / `control` | `array` (`signal`; default `0`) | 1 | `available` |
| `Splay` | `splay_ar` | `ar` / `audio` | `inArray` (`signal`; default `0`)<br>`spread` (`float`; default `1`)<br>`level` (`float`; default `1`)<br>`center` (`float`; default `0`)<br>`levelComp` (`float`; default `1`) | 2 | `available` |
| `Splay` | `splay_kr` | `kr` / `control` | `inArray` (`signal`; default `0`)<br>`spread` (`float`; default `1`)<br>`level` (`float`; default `1`)<br>`center` (`float`; default `0`)<br>`levelComp` (`float`; default `1`) | 2 | `available` |

## `noise.json`

Source: [`crates/vibelang-dsp/ugen_manifests/noise.json`](../../../crates/vibelang-dsp/ugen_manifests/noise.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BrownNoise` | `brown_noise_ar` | `ar` / `audio` | none | 1 | `available` |
| `BrownNoise` | `brown_noise_kr` | `kr` / `control` | none | 1 | `available` |
| `ClipNoise` | `clip_noise_ar` | `ar` / `audio` | none | 1 | `available` |
| `ClipNoise` | `clip_noise_kr` | `kr` / `control` | none | 1 | `available` |
| `Crackle` | `crackle_ar` | `ar` / `audio` | `chaosParam` (`float`; default `1.5`) | 1 | `available` |
| `Crackle` | `crackle_kr` | `kr` / `control` | `chaosParam` (`float`; default `1.5`) | 1 | `available` |
| `Dust` | `dust_ar` | `ar` / `audio` | `density` (`float`; default `0`) | 1 | `available` |
| `Dust` | `dust_kr` | `kr` / `control` | `density` (`float`; default `0`) | 1 | `available` |
| `Dust2` | `dust2_ar` | `ar` / `audio` | `density` (`float`; default `0`) | 1 | `available` |
| `Dust2` | `dust2_kr` | `kr` / `control` | `density` (`float`; default `0`) | 1 | `available` |
| `ExpRand` | `exp_rand_ir` | `ir` / `scalar` | `lo` (`float`; default `0.01`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `GrayNoise` | `gray_noise_ar` | `ar` / `audio` | none | 1 | `available` |
| `GrayNoise` | `gray_noise_kr` | `kr` / `control` | none | 1 | `available` |
| `Hasher` | `hasher_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `Hasher` | `hasher_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `LFClipNoise` | `lf_clip_noise_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFClipNoise` | `lf_clip_noise_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDClipNoise` | `lfd_clip_noise_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDClipNoise` | `lfd_clip_noise_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDNoise0` | `lfd_noise0_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDNoise0` | `lfd_noise0_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDNoise1` | `lfd_noise1_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDNoise1` | `lfd_noise1_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDNoise3` | `lfd_noise3_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFDNoise3` | `lfd_noise3_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFNoise0` | `lf_noise0_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFNoise0` | `lf_noise0_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFNoise1` | `lf_noise1_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFNoise1` | `lf_noise1_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFNoise2` | `lf_noise2_ar` | `ar` / `audio` | `freq` (`float`; default `500`) | 1 | `available` |
| `LFNoise2` | `lf_noise2_kr` | `kr` / `control` | `freq` (`float`; default `500`) | 1 | `available` |
| `Logistic` | `logistic_ar` | `ar` / `audio` | `chaosParam` (`float`; default `3.0`)<br>`freq` (`float`; default `1000`)<br>`init` (`float`; default `0.5`) | 1 | `available` |
| `Logistic` | `logistic_kr` | `kr` / `control` | `chaosParam` (`float`; default `3.0`)<br>`freq` (`float`; default `1000`)<br>`init` (`float`; default `0.5`) | 1 | `available` |
| `MantissaMask` | `mantissa_mask_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`bits` (`float`; default `3`) | 1 | `available` |
| `MantissaMask` | `mantissa_mask_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`bits` (`float`; default `3`) | 1 | `available` |
| `NRand` | `n_rand_ir` | `ir` / `scalar` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`)<br>`n` (`float`; default `0`) | 1 | `available` |
| `PinkNoise` | `pink_noise_ar` | `ar` / `audio` | none | 1 | `available` |
| `PinkNoise` | `pink_noise_kr` | `kr` / `control` | none | 1 | `available` |
| `RandID` | `rand_id_ir` | `ir` / `scalar` | `id` (`float`; default `0`) | 0 | `available` |
| `RandID` | `rand_id_kr` | `kr` / `control` | `id` (`float`; default `0`) | 0 | `available` |
| `RandSeed` | `rand_seed_ir` | `ir` / `scalar` | `trig` (`signal`; default `0`)<br>`seed` (`float`; default `56789`) | 0 | `available` |
| `RandSeed` | `rand_seed_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`seed` (`float`; default `56789`) | 0 | `available` |
| `WhiteNoise` | `white_noise_ar` | `ar` / `audio` | none | 1 | `available` |
| `WhiteNoise` | `white_noise_kr` | `kr` / `control` | none | 1 | `available` |

## `oscillators.json`

Source: [`crates/vibelang-dsp/ugen_manifests/oscillators.json`](../../../crates/vibelang-dsp/ugen_manifests/oscillators.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AmpComp` | `amp_comp_ar` | `ar` / `audio` | `freq` (`float`; default `261.6256`)<br>`root` (`float`; default `261.6256`)<br>`exp` (`float`; default `0.3333`) | 1 | `available` |
| `AmpComp` | `amp_comp_ir` | `ir` / `scalar` | `freq` (`float`; default `261.6256`)<br>`root` (`float`; default `261.6256`)<br>`exp` (`float`; default `0.3333`) | 1 | `available` |
| `AmpComp` | `amp_comp_kr` | `kr` / `control` | `freq` (`float`; default `261.6256`)<br>`root` (`float`; default `261.6256`)<br>`exp` (`float`; default `0.3333`) | 1 | `available` |
| `AmpCompA` | `amp_comp_a_ar` | `ar` / `audio` | `freq` (`float`; default `1000`)<br>`root` (`float`; default `0`)<br>`minAmp` (`float`; default `0.32`)<br>`rootAmp` (`float`; default `1`) | 1 | `available` |
| `AmpCompA` | `amp_comp_a_ir` | `ir` / `scalar` | `freq` (`float`; default `1000`)<br>`root` (`float`; default `0`)<br>`minAmp` (`float`; default `0.32`)<br>`rootAmp` (`float`; default `1`) | 1 | `available` |
| `AmpCompA` | `amp_comp_a_kr` | `kr` / `control` | `freq` (`float`; default `1000`)<br>`root` (`float`; default `0`)<br>`minAmp` (`float`; default `0.32`)<br>`rootAmp` (`float`; default `1`) | 1 | `available` |
| `Blip` | `blip_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`numharm` (`float`; default `200`) | 1 | `available` |
| `Blip` | `blip_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`numharm` (`float`; default `200`) | 1 | `available` |
| `COsc` | `c_osc_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`beats` (`float`; default `0.5`) | 1 | `available` |
| `COsc` | `c_osc_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`beats` (`float`; default `0.5`) | 1 | `available` |
| `DegreeToKey` | `degree_to_key_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`octave` (`float`; default `12`) | 1 | `available` |
| `DegreeToKey` | `degree_to_key_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`octave` (`float`; default `12`) | 1 | `available` |
| `DetectIndex` | `detect_index_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `DetectIndex` | `detect_index_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `FSinOsc` | `f_sin_osc_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `FSinOsc` | `f_sin_osc_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `FoldIndex` | `fold_index_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `FoldIndex` | `fold_index_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `Formant` | `formant_ar` | `ar` / `audio` | `fundfreq` (`float`; default `440`)<br>`formfreq` (`float`; default `1760`)<br>`bwfreq` (`float`; default `880`) | 1 | `available` |
| `Gendy1` | `gendy1_ar` | `ar` / `audio` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1`)<br>`ddparam` (`float`; default `1`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `1`) | 1 | `available` |
| `Gendy1` | `gendy1_kr` | `kr` / `control` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1`)<br>`ddparam` (`float`; default `1`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `1`) | 1 | `available` |
| `Gendy2` | `gendy2_ar` | `ar` / `audio` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1`)<br>`ddparam` (`float`; default `1`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `1`)<br>`a` (`float`; default `1.17`)<br>`c` (`float`; default `0.31`) | 1 | `available` |
| `Gendy2` | `gendy2_kr` | `kr` / `control` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1`)<br>`ddparam` (`float`; default `1`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `1`)<br>`a` (`float`; default `1.17`)<br>`c` (`float`; default `0.31`) | 1 | `available` |
| `Gendy3` | `gendy3_ar` | `ar` / `audio` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1`)<br>`ddparam` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `1`) | 1 | `available` |
| `Gendy3` | `gendy3_kr` | `kr` / `control` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1`)<br>`ddparam` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `1`) | 1 | `available` |
| `Impulse` | `impulse_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `Impulse` | `impulse_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `Index` | `index_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `Index` | `index_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `IndexInBetween` | `index_in_between_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `IndexInBetween` | `index_in_between_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `IndexL` | `index_l_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `IndexL` | `index_l_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `Klang` | `klang_ar` | `ar` / `audio` | `specificationsArrayRef` (`signal`; default `0`)<br>`freqscale` (`float`; default `1`)<br>`freqoffset` (`float`; default `0`) | 1 | `available` |
| `Klank` | `klank_ar` | `ar` / `audio` | `specificationsArrayRef` (`signal`; default `0`)<br>`input` (`signal`; default `0`)<br>`freqscale` (`float`; default `1`)<br>`freqoffset` (`float`; default `0`)<br>`decayscale` (`float`; default `1`) | 1 | `available` |
| `LFCub` | `lf_cub_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `LFCub` | `lf_cub_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `LFPar` | `lf_par_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `LFPar` | `lf_par_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `LFPulse` | `lf_pulse_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`)<br>`width` (`float`; default `0.5`) | 1 | `available` |
| `LFPulse` | `lf_pulse_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`)<br>`width` (`float`; default `0.5`) | 1 | `available` |
| `LFSaw` | `lf_saw_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `LFSaw` | `lf_saw_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `LFTri` | `lf_tri_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `LFTri` | `lf_tri_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `ModDif` | `mod_dif_ar` | `ar` / `audio` | `x` (`float`; default `0`)<br>`y` (`float`; default `0`)<br>`mod` (`float`; default `1`) | 1 | `available` |
| `ModDif` | `mod_dif_ir` | `ir` / `scalar` | `x` (`float`; default `0`)<br>`y` (`float`; default `0`)<br>`mod` (`float`; default `1`) | 1 | `available` |
| `ModDif` | `mod_dif_kr` | `kr` / `control` | `x` (`float`; default `0`)<br>`y` (`float`; default `0`)<br>`mod` (`float`; default `1`) | 1 | `available` |
| `Osc` | `osc_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `Osc` | `osc_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `OscN` | `osc_n_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `OscN` | `osc_n_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `PMOsc` | `pm_osc` | `builder` / `audio` | `carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `440`)<br>`pmindex` (`float`; default `0`)<br>`modphase` (`float`; default `0`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `PSinGrain` | `p_sin_grain_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`dur` (`float`; default `0.2`)<br>`amp` (`float`; default `0.1`) | 1 | `available` |
| `Pulse` | `pulse_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`width` (`float`; default `0.5`) | 1 | `available` |
| `Pulse` | `pulse_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`width` (`float`; default `0.5`) | 1 | `available` |
| `Saw` | `saw_ar` | `ar` / `audio` | `freq` (`float`; default `440`) | 1 | `available` |
| `Saw` | `saw_kr` | `kr` / `control` | `freq` (`float`; default `440`) | 1 | `available` |
| `Shaper` | `shaper_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `Shaper` | `shaper_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `SinOsc` | `sin_osc_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `SinOsc` | `sin_osc_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `SinOscFB` | `sin_osc_fb_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`feedback` (`float`; default `0`) | 1 | `available` |
| `SinOscFB` | `sin_osc_fb_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`feedback` (`float`; default `0`) | 1 | `available` |
| `SyncSaw` | `sync_saw_ar` | `ar` / `audio` | `syncFreq` (`float`; default `440`)<br>`sawFreq` (`float`; default `440`) | 1 | `available` |
| `SyncSaw` | `sync_saw_kr` | `kr` / `control` | `syncFreq` (`float`; default `440`)<br>`sawFreq` (`float`; default `440`) | 1 | `available` |
| `TWindex` | `t_windex_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`array` (`signal`; default `0`)<br>`normalize` (`float`; default `0`) | 1 | `available` |
| `TWindex` | `t_windex_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`array` (`signal`; default `0`)<br>`normalize` (`float`; default `0`) | 1 | `available` |
| `Unwrap` | `unwrap_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Unwrap` | `unwrap_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `VOsc` | `v_osc_ar` | `ar` / `audio` | `bufpos` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `VOsc` | `v_osc_kr` | `kr` / `control` | `bufpos` (`float`; default `0`)<br>`freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `VOsc3` | `v_osc3_ar` | `ar` / `audio` | `bufpos` (`float`; default `0`)<br>`freq1` (`float`; default `110`)<br>`freq2` (`float`; default `220`)<br>`freq3` (`float`; default `440`) | 1 | `available` |
| `VOsc3` | `v_osc3_kr` | `kr` / `control` | `bufpos` (`float`; default `0`)<br>`freq1` (`float`; default `110`)<br>`freq2` (`float`; default `220`)<br>`freq3` (`float`; default `440`) | 1 | `available` |
| `VarSaw` | `var_saw_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`)<br>`width` (`float`; default `0.5`) | 1 | `available` |
| `VarSaw` | `var_saw_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`)<br>`width` (`float`; default `0.5`) | 1 | `available` |
| `Vibrato` | `vibrato_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`rate` (`float`; default `6`)<br>`depth` (`float`; default `0.02`)<br>`delay` (`float`; default `0`)<br>`onset` (`float`; default `0`)<br>`rateVariation` (`float`; default `0.04`)<br>`depthVariation` (`float`; default `0.1`)<br>`iphase` (`float`; default `0`)<br>`trig` (`float`; default `0`) | 1 | `available` |
| `Vibrato` | `vibrato_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`rate` (`float`; default `6`)<br>`depth` (`float`; default `0.02`)<br>`delay` (`float`; default `0`)<br>`onset` (`float`; default `0`)<br>`rateVariation` (`float`; default `0.04`)<br>`depthVariation` (`float`; default `0.1`)<br>`iphase` (`float`; default `0`)<br>`trig` (`float`; default `0`) | 1 | `available` |
| `WrapIndex` | `wrap_index_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `WrapIndex` | `wrap_index_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |

## `panning.json`

Source: [`crates/vibelang-dsp/ugen_manifests/panning.json`](../../../crates/vibelang-dsp/ugen_manifests/panning.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Balance2` | `balance2_ar` | `ar` / `audio` | `left` (`signal`; default `0`)<br>`right` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 2 | `available` |
| `Balance2` | `balance2_kr` | `kr` / `control` | `left` (`signal`; default `0`)<br>`right` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 2 | `available` |
| `BiPanB2` | `bi_pan_b2_ar` | `ar` / `audio` | `inA` (`signal`; default `0`)<br>`inB` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`gain` (`float`; default `1`) | 3 | `available` |
| `BiPanB2` | `bi_pan_b2_kr` | `kr` / `control` | `inA` (`signal`; default `0`)<br>`inB` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`gain` (`float`; default `1`) | 3 | `available` |
| `DecodeB2` | `decode_b2_ar` | `ar` / `audio` | `numChans` (`float`; default `4`)<br>`w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`orientation` (`float`; default `0.5`) | 2 | `available` |
| `DecodeB2` | `decode_b2_kr` | `kr` / `control` | `numChans` (`float`; default `4`)<br>`w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`orientation` (`float`; default `0.5`) | 2 | `available` |
| `LinPan2` | `lin_pan2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 2 | `available` |
| `LinPan2` | `lin_pan2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 2 | `available` |
| `LinXFade2` | `lin_x_fade2_ar` | `ar` / `audio` | `inA` (`signal`; default `0`)<br>`inB` (`signal`; default `0`)<br>`pan` (`float`; default `0`)<br>`level` (`float`; default `1`) | 1 | `available` |
| `LinXFade2` | `lin_x_fade2_kr` | `kr` / `control` | `inA` (`signal`; default `0`)<br>`inB` (`signal`; default `0`)<br>`pan` (`float`; default `0`)<br>`level` (`float`; default `1`) | 1 | `available` |
| `Pan2` | `pan2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 2 | `available` |
| `Pan2` | `pan2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 2 | `available` |
| `Pan4` | `pan4_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`xpos` (`float`; default `0`)<br>`ypos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 4 | `available` |
| `Pan4` | `pan4_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`xpos` (`float`; default `0`)<br>`ypos` (`float`; default `0`)<br>`level` (`float`; default `1`) | 4 | `available` |
| `PanAz` | `pan_az_ar` | `ar` / `audio` | `numChans` (`float`; default `4`)<br>`in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`)<br>`width` (`float`; default `2`)<br>`orientation` (`float`; default `0.5`) | 4 | `available` |
| `PanAz` | `pan_az_kr` | `kr` / `control` | `numChans` (`float`; default `4`)<br>`in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`)<br>`width` (`float`; default `2`)<br>`orientation` (`float`; default `0.5`) | 4 | `available` |
| `PanB` | `pan_b_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`gain` (`float`; default `1`) | 4 | `available` |
| `PanB` | `pan_b_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`gain` (`float`; default `1`) | 4 | `available` |
| `PanB2` | `pan_b2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`gain` (`float`; default `1`) | 3 | `available` |
| `PanB2` | `pan_b2_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`gain` (`float`; default `1`) | 3 | `available` |
| `Rotate2` | `rotate2_ar` | `ar` / `audio` | `x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`pos` (`float`; default `0`) | 2 | `available` |
| `Rotate2` | `rotate2_kr` | `kr` / `control` | `x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`pos` (`float`; default `0`) | 2 | `available` |
| `SplayAz` | `splay_az_ar` | `ar` / `audio` | `numChans` (`float`; default `4`)<br>`inArray` (`signal`; default `0`)<br>`spread` (`float`; default `1`)<br>`level` (`float`; default `1`)<br>`width` (`float`; default `2`)<br>`center` (`float`; default `0`)<br>`orientation` (`float`; default `0.5`)<br>`levelComp` (`float`; default `1`) | 4 | `available` |
| `SplayAz` | `splay_az_kr` | `kr` / `control` | `numChans` (`float`; default `4`)<br>`inArray` (`signal`; default `0`)<br>`spread` (`float`; default `1`)<br>`level` (`float`; default `1`)<br>`width` (`float`; default `2`)<br>`center` (`float`; default `0`)<br>`orientation` (`float`; default `0.5`)<br>`levelComp` (`float`; default `1`) | 4 | `available` |
| `XFade2` | `x_fade2_ar` | `ar` / `audio` | `inA` (`signal`; default `0`)<br>`inB` (`signal`; default `0`)<br>`pan` (`float`; default `0`)<br>`level` (`float`; default `1`) | 1 | `available` |
| `XFade2` | `x_fade2_kr` | `kr` / `control` | `inA` (`signal`; default `0`)<br>`inB` (`signal`; default `0`)<br>`pan` (`float`; default `0`)<br>`level` (`float`; default `1`) | 1 | `available` |

## `physical.json`

Source: [`crates/vibelang-dsp/ugen_manifests/physical.json`](../../../crates/vibelang-dsp/ugen_manifests/physical.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Ball` | `ball_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`g` (`float`; default `1`)<br>`damp` (`float`; default `0`)<br>`friction` (`float`; default `0.01`) | 1 | `available` |
| `Ball` | `ball_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`g` (`float`; default `1`)<br>`damp` (`float`; default `0`)<br>`friction` (`float`; default `0.01`) | 1 | `available` |
| `Pluck` | `pluck_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `1`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`)<br>`coef` (`float`; default `0.5`) | 1 | `available` |
| `Spring` | `spring_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`spring` (`float`; default `1`)<br>`damp` (`float`; default `0`) | 1 | `available` |
| `TBall` | `t_ball_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`g` (`float`; default `10`)<br>`damp` (`float`; default `0`)<br>`friction` (`float`; default `0.01`) | 1 | `available` |
| `TBall` | `t_ball_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`g` (`float`; default `10`)<br>`damp` (`float`; default `0`)<br>`friction` (`float`; default `0.01`) | 1 | `available` |

## `pitchtime.json`

Source: [`crates/vibelang-dsp/ugen_manifests/pitchtime.json`](../../../crates/vibelang-dsp/ugen_manifests/pitchtime.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `PitchShift` | `pitch_shift_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`windowSize` (`float`; default `0.2`)<br>`pitchRatio` (`float`; default `1`)<br>`pitchDispersion` (`float`; default `0`)<br>`timeDispersion` (`float`; default `0`) | 1 | `available` |

## `pv_spectral.json`

Source: [`crates/vibelang-dsp/ugen_manifests/pv_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/pv_spectral.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `PV_Add` | `pv_add_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_BinScramble` | `pv_bin_scramble_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`wipe` (`float`; default `0`)<br>`width` (`float`; default `0.2`)<br>`trig` (`float`; default `0`) | 1 | `available` |
| `PV_BinShift` | `pv_bin_shift_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`stretch` (`float`; default `1.0`)<br>`shift` (`float`; default `0`)<br>`interp` (`float`; default `0`) | 1 | `available` |
| `PV_BinWipe` | `pv_bin_wipe_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`wipe` (`float`; default `0`) | 1 | `available` |
| `PV_BrickWall` | `pv_brick_wall_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`wipe` (`float`; default `0`) | 1 | `available` |
| `PV_ConformalMap` | `pv_conformal_map_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`areal` (`float`; default `0`)<br>`aimag` (`float`; default `0`) | 1 | `available` |
| `PV_Conj` | `pv_conj_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_Copy` | `pv_copy_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_CopyPhase` | `pv_copy_phase_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_Diffuser` | `pv_diffuser_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`trig` (`float`; default `0`) | 1 | `available` |
| `PV_Div` | `pv_div_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_LocalMax` | `pv_local_max_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0`) | 1 | `available` |
| `PV_MagAbove` | `pv_mag_above_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0`) | 1 | `available` |
| `PV_MagBelow` | `pv_mag_below_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0`) | 1 | `available` |
| `PV_MagClip` | `pv_mag_clip_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0`) | 1 | `available` |
| `PV_MagDiv` | `pv_mag_div_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`zeroed` (`float`; default `0.0001`) | 1 | `available` |
| `PV_MagFreeze` | `pv_mag_freeze_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`freeze` (`float`; default `0`) | 1 | `available` |
| `PV_MagMul` | `pv_mag_mul_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_MagNoise` | `pv_mag_noise_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_MagShift` | `pv_mag_shift_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`stretch` (`float`; default `1.0`)<br>`shift` (`float`; default `0`) | 1 | `available` |
| `PV_MagSmear` | `pv_mag_smear_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`bins` (`float`; default `0`) | 1 | `available` |
| `PV_MagSquared` | `pv_mag_squared_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_Max` | `pv_max_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_Min` | `pv_min_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_Mul` | `pv_mul_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_PhaseShift` | `pv_phase_shift_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`shift` (`float`; default `0`)<br>`integrate` (`float`; default `0`) | 1 | `available` |
| `PV_PhaseShift270` | `pv_phase_shift270_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_PhaseShift90` | `pv_phase_shift90_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_RandComb` | `pv_rand_comb_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`wipe` (`float`; default `0`)<br>`trig` (`float`; default `0`) | 1 | `available` |
| `PV_RandWipe` | `pv_rand_wipe_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`wipe` (`float`; default `0`)<br>`trig` (`float`; default `0`) | 1 | `available` |
| `PV_RectComb` | `pv_rect_comb_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`numTeeth` (`float`; default `0`)<br>`phase` (`float`; default `0`)<br>`width` (`float`; default `0.5`) | 1 | `available` |
| `PV_RectComb2` | `pv_rect_comb2_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`numTeeth` (`float`; default `0`)<br>`phase` (`float`; default `0`)<br>`width` (`float`; default `0.5`) | 1 | `available` |

## `random.json`

Source: [`crates/vibelang-dsp/ugen_manifests/random.json`](../../../crates/vibelang-dsp/ugen_manifests/random.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `CoinGate` | `coin_gate_ar` | `ar` / `audio` | `prob` (`float`; default `0.5`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `CoinGate` | `coin_gate_kr` | `kr` / `control` | `prob` (`float`; default `0.5`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `IRand` | `i_rand_ir` | `ir` / `scalar` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `127`) | 1 | `available` |
| `LinRand` | `lin_rand_ir` | `ir` / `scalar` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`)<br>`minmax` (`float`; default `0`) | 1 | `available` |
| `Rand` | `rand_ir` | `ir` / `scalar` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `TExpRand` | `t_exp_rand_ar` | `ar` / `audio` | `lo` (`float`; default `0.01`)<br>`hi` (`float`; default `1`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TExpRand` | `t_exp_rand_kr` | `kr` / `control` | `lo` (`float`; default `0.01`)<br>`hi` (`float`; default `1`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TIRand` | `ti_rand_ar` | `ar` / `audio` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `127`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TIRand` | `ti_rand_kr` | `kr` / `control` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `127`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TRand` | `t_rand_ar` | `ar` / `audio` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TRand` | `t_rand_kr` | `kr` / `control` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TWChoose` | `tw_choose` | `builder` / `audio` | `trig` (`signal`; default `0`)<br>`array` (`signal`; default `0`)<br>`weights` (`signal`; default `0`)<br>`normalize` (`float`; default `0`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |

## `reverb.json`

Source: [`crates/vibelang-dsp/ugen_manifests/reverb.json`](../../../crates/vibelang-dsp/ugen_manifests/reverb.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `FreeVerb` | `free_verb_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`mix` (`float`; default `0.33`)<br>`room` (`float`; default `0.5`)<br>`damp` (`float`; default `0.5`) | 1 | `available` |
| `FreeVerb2` | `free_verb2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`mix` (`float`; default `0.33`)<br>`room` (`float`; default `0.5`)<br>`damp` (`float`; default `0.5`) | 2 | `available` |
| `GVerb` | `g_verb_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`roomsize` (`float`; default `10`)<br>`revtime` (`float`; default `3`)<br>`damping` (`float`; default `0.5`)<br>`inputbw` (`float`; default `0.5`)<br>`spread` (`float`; default `15`)<br>`drylevel` (`float`; default `1`)<br>`earlyreflevel` (`float`; default `0.7`)<br>`taillevel` (`float`; default `0.5`)<br>`maxroomsize` (`float`; default `300`) | 2 | `available` |

## `sc3_aa_oscillators.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_aa_oscillators.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_aa_oscillators.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BlitB3` | `blit_b3_ar` | `ar` / `audio` | `freq` (`float`; default `440`) | 1 | `available` |
| `BlitB3Saw` | `blit_b3_saw_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`leak` (`float`; default `0.99`) | 1 | `available` |
| `BlitB3Square` | `blit_b3_square_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`leak` (`float`; default `0.99`) | 1 | `available` |
| `BlitB3Tri` | `blit_b3_tri_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`leak` (`float`; default `0.99`)<br>`leak2` (`float`; default `0.99`) | 1 | `available` |
| `DPW3Tri` | `dpw3_tri_ar` | `ar` / `audio` | `freq` (`float`; default `440`) | 1 | `available` |
| `DPW4Saw` | `dpw4_saw_ar` | `ar` / `audio` | `freq` (`float`; default `440`) | 1 | `available` |

## `sc3_auditory.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_auditory.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_auditory.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Gammatone` | `gammatone_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`centrefrequency` (`float`; default `440.0`)<br>`bandwidth` (`float`; default `200.0`) | 1 | `available` |
| `HairCell` | `hair_cell_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`spontaneousrate` (`float`; default `0.0`)<br>`boostrate` (`float`; default `200.0`)<br>`restorerate` (`float`; default `1000.0`)<br>`loss` (`float`; default `0.99`) | 1 | `available` |
| `HairCell` | `hair_cell_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`spontaneousrate` (`float`; default `0.0`)<br>`boostrate` (`float`; default `200.0`)<br>`restorerate` (`float`; default `1000.0`)<br>`loss` (`float`; default `0.99`) | 1 | `available` |
| `Meddis` | `meddis_ar` | `ar` / `audio` | `input` (`signal`; default `0`) | 1 | `available` |
| `Meddis` | `meddis_kr` | `kr` / `control` | `input` (`signal`; default `0`) | 1 | `available` |

## `sc3_ay.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_ay.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_ay.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AY` | `ay_ar` | `ar` / `audio` | `tonea` (`int`; default `1777`)<br>`toneb` (`int`; default `1666`)<br>`tonec` (`int`; default `1555`)<br>`noise` (`int`; default `1`)<br>`control` (`int`; default `7`)<br>`vola` (`int`; default `15`)<br>`volb` (`int`; default `15`)<br>`volc` (`int`; default `15`)<br>`envfreq` (`int`; default `4`)<br>`envstyle` (`int`; default `1`)<br>`chiptype` (`int`; default `0`) | 1 | `available` |

## `sc3_bat.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_bat.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_bat.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Coyote` | `coyote_kr` | `kr` / `control` | `in` (`signal`; default `0.0`)<br>`trackFall` (`signal`; default `0.2`)<br>`slowLag` (`signal`; default `0.2`)<br>`fastLag` (`signal`; default `0.01`)<br>`fastMul` (`signal`; default `0.5`)<br>`thresh` (`signal`; default `0.05`)<br>`minDur` (`signal`; default `0.1`) | 1 | `available` |
| `FrameCompare` | `frame_compare_kr` | `kr` / `control` | `buffer1` (`signal`; default `0`)<br>`buffer2` (`signal`; default `0`)<br>`wAmount` (`signal`; default `0.5`) | 1 | `available` |
| `MarkovSynth` | `markov_synth_ar` | `ar` / `audio` | `in` (`signal`; default `0.0`)<br>`isRecording` (`signal`; default `1`)<br>`waitTime` (`signal`; default `2`)<br>`tableSize` (`signal`; default `10`) | 1 | `available` |
| `NeedleRect` | `needle_rect_ar` | `ar` / `audio` | `rate` (`signal`; default `1.0`)<br>`imgWidth` (`signal`; default `100`)<br>`imgHeight` (`signal`; default `100`)<br>`rectX` (`signal`; default `0`)<br>`rectY` (`signal`; default `0`)<br>`rectW` (`signal`; default `100`)<br>`rectH` (`signal`; default `100`) | 1 | `available` |
| `SkipNeedle` | `skip_needle_ar` | `ar` / `audio` | `range` (`signal`; default `44100`)<br>`rate` (`signal`; default `10`)<br>`offset` (`signal`; default `0`) | 1 | `available` |
| `TrigAvg` | `trig_avg_kr` | `kr` / `control` | `in` (`signal`; default `0.0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `WAmp` | `w_amp_kr` | `kr` / `control` | `in` (`signal`; default `0.0`)<br>`winSize` (`signal`; default `0.1`) | 1 | `available` |

## `sc3_bbcut2.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_bbcut2.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_bbcut2.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AnalyseEvents2` | `analyse_events2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`bufnum` (`int`; default `0`)<br>`threshold` (`float`; default `0.34`)<br>`triggerid` (`int`; default `101`)<br>`circular` (`int`; default `0`)<br>`pitch` (`signal`; default `0`) | 1 | `available` |
| `DrumTrack` | `drum_track_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lock` (`float`; default `0`)<br>`dynleak` (`float`; default `0.0`)<br>`tempowt` (`float`; default `0.0`)<br>`phasewt` (`float`; default `0.0`)<br>`basswt` (`float`; default `0.0`)<br>`patternwt` (`float`; default `1.0`)<br>`prior` (`int`; default `-10`)<br>`kicksensitivity` (`float`; default `1.0`)<br>`snaresensitivity` (`float`; default `1.0`)<br>`debugmode` (`int`; default `0`) | 4 | `available` |

## `sc3_berlach.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_berlach.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_berlach.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BLBufRd` | `bl_buf_rd_ar` | `ar` / `audio` | `bufnum` (`signal`; default `0`)<br>`phase` (`signal`; default `0`)<br>`ratio` (`signal`; default `1`) | 1 | `available` |
| `BLBufRd` | `bl_buf_rd_kr` | `kr` / `control` | `bufnum` (`signal`; default `0`)<br>`phase` (`signal`; default `0`)<br>`ratio` (`signal`; default `1`) | 1 | `available` |
| `Clipper4` | `clipper4_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`signal`; default `-0.8`)<br>`hi` (`signal`; default `0.8`) | 1 | `available` |
| `Clipper8` | `clipper8_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`signal`; default `-0.8`)<br>`hi` (`signal`; default `0.8`) | 1 | `available` |
| `DriveNoise` | `drive_noise_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`amount` (`signal`; default `1`)<br>`multi` (`signal`; default `5`) | 1 | `available` |
| `LPF1` | `lpf1_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `1000`) | 1 | `available` |
| `LPF1` | `lpf1_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `1000`) | 1 | `available` |
| `LPF18` | `lpf18_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `100`)<br>`res` (`signal`; default `1`)<br>`dist` (`signal`; default `0.4`) | 1 | `available` |
| `LPFVS6` | `lpfvs6_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `1000`)<br>`slope` (`signal`; default `0.5`) | 1 | `available` |
| `LPFVS6` | `lpfvs6_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `1000`)<br>`slope` (`signal`; default `0.5`) | 1 | `available` |
| `OSFold4` | `os_fold4_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`signal`; default `-1.0`)<br>`hi` (`signal`; default `1.0`) | 1 | `available` |
| `OSFold8` | `os_fold8_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`signal`; default `-1.0`)<br>`hi` (`signal`; default `1.0`) | 1 | `available` |
| `OSTrunc4` | `os_trunc4_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`quant` (`signal`; default `0.5`) | 1 | `available` |
| `OSTrunc8` | `os_trunc8_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`quant` (`signal`; default `0.5`) | 1 | `available` |
| `OSWrap4` | `os_wrap4_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`signal`; default `-1.0`)<br>`hi` (`signal`; default `1.0`) | 1 | `available` |
| `OSWrap8` | `os_wrap8_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`signal`; default `-1.0`)<br>`hi` (`signal`; default `1.0`) | 1 | `available` |
| `PeakEQ2` | `peak_eq2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `1200.0`)<br>`rs` (`signal`; default `1.0`)<br>`db` (`signal`; default `0.0`) | 1 | `available` |
| `PeakEQ4` | `peak_eq4_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `1200.0`)<br>`rs` (`signal`; default `1.0`)<br>`db` (`signal`; default `0.0`) | 1 | `available` |
| `SoftClipAmp4` | `soft_clip_amp4_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`pregain` (`signal`; default `1`) | 1 | `available` |
| `SoftClipAmp8` | `soft_clip_amp8_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`pregain` (`signal`; default `1`) | 1 | `available` |
| `SoftClipper4` | `soft_clipper4_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `SoftClipper8` | `soft_clipper8_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |

## `sc3_betablocker.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_betablocker.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_betablocker.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BBlockerBuf` | `b_blocker_buf_ar` | `ar` / `audio` | `freq` (`signal`; default `440.0`)<br>`bufnum` (`signal`; default `0`)<br>`startpoint` (`signal`; default `0`) | 9 | `available` |
| `DetaBlockerBuf` | `deta_blocker_buf_demand` | `demand` / `unavailable` | `bufnum` (`signal`; default `0`)<br>`startpoint` (`signal`; default `0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |

## `sc3_bhob.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_bhob.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_bhob.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Dbrown2` | `dbrown2_demand` | `demand` / `unavailable` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`step` (`float`; default `0.01`)<br>`dist` (`float`; default `0`)<br>`length` (`float`; default `1000000000.0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `Dgauss` | `dgauss_demand` | `demand` / `unavailable` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`length` (`float`; default `1000000000.0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `DoubleNestedAllpassC` | `double_nested_allpass_c_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelay1` (`float`; default `0.0047`)<br>`delay1` (`float`; default `0.0047`)<br>`gain1` (`float`; default `0.15`)<br>`maxdelay2` (`float`; default `0.022`)<br>`delay2` (`float`; default `0.022`)<br>`gain2` (`float`; default `0.25`)<br>`maxdelay3` (`float`; default `0.0083`)<br>`delay3` (`float`; default `0.0083`)<br>`gain3` (`float`; default `0.3`) | 1 | `available` |
| `DoubleNestedAllpassL` | `double_nested_allpass_l_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelay1` (`float`; default `0.0047`)<br>`delay1` (`float`; default `0.0047`)<br>`gain1` (`float`; default `0.15`)<br>`maxdelay2` (`float`; default `0.022`)<br>`delay2` (`float`; default `0.022`)<br>`gain2` (`float`; default `0.25`)<br>`maxdelay3` (`float`; default `0.0083`)<br>`delay3` (`float`; default `0.0083`)<br>`gain3` (`float`; default `0.3`) | 1 | `available` |
| `DoubleNestedAllpassN` | `double_nested_allpass_n_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelay1` (`float`; default `0.0047`)<br>`delay1` (`float`; default `0.0047`)<br>`gain1` (`float`; default `0.15`)<br>`maxdelay2` (`float`; default `0.022`)<br>`delay2` (`float`; default `0.022`)<br>`gain2` (`float`; default `0.25`)<br>`maxdelay3` (`float`; default `0.0083`)<br>`delay3` (`float`; default `0.0083`)<br>`gain3` (`float`; default `0.3`) | 1 | `available` |
| `Fhn2DC` | `fhn2dc_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `Fhn2DC` | `fhn2dc_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `Fhn2DL` | `fhn2dl_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `Fhn2DL` | `fhn2dl_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `Fhn2DN` | `fhn2dn_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `Fhn2DN` | `fhn2dn_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `FhnTrig` | `fhn_trig_ar` | `ar` / `audio` | `minfreq` (`float`; default `4`)<br>`maxfreq` (`float`; default `10`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `FhnTrig` | `fhn_trig_kr` | `kr` / `control` | `minfreq` (`float`; default `4`)<br>`maxfreq` (`float`; default `10`)<br>`urate` (`float`; default `0.1`)<br>`wrate` (`float`; default `0.1`)<br>`b0` (`float`; default `0.6`)<br>`b1` (`float`; default `0.8`)<br>`i` (`float`; default `0.0`)<br>`u0` (`float`; default `0.0`)<br>`w0` (`float`; default `0.0`) | 1 | `available` |
| `GaussTrig` | `gauss_trig_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`dev` (`float`; default `0.3`) | 1 | `available` |
| `GaussTrig` | `gauss_trig_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`dev` (`float`; default `0.3`) | 1 | `available` |
| `Gbman2DC` | `gbman2dc_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `Gbman2DC` | `gbman2dc_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `Gbman2DL` | `gbman2dl_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `Gbman2DL` | `gbman2dl_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `Gbman2DN` | `gbman2dn_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `Gbman2DN` | `gbman2dn_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `GbmanTrig` | `gbman_trig_ar` | `ar` / `audio` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `GbmanTrig` | `gbman_trig_kr` | `kr` / `control` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`x0` (`float`; default `1.2`)<br>`y0` (`float`; default `2.1`) | 1 | `available` |
| `Gendy4` | `gendy4_ar` | `ar` / `audio` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1.0`)<br>`ddparam` (`float`; default `1.0`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `12`) | 1 | `available` |
| `Gendy4` | `gendy4_kr` | `kr` / `control` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1.0`)<br>`ddparam` (`float`; default `1.0`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `12`) | 1 | `available` |
| `Gendy5` | `gendy5_ar` | `ar` / `audio` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1.0`)<br>`ddparam` (`float`; default `1.0`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `12`) | 1 | `available` |
| `Gendy5` | `gendy5_kr` | `kr` / `control` | `ampdist` (`float`; default `1`)<br>`durdist` (`float`; default `1`)<br>`adparam` (`float`; default `1.0`)<br>`ddparam` (`float`; default `1.0`)<br>`minfreq` (`float`; default `440`)<br>`maxfreq` (`float`; default `660`)<br>`ampscale` (`float`; default `0.5`)<br>`durscale` (`float`; default `0.5`)<br>`initCPs` (`float`; default `12`)<br>`knum` (`float`; default `12`) | 1 | `available` |
| `Henon2DC` | `henon2dc_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `Henon2DC` | `henon2dc_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `Henon2DL` | `henon2dl_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `Henon2DL` | `henon2dl_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `Henon2DN` | `henon2dn_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `Henon2DN` | `henon2dn_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `HenonTrig` | `henon_trig_ar` | `ar` / `audio` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `HenonTrig` | `henon_trig_kr` | `kr` / `control` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`a` (`float`; default `1.4`)<br>`b` (`float`; default `0.3`)<br>`x0` (`float`; default `0.30501993062401`)<br>`y0` (`float`; default `0.20938865431933`) | 1 | `available` |
| `LFBrownNoise0` | `lf_brown_noise0_ar` | `ar` / `audio` | `freq` (`float`; default `20`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`) | 1 | `available` |
| `LFBrownNoise0` | `lf_brown_noise0_kr` | `kr` / `control` | `freq` (`float`; default `20`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`) | 1 | `available` |
| `LFBrownNoise1` | `lf_brown_noise1_ar` | `ar` / `audio` | `freq` (`float`; default `20`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`) | 1 | `available` |
| `LFBrownNoise1` | `lf_brown_noise1_kr` | `kr` / `control` | `freq` (`float`; default `20`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`) | 1 | `available` |
| `LFBrownNoise2` | `lf_brown_noise2_ar` | `ar` / `audio` | `freq` (`float`; default `20`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`) | 1 | `available` |
| `LFBrownNoise2` | `lf_brown_noise2_kr` | `kr` / `control` | `freq` (`float`; default `20`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`) | 1 | `available` |
| `Latoocarfian2DC` | `latoocarfian2dc_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `Latoocarfian2DC` | `latoocarfian2dc_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `Latoocarfian2DL` | `latoocarfian2dl_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `Latoocarfian2DL` | `latoocarfian2dl_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `Latoocarfian2DN` | `latoocarfian2dn_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `Latoocarfian2DN` | `latoocarfian2dn_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `LatoocarfianTrig` | `latoocarfian_trig_ar` | `ar` / `audio` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `LatoocarfianTrig` | `latoocarfian_trig_kr` | `kr` / `control` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`a` (`float`; default `1`)<br>`b` (`float`; default `3`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `0.5`)<br>`x0` (`float`; default `0.34082301375036`)<br>`y0` (`float`; default `-0.38270086971332`) | 1 | `available` |
| `Lorenz2DC` | `lorenz2dc_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `Lorenz2DC` | `lorenz2dc_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `Lorenz2DL` | `lorenz2dl_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `Lorenz2DL` | `lorenz2dl_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `Lorenz2DN` | `lorenz2dn_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `Lorenz2DN` | `lorenz2dn_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `LorenzTrig` | `lorenz_trig_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `LorenzTrig` | `lorenz_trig_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`s` (`float`; default `10`)<br>`r` (`float`; default `28`)<br>`b` (`float`; default `2.6666667`)<br>`h` (`float`; default `0.02`)<br>`x0` (`float`; default `0.090879182417163`)<br>`y0` (`float`; default `2.97077458055`)<br>`z0` (`float`; default `24.282041054363`) | 1 | `available` |
| `MoogLadder` | `moog_ladder_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`ffreq` (`float`; default `440.0`)<br>`res` (`float`; default `0.0`) | 1 | `available` |
| `MoogLadder` | `moog_ladder_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`ffreq` (`float`; default `440.0`)<br>`res` (`float`; default `0.0`) | 1 | `available` |
| `NLFiltC` | `nl_filt_c_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`a` (`float`; default `0.0`)<br>`b` (`float`; default `0.0`)<br>`d` (`float`; default `0.0`)<br>`c` (`float`; default `0.0`)<br>`l` (`float`; default `0.0`) | 1 | `available` |
| `NLFiltC` | `nl_filt_c_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`a` (`float`; default `0.0`)<br>`b` (`float`; default `0.0`)<br>`d` (`float`; default `0.0`)<br>`c` (`float`; default `0.0`)<br>`l` (`float`; default `0.0`) | 1 | `available` |
| `NLFiltL` | `nl_filt_l_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`a` (`float`; default `0.0`)<br>`b` (`float`; default `0.0`)<br>`d` (`float`; default `0.0`)<br>`c` (`float`; default `0.0`)<br>`l` (`float`; default `0.0`) | 1 | `available` |
| `NLFiltL` | `nl_filt_l_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`a` (`float`; default `0.0`)<br>`b` (`float`; default `0.0`)<br>`d` (`float`; default `0.0`)<br>`c` (`float`; default `0.0`)<br>`l` (`float`; default `0.0`) | 1 | `available` |
| `NLFiltN` | `nl_filt_n_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`a` (`float`; default `0.0`)<br>`b` (`float`; default `0.0`)<br>`d` (`float`; default `0.0`)<br>`c` (`float`; default `0.0`)<br>`l` (`float`; default `0.0`) | 1 | `available` |
| `NLFiltN` | `nl_filt_n_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`a` (`float`; default `0.0`)<br>`b` (`float`; default `0.0`)<br>`d` (`float`; default `0.0`)<br>`c` (`float`; default `0.0`)<br>`l` (`float`; default `0.0`) | 1 | `available` |
| `NestedAllpassC` | `nested_allpass_c_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelay1` (`float`; default `0.036`)<br>`delay1` (`float`; default `0.036`)<br>`gain1` (`float`; default `0.08`)<br>`maxdelay2` (`float`; default `0.03`)<br>`delay2` (`float`; default `0.03`)<br>`gain2` (`float`; default `0.3`) | 1 | `available` |
| `NestedAllpassL` | `nested_allpass_l_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelay1` (`float`; default `0.036`)<br>`delay1` (`float`; default `0.036`)<br>`gain1` (`float`; default `0.08`)<br>`maxdelay2` (`float`; default `0.03`)<br>`delay2` (`float`; default `0.03`)<br>`gain2` (`float`; default `0.3`) | 1 | `available` |
| `NestedAllpassN` | `nested_allpass_n_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`maxdelay1` (`float`; default `0.036`)<br>`delay1` (`float`; default `0.036`)<br>`gain1` (`float`; default `0.08`)<br>`maxdelay2` (`float`; default `0.03`)<br>`delay2` (`float`; default `0.03`)<br>`gain2` (`float`; default `0.3`) | 1 | `available` |
| `PV_CommonMag` | `pv_common_mag_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`tolerance` (`float`; default `0.0`)<br>`remove` (`float`; default `0.0`) | 1 | `available` |
| `PV_CommonMul` | `pv_common_mul_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`tolerance` (`float`; default `0.0`)<br>`remove` (`float`; default `0.0`) | 1 | `available` |
| `PV_Compander` | `pv_compander_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`thresh` (`float`; default `50`)<br>`slopeBelow` (`float`; default `1`)<br>`slopeAbove` (`float`; default `1`) | 1 | `available` |
| `PV_Cutoff` | `pv_cutoff_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`wipe` (`float`; default `0.0`) | 1 | `available` |
| `PV_MagGate` | `pv_mag_gate_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`thresh` (`float`; default `1.0`)<br>`remove` (`float`; default `0.0`) | 1 | `available` |
| `PV_MagMinus` | `pv_mag_minus_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`remove` (`float`; default `1.0`) | 1 | `available` |
| `PV_MagScale` | `pv_mag_scale_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `PV_Morph` | `pv_morph_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`morph` (`float`; default `0.0`) | 1 | `available` |
| `PV_SoftWipe` | `pv_soft_wipe_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`wipe` (`float`; default `0.0`) | 1 | `available` |
| `PV_XFade` | `pv_x_fade_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`fade` (`float`; default `0.0`) | 1 | `available` |
| `RLPFD` | `rlpfd_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`ffreq` (`float`; default `440.0`)<br>`res` (`float`; default `0.0`)<br>`dist` (`float`; default `0.0`) | 1 | `available` |
| `RLPFD` | `rlpfd_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`ffreq` (`float`; default `440.0`)<br>`res` (`float`; default `0.0`)<br>`dist` (`float`; default `0.0`) | 1 | `available` |
| `Standard2DC` | `standard2dc_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `Standard2DC` | `standard2dc_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `Standard2DL` | `standard2dl_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `Standard2DL` | `standard2dl_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `Standard2DN` | `standard2dn_ar` | `ar` / `audio` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `Standard2DN` | `standard2dn_kr` | `kr` / `control` | `minfreq` (`float`; default `11025`)<br>`maxfreq` (`float`; default `22050`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `StandardTrig` | `standard_trig_ar` | `ar` / `audio` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `StandardTrig` | `standard_trig_kr` | `kr` / `control` | `minfreq` (`float`; default `5`)<br>`maxfreq` (`float`; default `10`)<br>`k` (`float`; default `1.4`)<br>`x0` (`float`; default `4.9789799812499`)<br>`y0` (`float`; default `5.7473416156381`) | 1 | `available` |
| `Streson` | `streson_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`delayTime` (`float`; default `0.003`)<br>`res` (`float`; default `0.9`) | 1 | `available` |
| `Streson` | `streson_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`delayTime` (`float`; default `0.003`)<br>`res` (`float`; default `0.9`) | 1 | `available` |
| `TBetaRand` | `t_beta_rand_ar` | `ar` / `audio` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`prob1` (`float`; default `1.0`)<br>`prob2` (`float`; default `1.0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TBetaRand` | `t_beta_rand_kr` | `kr` / `control` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`prob1` (`float`; default `1.0`)<br>`prob2` (`float`; default `1.0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TBrownRand` | `t_brown_rand_ar` | `ar` / `audio` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TBrownRand` | `t_brown_rand_kr` | `kr` / `control` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`dev` (`float`; default `1.0`)<br>`dist` (`float`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TGaussRand` | `t_gauss_rand_ar` | `ar` / `audio` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TGaussRand` | `t_gauss_rand_kr` | `kr` / `control` | `lo` (`float`; default `0`)<br>`hi` (`float`; default `1.0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `TGrains2` | `t_grains2_ar` | `ar` / `audio` | `numChannels` (`float`; default `2`)<br>`trigger` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`rate` (`float`; default `1.0`)<br>`centerPos` (`float`; default `0`)<br>`dur` (`float`; default `0.1`)<br>`pan` (`float`; default `0`)<br>`amp` (`float`; default `0.1`)<br>`att` (`float`; default `0.5`)<br>`dec` (`float`; default `0.5`)<br>`interp` (`float`; default `4`) | 1 | `available` |
| `TGrains3` | `t_grains3_ar` | `ar` / `audio` | `numChannels` (`float`; default `2`)<br>`trigger` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`rate` (`float`; default `1.0`)<br>`centerPos` (`float`; default `0`)<br>`dur` (`float`; default `0.1`)<br>`pan` (`float`; default `0`)<br>`amp` (`float`; default `0.1`)<br>`att` (`float`; default `0.5`)<br>`dec` (`float`; default `0.5`)<br>`window` (`float`; default `1`)<br>`interp` (`float`; default `4`) | 1 | `available` |

## `sc3_blackrain.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_blackrain.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_blackrain.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AmplitudeMod` | `amplitude_mod_ar` | `ar` / `audio` | `in` (`signal`; default `0.0`)<br>`attackTime` (`signal`; default `0.01`)<br>`releaseTime` (`signal`; default `0.01`) | 1 | `available` |
| `AmplitudeMod` | `amplitude_mod_kr` | `kr` / `control` | `in` (`signal`; default `0.0`)<br>`attackTime` (`signal`; default `0.01`)<br>`releaseTime` (`signal`; default `0.01`) | 1 | `available` |
| `BMoog` | `b_moog_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `440.0`)<br>`q` (`signal`; default `0.2`)<br>`mode` (`signal`; default `0.0`)<br>`saturation` (`signal`; default `0.95`) | 1 | `available` |
| `IIRFilter` | `iir_filter_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `440.0`)<br>`rq` (`signal`; default `1.0`) | 1 | `available` |
| `SVF` | `svf_ar` | `ar` / `audio` | `signal` (`signal`; default `0`)<br>`cutoff` (`signal`; default `2200.0`)<br>`res` (`signal`; default `0.1`)<br>`lowpass` (`signal`; default `1.0`)<br>`bandpass` (`signal`; default `0.0`)<br>`highpass` (`signal`; default `0.0`)<br>`notch` (`signal`; default `0.0`)<br>`peak` (`signal`; default `0.0`) | 1 | `available` |
| `SVF` | `svf_kr` | `kr` / `control` | `signal` (`signal`; default `0`)<br>`cutoff` (`signal`; default `2200.0`)<br>`res` (`signal`; default `0.1`)<br>`lowpass` (`signal`; default `1.0`)<br>`bandpass` (`signal`; default `0.0`)<br>`highpass` (`signal`; default `0.0`)<br>`notch` (`signal`; default `0.0`)<br>`peak` (`signal`; default `0.0`) | 1 | `available` |

## `sc3_chaos.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_chaos.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_chaos.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `ArneodoCoulletTresser` | `arneodo_coullet_tresser_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`alpha` (`float`; default `1.5`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0.5`)<br>`yi` (`float`; default `0.5`)<br>`zi` (`float`; default `0.5`) | 3 | `available` |
| `DNoiseRing` | `d_noise_ring_demand` | `demand` / `unavailable` | `change` (`signal`; default `0.5`)<br>`chance` (`signal`; default `0.5`)<br>`shift` (`signal`; default `1`)<br>`numBits` (`signal`; default `8`)<br>`resetval` (`signal`; default `0`) | 1 | `quarantined` — demand-rate graph encoding and UGen-specific input lowering are not implemented; demand-rate graph encoding and UGen-specific input lowering are not implemented |
| `LotkaVolterra` | `lotka_volterra_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `1.5`)<br>`b` (`float`; default `1.5`)<br>`c` (`float`; default `0.5`)<br>`d` (`float`; default `1.5`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `1`)<br>`yi` (`float`; default `0.2`) | 2 | `available` |

## `sc3_concat.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_concat.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_concat.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Concat` | `concat_ar` | `ar` / `audio` | `control` (`signal`; default `0`)<br>`source` (`signal`; default `0`)<br>`storesize` (`float`; default `1.0`)<br>`seektime` (`float`; default `1.0`)<br>`seekdur` (`float`; default `1.0`)<br>`matchlength` (`float`; default `0.05`)<br>`freezestore` (`float`; default `0`)<br>`zcr` (`float`; default `1.0`)<br>`lms` (`float`; default `1.0`)<br>`sc` (`float`; default `1.0`)<br>`st` (`float`; default `0.0`)<br>`randscore` (`float`; default `0.0`) | 1 | `available` |
| `Concat2` | `concat2_ar` | `ar` / `audio` | `control` (`signal`; default `0`)<br>`source` (`signal`; default `0`)<br>`storesize` (`float`; default `1.0`)<br>`seektime` (`float`; default `1.0`)<br>`seekdur` (`float`; default `1.0`)<br>`matchlength` (`float`; default `0.05`)<br>`freezestore` (`float`; default `0`)<br>`zcr` (`float`; default `1.0`)<br>`lms` (`float`; default `1.0`)<br>`sc` (`float`; default `1.0`)<br>`st` (`float`; default `0.0`)<br>`randscore` (`float`; default `0.0`)<br>`threshold` (`float`; default `0.01`) | 1 | `available` |

## `sc3_deind.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_deind.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_deind.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `ComplexRes` | `complex_res_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `100`)<br>`decay` (`float`; default `0.2`) | 1 | `available` |
| `DiodeRingMod` | `diode_ring_mod_ar` | `ar` / `audio` | `car` (`signal`; default `0`)<br>`mod` (`signal`; default `0`) | 1 | `available` |
| `FaustGreyholeRaw` | `faust_greyhole_raw` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`damping` (`float`; default `0`)<br>`delaytime` (`float`; default `0.2`)<br>`diffusion` (`float`; default `0.5`)<br>`feedback` (`float`; default `0.9`)<br>`moddepth` (`float`; default `0.1`)<br>`modfreq` (`float`; default `2`)<br>`size` (`float`; default `1`) | 2 | `documentation_only` — No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| `Greyhole` | `greyhole_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`delayTime` (`float`; default `2`)<br>`damp` (`float`; default `0`)<br>`size` (`float`; default `1`)<br>`diff` (`float`; default `0.707`)<br>`feedback` (`float`; default `0.9`)<br>`modDepth` (`float`; default `0.1`)<br>`modFreq` (`float`; default `2`) | 2 | `available` |
| `GreyholeRaw` | `greyhole_raw_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`damping` (`float`; default `0`)<br>`delaytime` (`float`; default `2`)<br>`diffusion` (`float`; default `0.5`)<br>`feedback` (`float`; default `0.9`)<br>`moddepth` (`float`; default `0.1`)<br>`modfreq` (`float`; default `2`)<br>`size` (`float`; default `1`) | 2 | `available` |
| `JPverb` | `j_pverb_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`t60` (`float`; default `1`)<br>`damp` (`float`; default `0`)<br>`size` (`float`; default `1`)<br>`earlyDiff` (`float`; default `0.707`)<br>`modDepth` (`float`; default `0.1`)<br>`modFreq` (`float`; default `2`)<br>`low` (`float`; default `1`)<br>`mid` (`float`; default `1`)<br>`high` (`float`; default `1`)<br>`lowcut` (`float`; default `500`)<br>`highcut` (`float`; default `2000`) | 2 | `available` |
| `JPverbRaw` | `j_pverb_raw_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`damp` (`float`; default `0`)<br>`earlydiff` (`float`; default `0.707`)<br>`highband` (`float`; default `2000`)<br>`highx` (`float`; default `1`)<br>`lowband` (`float`; default `500`)<br>`lowx` (`float`; default `1`)<br>`mdepth` (`float`; default `0.1`)<br>`mfreq` (`float`; default `2`)<br>`midx` (`float`; default `1`)<br>`size` (`float`; default `1`)<br>`t60` (`float`; default `1`) | 2 | `available` |
| `JPverbRaw` | `j_pverb_raw_kr` | `kr` / `control` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`damp` (`float`; default `0`)<br>`earlydiff` (`float`; default `0.707`)<br>`highband` (`float`; default `2000`)<br>`highx` (`float`; default `1`)<br>`lowband` (`float`; default `500`)<br>`lowx` (`float`; default `1`)<br>`mdepth` (`float`; default `0.1`)<br>`mfreq` (`float`; default `2`)<br>`midx` (`float`; default `1`)<br>`size` (`float`; default `1`)<br>`t60` (`float`; default `1`) | 2 | `available` |
| `RMS` | `rms_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lpFreq` (`float`; default `10`) | 1 | `available` |
| `RMS` | `rms_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lpFreq` (`float`; default `10`) | 1 | `available` |

## `sc3_dfm1.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_dfm1.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_dfm1.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `DFM1` | `dfm1_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`signal`; default `1000.0`)<br>`res` (`signal`; default `0.1`)<br>`inputgain` (`signal`; default `1.0`)<br>`type` (`signal`; default `0.0`)<br>`noiselevel` (`signal`; default `0.0003`) | 1 | `available` |

## `sc3_distortion.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_distortion.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_distortion.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `CrossoverDistortion` | `crossover_distortion_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`amp` (`signal`; default `0.5`)<br>`smooth` (`signal`; default `0.5`) | 1 | `available` |
| `Decimator` | `decimator_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`rate` (`signal`; default `44100`)<br>`bits` (`signal`; default `24`) | 1 | `available` |
| `Disintegrator` | `disintegrator_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`probability` (`signal`; default `0.5`)<br>`multiplier` (`signal`; default `0.0`) | 1 | `available` |
| `SineShaper` | `sine_shaper_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`limit` (`signal`; default `1.0`) | 1 | `available` |
| `SmoothDecimator` | `smooth_decimator_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`rate` (`signal`; default `44100`)<br>`smoothing` (`signal`; default `0.5`) | 1 | `available` |

## `sc3_dwg.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_dwg.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_dwg.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `DWGBowed` | `dwg_bowed_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`velb` (`float`; default `0.5`)<br>`force` (`float`; default `1.0`)<br>`gate` (`float`; default `1`)<br>`pos` (`float`; default `0.14`)<br>`release` (`float`; default `0.1`)<br>`c1` (`float`; default `1.0`)<br>`c3` (`float`; default `3.0`)<br>`impZ` (`float`; default `0.55`)<br>`fB` (`float`; default `2.0`) | 1 | `available` |
| `DWGBowedSimple` | `dwg_bowed_simple_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`velb` (`float`; default `0.5`)<br>`force` (`float`; default `1.0`)<br>`gate` (`float`; default `1`)<br>`pos` (`float`; default `0.14`)<br>`release` (`float`; default `0.1`)<br>`c1` (`float`; default `1.0`)<br>`c3` (`float`; default `30.0`) | 1 | `available` |
| `DWGBowedTor` | `dwg_bowed_tor_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`velb` (`float`; default `0.5`)<br>`force` (`float`; default `1.0`)<br>`gate` (`float`; default `1`)<br>`pos` (`float`; default `0.14`)<br>`release` (`float`; default `0.1`)<br>`c1` (`float`; default `1.0`)<br>`c3` (`float`; default `3.0`)<br>`impZ` (`float`; default `0.55`)<br>`fB` (`float`; default `2.0`)<br>`mistune` (`float`; default `5.2`)<br>`c1tor` (`float`; default `1.0`)<br>`c3tor` (`float`; default `3000.0`)<br>`iZtor` (`float`; default `1.8`) | 1 | `available` |
| `DWGPlucked` | `dwg_plucked_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`amp` (`float`; default `0.5`)<br>`gate` (`float`; default `1`)<br>`pos` (`float`; default `0.14`)<br>`c1` (`float`; default `1.0`)<br>`c3` (`float`; default `30.0`)<br>`inp` (`signal`; default `0`)<br>`release` (`float`; default `0.1`) | 1 | `available` |
| `DWGPlucked2` | `dwg_plucked2_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`amp` (`float`; default `0.5`)<br>`gate` (`float`; default `1`)<br>`pos` (`float`; default `0.14`)<br>`c1` (`float`; default `1.0`)<br>`c3` (`float`; default `30.0`)<br>`inp` (`signal`; default `0`)<br>`release` (`float`; default `0.1`)<br>`mistune` (`float`; default `1.008`)<br>`mp` (`float`; default `0.55`)<br>`gc` (`float`; default `0.01`) | 1 | `available` |
| `DWGPluckedStiff` | `dwg_plucked_stiff_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`amp` (`float`; default `0.5`)<br>`gate` (`float`; default `1`)<br>`pos` (`float`; default `0.14`)<br>`c1` (`float`; default `1.0`)<br>`c3` (`float`; default `30.0`)<br>`inp` (`signal`; default `0`)<br>`release` (`float`; default `0.1`)<br>`fB` (`float`; default `2.0`) | 1 | `available` |
| `DWGSoundBoard` | `dwg_sound_board_ar` | `ar` / `audio` | `inp` (`signal`; default `0`)<br>`c1` (`float`; default `20.0`)<br>`c3` (`float`; default `20.0`)<br>`mix` (`float`; default `0.8`)<br>`d1` (`float`; default `199.0`)<br>`d2` (`float`; default `211.0`)<br>`d3` (`float`; default `223.0`)<br>`d4` (`float`; default `227.0`)<br>`d5` (`float`; default `229.0`)<br>`d6` (`float`; default `233.0`)<br>`d7` (`float`; default `239.0`)<br>`d8` (`float`; default `241.0`) | 1 | `available` |

## `sc3_fm7.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_fm7.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_fm7.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `FM7` | `fm7` | `builder` / `audio` | `.controlMatrix(rows...)` (`method`; default `0`)<br>`.modMatrix(rows...)` (`method`; default `0`)<br>`.algoSpec(algo, feedback)` (`method`; default `0`) | 6 | `documentation_only` |

## `sc3_glitch.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_glitch.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_glitch.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `GlitchBPF` | `glitch_bpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `GlitchBPF` | `glitch_bpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `GlitchBRF` | `glitch_brf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `GlitchBRF` | `glitch_brf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `GlitchHPF` | `glitch_hpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`) | 1 | `available` |
| `GlitchHPF` | `glitch_hpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`) | 1 | `available` |
| `GlitchRHPF` | `glitch_rhpf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |
| `GlitchRHPF` | `glitch_rhpf_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440`)<br>`rq` (`float`; default `1`) | 1 | `available` |

## `sc3_josh_granular.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_josh_granular.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_granular.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `BufGrain` | `buf_grain_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`) | 1 | `available` |
| `BufGrainB` | `buf_grain_b_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`)<br>`envbuf` (`float`; default `0`) | 1 | `available` |
| `BufGrainBBF` | `buf_grain_bbf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`)<br>`envbuf` (`float`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `BufGrainBF` | `buf_grain_bf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `BufGrainI` | `buf_grain_i_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`) | 1 | `available` |
| `BufGrainIBF` | `buf_grain_ibf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `FMGrain` | `fm_grain_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`) | 1 | `available` |
| `FMGrainB` | `fm_grain_b_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`)<br>`envbuf` (`float`; default `0`) | 1 | `available` |
| `FMGrainBBF` | `fm_grain_bbf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`)<br>`envbuf` (`float`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `FMGrainBF` | `fm_grain_bf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `FMGrainI` | `fm_grain_i_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`) | 1 | `available` |
| `FMGrainIBF` | `fm_grain_ibf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `GrainBufJ` | `grain_buf_j_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`sndbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`pos` (`float`; default `0`)<br>`interp` (`float`; default `2`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`)<br>`grainAmp` (`float`; default `1`)<br>`loop` (`float`; default `0`) | 1 | `available` |
| `GrainFMJ` | `grain_fmj_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`carfreq` (`float`; default `440`)<br>`modfreq` (`float`; default `200`)<br>`index` (`float`; default `1`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`)<br>`grainAmp` (`float`; default `1`) | 1 | `available` |
| `GrainInJ` | `grain_in_j_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`)<br>`grainAmp` (`float`; default `1`) | 1 | `available` |
| `GrainSinJ` | `grain_sin_j_ar` | `ar` / `audio` | `numChannels` (`float`; default `1`)<br>`trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`pan` (`float`; default `0`)<br>`envbufnum` (`float`; default `-1`)<br>`maxGrains` (`float`; default `512`)<br>`grainAmp` (`float`; default `1`) | 1 | `available` |
| `InGrain` | `in_grain_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `InGrainB` | `in_grain_b_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`)<br>`envbuf` (`float`; default `0`) | 1 | `available` |
| `InGrainBBF` | `in_grain_bbf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`)<br>`envbuf` (`float`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `InGrainBF` | `in_grain_bf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `InGrainI` | `in_grain_i_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`) | 1 | `available` |
| `InGrainIBF` | `in_grain_ibf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`in` (`signal`; default `0`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `MonoGrain` | `mono_grain_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`winsize` (`float`; default `0.1`)<br>`grainrate` (`float`; default `10`)<br>`winrandpct` (`float`; default `0`) | 1 | `available` |
| `MonoGrainBF` | `mono_grain_bf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`winsize` (`float`; default `0.1`)<br>`grainrate` (`float`; default `10`)<br>`winrandpct` (`float`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`azrand` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`elrand` (`float`; default `0`)<br>`rho` (`float`; default `1`) | 4 | `available` |
| `SinGrain` | `sin_grain_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`) | 1 | `available` |
| `SinGrainB` | `sin_grain_b_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`envbuf` (`float`; default `0`) | 1 | `available` |
| `SinGrainBBF` | `sin_grain_bbf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`envbuf` (`float`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `SinGrainBF` | `sin_grain_bf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `SinGrainI` | `sin_grain_i_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`) | 1 | `available` |
| `SinGrainIBF` | `sin_grain_ibf_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dur` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`envbuf1` (`float`; default `0`)<br>`envbuf2` (`float`; default `0`)<br>`ifac` (`float`; default `0.5`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |

## `sc3_josh_spectral.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_josh_spectral.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `A2B` | `a2b_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`)<br>`c` (`signal`; default `0`)<br>`d` (`signal`; default `0`) | 4 | `available` |
| `AtsAmp` | `ats_amp_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`partialNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 1 | `available` |
| `AtsAmp` | `ats_amp_kr` | `kr` / `control` | `atsbuffer` (`float`; default `0`)<br>`partialNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 1 | `available` |
| `AtsBand` | `ats_band_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`band` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 1 | `available` |
| `AtsFile` | `ats_file` | `builder` / `audio` | `.load(path)` (`method`; default `0`)<br>`.load_to_buffer(buffer)` (`method`; default `0`)<br>`.buffer` (`method`; default `0`)<br>`.free_buffer` (`method`; default `0`) | 1 | `documentation_only` — Client-side data/helper class; no scsynth UGen is emitted. |
| `AtsFreq` | `ats_freq_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`partialNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 1 | `available` |
| `AtsFreq` | `ats_freq_kr` | `kr` / `control` | `atsbuffer` (`float`; default `0`)<br>`partialNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 1 | `available` |
| `AtsNoiSynth` | `ats_noi_synth_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`numPartials` (`float`; default `0`)<br>`partialStart` (`float`; default `0`)<br>`partialSkip` (`float`; default `1`)<br>`filePointer` (`float`; default `0`)<br>`sinePct` (`float`; default `1`)<br>`noisePct` (`float`; default `1`)<br>`freqMul` (`float`; default `1`)<br>`freqAdd` (`float`; default `0`)<br>`numBands` (`float`; default `25`)<br>`bandStart` (`float`; default `0`)<br>`bandSkip` (`float`; default `1`) | 1 | `available` |
| `AtsNoise` | `ats_noise_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`bandNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 1 | `available` |
| `AtsNoise` | `ats_noise_kr` | `kr` / `control` | `atsbuffer` (`float`; default `0`)<br>`bandNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 1 | `available` |
| `AtsParInfo` | `ats_par_info_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`partialNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 2 | `available` |
| `AtsParInfo` | `ats_par_info_kr` | `kr` / `control` | `atsbuffer` (`float`; default `0`)<br>`partialNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 2 | `available` |
| `AtsPartial` | `ats_partial_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`partial` (`float`; default `0`)<br>`filePointer` (`float`; default `0`)<br>`freqMul` (`float`; default `1`)<br>`freqAdd` (`float`; default `0`) | 1 | `available` |
| `AtsSynth` | `ats_synth_ar` | `ar` / `audio` | `atsbuffer` (`float`; default `0`)<br>`numPartials` (`float`; default `0`)<br>`partialStart` (`float`; default `0`)<br>`partialSkip` (`float`; default `1`)<br>`filePointer` (`float`; default `0`)<br>`freqMul` (`float`; default `1`)<br>`freqAdd` (`float`; default `0`) | 1 | `available` |
| `AudioMSG` | `audio_msg_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`index` (`float`; default `0`) | 1 | `available` |
| `B2A` | `b2a_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`) | 4 | `available` |
| `B2Ster` | `b2_ster_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`) | 2 | `available` |
| `B2UHJ` | `b2uhj_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`) | 2 | `available` |
| `BFDecode1` | `bf_decode1_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`wComp` (`float`; default `0`) | 1 | `available` |
| `BFEncode1` | `bf_encode1_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`gain` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `BFEncode2` | `bf_encode2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`point_x` (`float`; default `1`)<br>`point_y` (`float`; default `1`)<br>`elevation` (`float`; default `0`)<br>`gain` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `BFEncodeSter` | `bf_encode_ster_ar` | `ar` / `audio` | `l` (`signal`; default `0`)<br>`r` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`width` (`float`; default `1.5707963`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`gain` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 4 | `available` |
| `BFManipulate` | `bf_manipulate_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`rotate` (`float`; default `0`)<br>`tilt` (`float`; default `0`)<br>`tumble` (`float`; default `0`) | 4 | `available` |
| `Balance` | `balance_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`test` (`signal`; default `0`)<br>`hp` (`float`; default `10`)<br>`stor` (`float`; default `0`) | 1 | `available` |
| `BinData` | `bin_data_ar` | `ar` / `audio` | `buffer` (`signal`; default `0`)<br>`bin` (`float`; default `0`)<br>`overlaps` (`float`; default `0.5`) | 2 | `available` |
| `BinData` | `bin_data_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`bin` (`float`; default `0`)<br>`overlaps` (`float`; default `0.5`) | 2 | `available` |
| `CombLP` | `comb_lp_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`gate` (`float`; default `1`)<br>`maxdelaytime` (`float`; default `0.2`)<br>`delaytime` (`float`; default `0.2`)<br>`decaytime` (`float`; default `1`)<br>`coef` (`float`; default `0.5`) | 1 | `available` |
| `FMHDecode1` | `fmh_decode1_ar` | `ar` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`r` (`signal`; default `0`)<br>`s` (`signal`; default `0`)<br>`t` (`signal`; default `0`)<br>`u` (`signal`; default `0`)<br>`v` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`) | 1 | `available` |
| `FMHEncode0` | `fmh_encode0_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`gain` (`float`; default `1`) | 9 | `available` |
| `FMHEncode1` | `fmh_encode1_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`rho` (`float`; default `1`)<br>`gain` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 9 | `available` |
| `FMHEncode2` | `fmh_encode2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`point_x` (`float`; default `0`)<br>`point_y` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`gain` (`float`; default `1`)<br>`wComp` (`float`; default `0`) | 9 | `available` |
| `LPCSynth` | `lpc_synth_ar` | `ar` / `audio` | `buffer` (`float`; default `0`)<br>`signal` (`signal`; default `0`)<br>`pointer` (`float`; default `0`) | 1 | `available` |
| `LPCVals` | `lpc_vals_ar` | `ar` / `audio` | `buffer` (`float`; default `0`)<br>`pointer` (`float`; default `0`) | 3 | `available` |
| `LPCVals` | `lpc_vals_kr` | `kr` / `control` | `buffer` (`float`; default `0`)<br>`pointer` (`float`; default `0`) | 3 | `available` |
| `Maxamp` | `maxamp_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`numSamps` (`float`; default `1000`) | 1 | `available` |
| `Metro` | `metro_ar` | `ar` / `audio` | `bpm` (`float`; default `120`)<br>`numBeats` (`float`; default `4`) | 1 | `available` |
| `Metro` | `metro_kr` | `kr` / `control` | `bpm` (`float`; default `120`)<br>`numBeats` (`float`; default `4`) | 1 | `available` |
| `MoogVCF` | `moog_vcf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`fco` (`float`; default `440`)<br>`res` (`float`; default `0`) | 1 | `available` |
| `PVInfo` | `pv_info_ar` | `ar` / `audio` | `pvbuffer` (`float`; default `0`)<br>`binNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 2 | `available` |
| `PVInfo` | `pv_info_kr` | `kr` / `control` | `pvbuffer` (`float`; default `0`)<br>`binNum` (`float`; default `0`)<br>`filePointer` (`float`; default `0`) | 2 | `available` |
| `PVSynth` | `pv_synth_ar` | `ar` / `audio` | `pvbuffer` (`float`; default `0`)<br>`numBins` (`float`; default `0`)<br>`binStart` (`float`; default `0`)<br>`binSkip` (`float`; default `1`)<br>`filePointer` (`float`; default `0`)<br>`freqMul` (`float`; default `1`)<br>`freqAdd` (`float`; default `0`) | 1 | `available` |
| `PV_BinBufRd` | `pv_bin_buf_rd_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`playbuf` (`float`; default `0`)<br>`point` (`float`; default `1`)<br>`binStart` (`float`; default `0`)<br>`binSkip` (`float`; default `1`)<br>`numBins` (`float`; default `1`)<br>`clear` (`float`; default `0`) | 1 | `available` |
| `PV_BinDelay` | `pv_bin_delay_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`maxdelay` (`float`; default `1`)<br>`delaybuf` (`float`; default `0`)<br>`fbbuf` (`float`; default `0`)<br>`hop` (`float`; default `0.5`) | 1 | `available` |
| `PV_BinFilter` | `pv_bin_filter_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`start` (`float`; default `0`)<br>`end` (`float`; default `0`) | 1 | `available` |
| `PV_BinPlayBuf` | `pv_bin_play_buf_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`playbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`offset` (`float`; default `0`)<br>`loop` (`float`; default `0`)<br>`binStart` (`float`; default `0`)<br>`binSkip` (`float`; default `1`)<br>`numBins` (`float`; default `1`)<br>`clear` (`float`; default `0`) | 1 | `available` |
| `PV_BufRd` | `pv_buf_rd_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`playbuf` (`float`; default `0`)<br>`point` (`float`; default `1`) | 1 | `available` |
| `PV_EvenBin` | `pv_even_bin_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_Freeze` | `pv_freeze_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`freeze` (`float`; default `0`) | 1 | `available` |
| `PV_FreqBuffer` | `pv_freq_buffer_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`databuffer` (`float`; default `0`) | 1 | `available` |
| `PV_Invert` | `pv_invert_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_MagBuffer` | `pv_mag_buffer_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`databuffer` (`float`; default `0`) | 1 | `available` |
| `PV_MagMap` | `pv_mag_map_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`mapbuf` (`float`; default `0`) | 1 | `available` |
| `PV_MaxMagN` | `pv_max_mag_n_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`numbins` (`float`; default `8`) | 1 | `available` |
| `PV_MinMagN` | `pv_min_mag_n_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`numbins` (`float`; default `8`) | 1 | `available` |
| `PV_NoiseSynthF` | `pv_noise_synth_f_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0.1`)<br>`numFrames` (`float`; default `2`)<br>`initflag` (`float`; default `0`) | 1 | `available` |
| `PV_NoiseSynthP` | `pv_noise_synth_p_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0.1`)<br>`numFrames` (`float`; default `2`)<br>`initflag` (`float`; default `0`) | 1 | `available` |
| `PV_OddBin` | `pv_odd_bin_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_PartialSynthF` | `pv_partial_synth_f_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0.1`)<br>`numFrames` (`float`; default `2`)<br>`initflag` (`float`; default `0`) | 1 | `available` |
| `PV_PartialSynthP` | `pv_partial_synth_p_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`threshold` (`float`; default `0.1`)<br>`numFrames` (`float`; default `2`)<br>`initflag` (`float`; default `0`) | 1 | `available` |
| `PV_PitchShift` | `pv_pitch_shift_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`ratio` (`float`; default `1`) | 1 | `available` |
| `PV_PlayBuf` | `pv_play_buf_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`playbuf` (`float`; default `0`)<br>`rate` (`float`; default `1`)<br>`offset` (`float`; default `0`)<br>`loop` (`float`; default `0`) | 1 | `available` |
| `PV_RecordBuf` | `pv_record_buf_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`recbuf` (`float`; default `0`)<br>`offset` (`float`; default `0`)<br>`run` (`float`; default `0`)<br>`loop` (`float`; default `0`)<br>`hop` (`float`; default `0.5`)<br>`wintype` (`float`; default `0`) | 1 | `available` |
| `PV_SpectralEnhance` | `pv_spectral_enhance_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`numPartials` (`float`; default `8`)<br>`ratio` (`float`; default `2`)<br>`strength` (`float`; default `0.1`) | 1 | `available` |
| `PV_SpectralMap` | `pv_spectral_map_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`specBuffer` (`float`; default `0`)<br>`floor` (`float`; default `0`)<br>`freeze` (`float`; default `0`)<br>`mode` (`float`; default `0`)<br>`norm` (`float`; default `0`)<br>`window` (`float`; default `0`) | 1 | `available` |
| `PanX` | `pan_x_ar` | `ar` / `audio` | `numChans` (`int`; default `4`)<br>`in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`)<br>`width` (`float`; default `2`) | 4 | `available` |
| `PanX` | `pan_x_kr` | `kr` / `control` | `numChans` (`int`; default `4`)<br>`in` (`signal`; default `0`)<br>`pos` (`float`; default `0`)<br>`level` (`float`; default `1`)<br>`width` (`float`; default `2`) | 4 | `available` |
| `PanX2D` | `pan_x2d` | `builder` / `audio` | `numChansX` (`int`; default `4`)<br>`numChansY` (`int`; default `4`)<br>`in` (`signal`; default `0`)<br>`posX` (`float`; default `0`)<br>`posY` (`float`; default `0`)<br>`level` (`float`; default `1`)<br>`widthX` (`float`; default `2`)<br>`widthY` (`float`; default `2`) | 16 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `PermMod` | `perm_mod_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `100`) | 1 | `available` |
| `PermModArray` | `perm_mod_array_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `50`)<br>`pattern` (`float`; default `0`) | 1 | `available` |
| `PermModT` | `perm_mod_t_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`outfreq` (`float`; default `440`)<br>`infreq` (`float`; default `5000`) | 1 | `available` |
| `PosRatio` | `pos_ratio_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`period` (`float`; default `100`)<br>`thresh` (`float`; default `0.1`) | 1 | `available` |
| `Rotate` | `rotate` | `builder` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`rotate` (`float`; default `0`) | 4 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `SinTone` | `sin_tone_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`phase` (`float`; default `0`) | 1 | `available` |
| `TTendency` | `t_tendency_ar` | `ar` / `audio` | `trigger` (`signal`; default `0`)<br>`dist` (`float`; default `0`)<br>`parX` (`float`; default `0`)<br>`parY` (`float`; default `1`)<br>`parA` (`float`; default `0`)<br>`parB` (`float`; default `0`) | 1 | `available` |
| `TTendency` | `t_tendency_kr` | `kr` / `control` | `trigger` (`signal`; default `0`)<br>`dist` (`float`; default `0`)<br>`parX` (`float`; default `0`)<br>`parY` (`float`; default `1`)<br>`parA` (`float`; default `0`)<br>`parB` (`float`; default `0`) | 1 | `available` |
| `Tilt` | `tilt` | `builder` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`tilt` (`float`; default `0`) | 4 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `Tumble` | `tumble` | `builder` / `audio` | `w` (`signal`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`)<br>`tumble` (`float`; default `0`) | 4 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `UHJ2B` | `uhj2b_ar` | `ar` / `audio` | `ls` (`signal`; default `0`)<br>`rs` (`signal`; default `0`) | 3 | `available` |
| `WarpZ` | `warp_z_ar` | `ar` / `audio` | `numChannels` (`int`; default `1`)<br>`bufnum` (`float`; default `0`)<br>`pointer` (`float`; default `0`)<br>`freqScale` (`float`; default `1`)<br>`windowSize` (`float`; default `0.2`)<br>`envbufnum` (`float`; default `-1`)<br>`overlaps` (`float`; default `8`)<br>`windowRandRatio` (`float`; default `0`)<br>`interp` (`float`; default `1`)<br>`zeroSearch` (`float`; default `0`)<br>`zeroStart` (`float`; default `0`) | 1 | `available` |

## `sc3_loopbuf.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_loopbuf.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_loopbuf.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `LoopBuf` | `loop_buf_ar` | `ar` / `audio` | `numChannels` (`int`; default `1`)<br>`bufnum` (`int`; default `0`)<br>`rate` (`signal`; default `1.0`)<br>`gate` (`signal`; default `1.0`)<br>`startPos` (`float`; default `0.0`)<br>`startLoop` (`float`; default `0.0`)<br>`endLoop` (`float`; default `0.0`)<br>`interpolation` (`int`; default `2`) | 1 | `available` |

## `sc3_mcld.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_mcld.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mcld.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `ArrayMax` | `array_max_ar` | `ar` / `audio` | `array` (`signal`; default `0`) | 2 | `available` |
| `ArrayMax` | `array_max_kr` | `kr` / `control` | `array` (`signal`; default `0`) | 2 | `available` |
| `ArrayMin` | `array_min_ar` | `ar` / `audio` | `array` (`signal`; default `0`) | 2 | `available` |
| `ArrayMin` | `array_min_kr` | `kr` / `control` | `array` (`signal`; default `0`) | 2 | `available` |
| `BufMax` | `buf_max_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`gate` (`float`; default `1`) | 2 | `available` |
| `BufMin` | `buf_min_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`gate` (`float`; default `1`) | 2 | `available` |
| `CQ_Diff` | `cq_diff` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`databufnum` (`float`; default `0`) | 1 | `documentation_only` — No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| `Cepstrum` | `cepstrum_kr` | `kr` / `control` | `cepbuf` (`float`; default `0`)<br>`fftchain` (`signal`; default `0`) | 1 | `available` |
| `Clockmus` | `clockmus_kr` | `kr` / `control` | none | 1 | `available` |
| `Crest` | `crest_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numsamps` (`float`; default `400`)<br>`gate` (`signal`; default `1`) | 1 | `available` |
| `FFTCentroid` | `fft_centroid_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `FFTComplexDev` | `fft_complex_dev_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`rectify` (`float`; default `0`)<br>`powthresh` (`float`; default `0.1`) | 1 | `available` |
| `FFTCrest` | `fft_crest_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`freqlo` (`float`; default `0`)<br>`freqhi` (`float`; default `50000`) | 1 | `available` |
| `FFTDiffMags` | `fft_diff_mags_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`) | 1 | `available` |
| `FFTFlux` | `fft_flux_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`normalise` (`float`; default `1`) | 1 | `available` |
| `FFTFluxPos` | `fft_flux_pos_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`normalise` (`float`; default `1`) | 1 | `available` |
| `FFTMKL` | `fftmkl_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`epsilon` (`float`; default `1e-6`) | 1 | `available` |
| `FFTPeak` | `fft_peak_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`freqlo` (`float`; default `0`)<br>`freqhi` (`float`; default `50000`) | 2 | `available` |
| `FFTPhaseDev` | `fft_phase_dev_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`weight` (`float`; default `0`)<br>`powthresh` (`float`; default `0.1`) | 1 | `available` |
| `FFTPower` | `fft_power_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`square` (`float`; default `1`) | 1 | `available` |
| `FFTSlope` | `fft_slope_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `FFTSpread` | `fft_spread_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`centroid` (`signal`; default `0`) | 1 | `available` |
| `FFTSubbandFlatness` | `fft_subband_flatness_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`cutfreqs` (`signal`; default `0`) | 1 | `available` |
| `FFTSubbandFlux` | `fft_subband_flux` | `builder` / `audio` | `chain` (`signal`; default `0`)<br>`cutfreqs` (`signal`; default `0`)<br>`posonly` (`float`; default `0`) | 1 | `documentation_only` — No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| `FFTSubbandPower` | `fft_subband_power_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`cutfreqs` (`signal`; default `0`)<br>`square` (`float`; default `1`)<br>`scalemode` (`float`; default `1`) | 1 | `available` |
| `FincoSprottL` | `finco_sprott_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `2.45`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0`)<br>`yi` (`float`; default `0`)<br>`zi` (`float`; default `0`) | 3 | `available` |
| `FincoSprottM` | `finco_sprott_m_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `-7`)<br>`b` (`float`; default `4`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0`)<br>`yi` (`float`; default `0`)<br>`zi` (`float`; default `0`) | 3 | `available` |
| `FincoSprottS` | `finco_sprott_s_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `8`)<br>`b` (`float`; default `2`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0`)<br>`yi` (`float`; default `0`)<br>`zi` (`float`; default `0`) | 3 | `available` |
| `Friction` | `friction_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`friction` (`float`; default `0.5`)<br>`spring` (`float`; default `0.414`)<br>`damp` (`float`; default `0.313`)<br>`mass` (`float`; default `0.1`)<br>`beltmass` (`float`; default `1`) | 1 | `available` |
| `Friction` | `friction_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`friction` (`float`; default `0.5`)<br>`spring` (`float`; default `0.414`)<br>`damp` (`float`; default `0.313`)<br>`mass` (`float`; default `0.1`)<br>`beltmass` (`float`; default `1`) | 1 | `available` |
| `GaussClass` | `gauss_class_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`gate` (`signal`; default `0`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `Getenv` | `getenv_ir` | `ir` / `scalar` | `key` (`float`; default `0`)<br>`defaultval` (`float`; default `0`) | 1 | `available` |
| `Goertzel` | `goertzel_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`bufsize` (`float`; default `1024`)<br>`freq` (`float`; default `440`)<br>`hop` (`float`; default `1`) | 2 | `available` |
| `ICepstrum` | `i_cepstrum_kr` | `kr` / `control` | `cepchain` (`signal`; default `0`)<br>`fftbuf` (`float`; default `0`) | 1 | `available` |
| `InsideOut` | `inside_out_ar` | `ar` / `audio` | `in` (`signal`; default `0`) | 1 | `available` |
| `InsideOut` | `inside_out_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `KMeansRT` | `k_means_rt_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`k` (`float`; default `5`)<br>`gate` (`signal`; default `1`)<br>`reset` (`signal`; default `0`)<br>`learn` (`float`; default `1`)<br>`inputdata` (`signal`; default `0`) | 1 | `available` |
| `ListTrig` | `list_trig_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`reset` (`signal`; default `0`)<br>`offset` (`float`; default `0`)<br>`numframes` (`float`; default `0`) | 1 | `available` |
| `ListTrig2` | `list_trig2_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`reset` (`signal`; default `0`)<br>`numframes` (`float`; default `0`) | 1 | `available` |
| `Logger` | `logger_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`)<br>`inputArray` (`signal`; default `0`) | 1 | `available` |
| `MIDelay` | `mi_delay` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`maxdelay` (`float`; default `0.2`)<br>`gate` (`signal`; default `1`)<br>`mibuf` (`float`; default `-1`) | 1 | `documentation_only` — No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| `MatchingP` | `matching_p_ar` | `ar` / `audio` | `dict` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`dictsize` (`float`; default `1`)<br>`ntofind` (`float`; default `1`)<br>`hop` (`float`; default `1`)<br>`method` (`float`; default `0`) | 1 | `available` |
| `MatchingP` | `matching_p_kr` | `kr` / `control` | `dict` (`float`; default `0`)<br>`in` (`signal`; default `0`)<br>`dictsize` (`float`; default `1`)<br>`ntofind` (`float`; default `1`)<br>`hop` (`float`; default `1`)<br>`method` (`float`; default `0`) | 1 | `available` |
| `MatchingPResynth` | `matching_p_resynth_ar` | `ar` / `audio` | `dict` (`float`; default `0`)<br>`method` (`float`; default `0`)<br>`trigger` (`signal`; default `0`)<br>`residual` (`signal`; default `0`)<br>`activs` (`signal`; default `0`) | 1 | `available` |
| `MatchingPResynth` | `matching_p_resynth_kr` | `kr` / `control` | `dict` (`float`; default `0`)<br>`method` (`float`; default `0`)<br>`trigger` (`signal`; default `0`)<br>`residual` (`signal`; default `0`)<br>`activs` (`signal`; default `0`) | 1 | `available` |
| `MeanTriggered` | `mean_triggered_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`)<br>`length` (`float`; default `10`) | 1 | `available` |
| `MeanTriggered` | `mean_triggered_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`)<br>`length` (`float`; default `10`) | 1 | `available` |
| `MedianTriggered` | `median_triggered_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`)<br>`length` (`float`; default `10`) | 1 | `available` |
| `MedianTriggered` | `median_triggered_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`)<br>`length` (`float`; default `10`) | 1 | `available` |
| `NearestN` | `nearest_n_kr` | `kr` / `control` | `treebuf` (`float`; default `0`)<br>`gate` (`signal`; default `1`)<br>`num` (`float`; default `1`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `OnsetsDS` | `onsets_ds` | `builder` / `audio` | `in` (`signal`; default `0`)<br>`fftbuf` (`float`; default `0`)<br>`trackbuf` (`float`; default `0`)<br>`thresh` (`float`; default `0.5`)<br>`type` (`float`; default `0`)<br>`extchain` (`float`; default `0`)<br>`relaxtime` (`float`; default `0.1`)<br>`floor` (`float`; default `0.1`)<br>`smear` (`float`; default `0`)<br>`mingap` (`float`; default `0.05`)<br>`medianspan` (`float`; default `11`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `PV_DiffMags` | `pv_diff_mags_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`zerolimit` (`float`; default `0`) | 1 | `available` |
| `PV_ExtractRepeat` | `pv_extract_repeat_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`loopbuf` (`float`; default `0`)<br>`loopdur` (`float`; default `1`)<br>`memorytime` (`float`; default `30`)<br>`which` (`float`; default `0`)<br>`ffthop` (`float`; default `0.5`)<br>`thresh` (`float`; default `1`) | 1 | `available` |
| `PV_MagExp` | `pv_mag_exp_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_MagLog` | `pv_mag_log_kr` | `kr` / `control` | `buffer` (`signal`; default `0`) | 1 | `available` |
| `PV_MagMulAdd` | `pv_mag_mul_add_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`mul` (`float`; default `1`)<br>`add` (`float`; default `0`) | 1 | `available` |
| `PV_MagSmooth` | `pv_mag_smooth_kr` | `kr` / `control` | `buffer` (`signal`; default `0`)<br>`factor` (`float`; default `0.1`) | 1 | `available` |
| `PV_MagSubtract` | `pv_mag_subtract_kr` | `kr` / `control` | `bufferA` (`signal`; default `0`)<br>`bufferB` (`signal`; default `0`)<br>`zerolimit` (`float`; default `0`) | 1 | `available` |
| `PV_Whiten` | `pv_whiten_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`trackbufnum` (`float`; default `0`)<br>`relaxtime` (`float`; default `2`)<br>`floor` (`float`; default `0.1`)<br>`smear` (`float`; default `0`)<br>`bindownsample` (`float`; default `0`) | 1 | `available` |
| `Perlin3` | `perlin3_ar` | `ar` / `audio` | `x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`) | 1 | `available` |
| `Perlin3` | `perlin3_kr` | `kr` / `control` | `x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`z` (`signal`; default `0`) | 1 | `available` |
| `PlaneTree` | `plane_tree_kr` | `kr` / `control` | `treebuf` (`float`; default `0`)<br>`gate` (`signal`; default `1`)<br>`in` (`signal`; default `0`) | 1 | `available` |
| `PulseDPW` | `pulse_dpw` | `builder` / `audio` | `freq` (`float`; default `440`)<br>`width` (`float`; default `0.5`) | 1 | `documentation_only` — Confirmed sclang-side pseudo-UGen; unavailable until a VibeLang lowering is implemented. |
| `RMAFoodChainL` | `rma_food_chain_l` | `builder` / `audio` | `freq` (`float`; default `22050`)<br>`a1` (`float`; default `5`)<br>`b1` (`float`; default `3`)<br>`d1` (`float`; default `0.4`)<br>`a2` (`float`; default `0.1`)<br>`b2` (`float`; default `2`)<br>`d2` (`float`; default `0.01`)<br>`k` (`float`; default `1.0943`)<br>`r` (`float`; default `0.8904`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0.1`)<br>`yi` (`float`; default `0`)<br>`zi` (`float`; default `0`) | 3 | `documentation_only` — No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| `RosslerL` | `rossler_l_ar` | `ar` / `audio` | `freq` (`float`; default `22050`)<br>`a` (`float`; default `0.2`)<br>`b` (`float`; default `0.2`)<br>`c` (`float`; default `5.7`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0.1`)<br>`yi` (`float`; default `0`)<br>`zi` (`float`; default `0`) | 3 | `available` |
| `RosslerResL` | `rossler_res_l` | `builder` / `audio` | `in` (`signal`; default `0`)<br>`stiff` (`float`; default `1`)<br>`freq` (`float`; default `22050`)<br>`a` (`float`; default `0.2`)<br>`b` (`float`; default `0.2`)<br>`c` (`float`; default `5.7`)<br>`h` (`float`; default `0.05`)<br>`xi` (`float`; default `0.1`)<br>`yi` (`float`; default `0`)<br>`zi` (`float`; default `0`) | 3 | `documentation_only` — No installed scsynth binary registers this UGen; quarantined until plugin/source verification. |
| `SOMAreaWr` | `som_area_wr_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`netsize` (`float`; default `10`)<br>`numdims` (`float`; default `2`)<br>`nhood` (`float`; default `0.5`)<br>`gate` (`signal`; default `1`)<br>`inputdata` (`signal`; default `0`) | 1 | `available` |
| `SOMRd` | `som_rd_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`netsize` (`float`; default `10`)<br>`numdims` (`float`; default `2`)<br>`gate` (`signal`; default `1`)<br>`inputdata` (`signal`; default `0`) | 1 | `available` |
| `SOMRd` | `som_rd_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`netsize` (`float`; default `10`)<br>`numdims` (`float`; default `2`)<br>`gate` (`signal`; default `1`)<br>`inputdata` (`signal`; default `0`) | 1 | `available` |
| `SOMTrain` | `som_train_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`netsize` (`float`; default `10`)<br>`numdims` (`float`; default `2`)<br>`traindur` (`float`; default `5000`)<br>`nhood` (`float`; default `0.5`)<br>`gate` (`signal`; default `1`)<br>`initweight` (`float`; default `1`)<br>`inputdata` (`signal`; default `0`) | 3 | `available` |
| `SawDPW` | `saw_dpw_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `SawDPW` | `saw_dpw_kr` | `kr` / `control` | `freq` (`float`; default `440`)<br>`iphase` (`float`; default `0`) | 1 | `available` |
| `Squiz` | `squiz_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`pitchratio` (`float`; default `2`)<br>`zcperchunk` (`float`; default `1`)<br>`memlen` (`float`; default `0.1`) | 1 | `available` |
| `Squiz` | `squiz_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`pitchratio` (`float`; default `2`)<br>`zcperchunk` (`float`; default `1`)<br>`memlen` (`float`; default `0.1`) | 1 | `available` |
| `TextVU` | `text_vu_ar` | `ar` / `audio` | `trig` (`signal`; default `2`)<br>`in` (`signal`; default `0`)<br>`label` (`float`; default `0`)<br>`width` (`float`; default `21`)<br>`reset` (`signal`; default `0`)<br>`ana` (`signal`; default `0`) | 1 | `available` |
| `TextVU` | `text_vu_kr` | `kr` / `control` | `trig` (`signal`; default `2`)<br>`in` (`signal`; default `0`)<br>`label` (`float`; default `0`)<br>`width` (`float`; default `21`)<br>`reset` (`signal`; default `0`)<br>`ana` (`signal`; default `0`) | 1 | `available` |
| `WaveLoss` | `wave_loss_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`drop` (`float`; default `20`)<br>`outof` (`float`; default `40`)<br>`mode` (`float`; default `1`) | 1 | `available` |
| `WaveLoss` | `wave_loss_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`drop` (`float`; default `20`)<br>`outof` (`float`; default `40`)<br>`mode` (`float`; default `1`) | 1 | `available` |

## `sc3_mda.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_mda.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_mda.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `MdaPiano` | `mda_piano_ar` | `ar` / `audio` | `freq` (`signal`; default `440`)<br>`gate` (`signal`; default `1`)<br>`vel` (`signal`; default `100`)<br>`decay` (`float`; default `0.8`)<br>`release` (`float`; default `0.8`)<br>`hard` (`float`; default `0.8`)<br>`velhard` (`float`; default `0.8`)<br>`muffle` (`float`; default `0.8`)<br>`velmuff` (`float`; default `0.8`)<br>`velcurve` (`float`; default `0.8`)<br>`stereo` (`float`; default `0.2`)<br>`tune` (`float`; default `0.5`)<br>`random` (`float`; default `0.1`)<br>`stretch` (`float`; default `0.1`)<br>`sustain` (`signal`; default `0`) | 2 | `available` |

## `sc3_membrane.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_membrane.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_membrane.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `MembraneCircle` | `membrane_circle_ar` | `ar` / `audio` | `excitation` (`signal`; default `0`)<br>`tension` (`float`; default `0.05`)<br>`loss` (`float`; default `0.99999`) | 1 | `available` |
| `MembraneHexagon` | `membrane_hexagon_ar` | `ar` / `audio` | `excitation` (`signal`; default `0`)<br>`tension` (`float`; default `0.05`)<br>`loss` (`float`; default `0.99999`) | 1 | `available` |

## `sc3_ncanalysis.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_ncanalysis.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_ncanalysis.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `LPCAnalyzer` | `lpc_analyzer_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`source` (`signal`; default `0.01`)<br>`n` (`int`; default `256`)<br>`p` (`int`; default `10`)<br>`testE` (`int`; default `0`)<br>`delta` (`float`; default `0.999`)<br>`windowtype` (`int`; default `0`) | 1 | `available` |
| `MedianSeparation` | `median_separation_kr` | `kr` / `control` | `fft` (`signal`; default `0`)<br>`fftharmonic` (`int`; default `0`)<br>`fftpercussive` (`int`; default `0`)<br>`fftsize` (`int`; default `1024`)<br>`mediansize` (`int`; default `17`)<br>`hardorsoft` (`int`; default `0`)<br>`p` (`float`; default `2.0`)<br>`medianormax` (`int`; default `0`) | 2 | `available` |
| `SMS` | `sms_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`maxpeaks` (`int`; default `80`)<br>`currentpeaks` (`int`; default `80`)<br>`tolerance` (`int`; default `4`)<br>`noisefloor` (`float`; default `0.2`)<br>`freqmult` (`float`; default `1.0`)<br>`freqadd` (`float`; default `0.0`)<br>`formantpreserve` (`int`; default `0`)<br>`useifft` (`int`; default `0`)<br>`ampmult` (`float`; default `1.0`)<br>`graphicsbufnum` (`int`; default `-1`) | 2 | `available` |
| `TPV` | `tpv_ar` | `ar` / `audio` | `chain` (`signal`; default `0`)<br>`windowsize` (`int`; default `1024`)<br>`hopsize` (`int`; default `512`)<br>`maxpeaks` (`int`; default `80`)<br>`currentpeaks` (`int`; default `80`)<br>`freqmult` (`float`; default `1.0`)<br>`tolerance` (`int`; default `4`)<br>`noisefloor` (`float`; default `0.2`) | 1 | `available` |
| `WalshHadamard` | `walsh_hadamard_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`which` (`int`; default `0`) | 1 | `available` |
| `WaveletDaub` | `wavelet_daub_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`n` (`int`; default `64`)<br>`which` (`int`; default `0`) | 1 | `available` |

## `sc3_neuromodules.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_neuromodules.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_neuromodules.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Dneuromodule` | `dneuromodule` | `builder` / `audio` | `.numChannels(n)` (`method`; default `0`)<br>`.theta(array)` (`method`; default `0`)<br>`.x(array)` (`method`; default `0`)<br>`.weights(array)` (`method`; default `0`) | 1 | `documentation_only` |

## `sc3_nh_hall.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_nh_hall.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_nh_hall.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `NHHall` | `nh_hall_ar` | `ar` / `audio` | `inLeft` (`signal`; default `0`)<br>`inRight` (`signal`; default `0`)<br>`rt60` (`float`; default `1.0`)<br>`stereo` (`float`; default `0.5`)<br>`lowFreq` (`float`; default `200.0`)<br>`lowRatio` (`float`; default `0.5`)<br>`hiFreq` (`float`; default `4000.0`)<br>`hiRatio` (`float`; default `0.5`)<br>`earlyDiffusion` (`float`; default `0.5`)<br>`lateDiffusion` (`float`; default `0.5`)<br>`modRate` (`float`; default `0.2`)<br>`modDepth` (`float`; default `0.3`) | 2 | `available` |

## `sc3_otey_piano.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_otey_piano.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_otey_piano.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `OteyPiano` | `otey_piano_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`vel` (`float`; default `1.0`)<br>`t_gate` (`signal`; default `0.0`)<br>`rmin` (`float`; default `0.35`)<br>`rmax` (`float`; default `2.0`)<br>`rampl` (`float`; default `4.0`)<br>`rampr` (`float`; default `8.0`)<br>`rcore` (`float`; default `1.0`)<br>`lmin` (`float`; default `0.07`)<br>`lmax` (`float`; default `1.4`)<br>`lampl` (`float`; default `-4.0`)<br>`lampr` (`float`; default `4.0`)<br>`rho` (`float`; default `1.0`)<br>`e` (`float`; default `1.0`)<br>`zb` (`float`; default `1.0`)<br>`zh` (`float`; default `0.0`)<br>`mh` (`float`; default `1.0`)<br>`k` (`float`; default `0.2`)<br>`alpha` (`float`; default `1.0`)<br>`p` (`float`; default `1.0`)<br>`hpos` (`float`; default `0.142`)<br>`loss` (`float`; default `1.0`)<br>`detune` (`float`; default `0.0003`)<br>`hammer_type` (`float`; default `1.0`) | 1 | `available` |

## `sc3_pitch_detection.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_pitch_detection.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_pitch_detection.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Qitch` | `qitch_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`databufnum` (`float`; default `0`)<br>`ampThreshold` (`float`; default `0.01`)<br>`algoflag` (`float`; default `1`)<br>`ampbufnum` (`float`; default `-1`)<br>`minfreq` (`float`; default `0`)<br>`maxfreq` (`float`; default `2500`) | 2 | `available` |
| `Tartini` | `tartini_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`threshold` (`float`; default `0.93`)<br>`n` (`float`; default `2048`)<br>`k` (`float`; default `0`)<br>`overlap` (`float`; default `1024`)<br>`smallCutoff` (`float`; default `0.5`) | 2 | `available` |

## `sc3_quantity.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_quantity.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_quantity.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `MovingAverage` | `moving_average_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`)<br>`maxsamp` (`int`; default `400`) | 1 | `available` |
| `MovingAverage` | `moving_average_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`)<br>`maxsamp` (`int`; default `400`) | 1 | `available` |
| `MovingSum` | `moving_sum_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`)<br>`maxsamp` (`int`; default `400`) | 1 | `available` |
| `MovingSum` | `moving_sum_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `40`)<br>`maxsamp` (`int`; default `400`) | 1 | `available` |

## `sc3_rfw.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_rfw.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_rfw.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AverageOutput` | `average_output_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `AverageOutput` | `average_output_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `SwitchDelay` | `switch_delay_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`drylevel` (`float`; default `1.0`)<br>`wetlevel` (`float`; default `1.0`)<br>`delaytime` (`signal`; default `1.0`)<br>`delayfactor` (`float`; default `0.7`)<br>`maxdelaytime` (`float`; default `20.0`) | 1 | `available` |

## `sc3_rmeqsuite.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_rmeqsuite.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_rmeqsuite.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Allpass1` | `allpass1_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200.0`) | 1 | `available` |
| `Allpass2` | `allpass2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `1200.0`)<br>`rq` (`float`; default `1.0`) | 1 | `available` |
| `RMEQ` | `rmeq_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440.0`)<br>`rq` (`float`; default `0.1`)<br>`k` (`float`; default `0`) | 1 | `available` |
| `RMShelf` | `rm_shelf_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440.0`)<br>`k` (`float`; default `0`) | 1 | `available` |
| `RMShelf2` | `rm_shelf2_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`freq` (`float`; default `440.0`)<br>`k` (`float`; default `0`) | 1 | `available` |
| `Spreader` | `spreader_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`theta` (`float`; default `1.5707963267949`)<br>`filtsPerOctave` (`int`; default `8`) | 2 | `available` |

## `sc3_scmir.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_scmir.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_scmir.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `AttackSlope` | `attack_slope_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`windowsize` (`int`; default `1024`)<br>`peakpicksize` (`int`; default `20`)<br>`leak` (`float`; default `0.999`)<br>`energythreshold` (`float`; default `0.01`)<br>`sumthreshold` (`float`; default `20.0`)<br>`mingap` (`int`; default `30`)<br>`numslopesaveraged` (`int`; default `10`) | 6 | `available` |
| `BeatStatistics` | `beat_statistics_kr` | `kr` / `control` | `fft` (`signal`; default `0`)<br>`leak` (`float`; default `0.995`)<br>`numpreviousbeats` (`int`; default `4`) | 4 | `available` |
| `Chromagram` | `chromagram_kr` | `kr` / `control` | `fft` (`signal`; default `0`)<br>`fftsize` (`int`; default `2048`)<br>`n` (`int`; default `12`)<br>`tuningbase` (`float`; default `32.703195662575`)<br>`octaves` (`int`; default `8`)<br>`integrationflag` (`int`; default `0`)<br>`coeff` (`float`; default `0.9`)<br>`octaveratio` (`float`; default `2.0`)<br>`perframenormalize` (`int`; default `0`) | 12 | `available` |
| `FeatureSave` | `feature_save` | `builder` / `audio` | `.features(array)` (`method`; default `0`)<br>`.trig(signal)` (`method`; default `0`) | 1 | `documentation_only` |
| `KeyClarity` | `key_clarity_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`keydecay` (`float`; default `2.0`)<br>`chromaleak` (`float`; default `0.5`) | 1 | `available` |
| `KeyMode` | `key_mode_kr` | `kr` / `control` | `chain` (`signal`; default `0`)<br>`keydecay` (`float`; default `2.0`)<br>`chromaleak` (`float`; default `0.5`) | 1 | `available` |
| `OnsetStatistics` | `onset_statistics_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`windowsize` (`float`; default `1.0`)<br>`hopsize` (`float`; default `0.1`) | 3 | `available` |
| `SensoryDissonance` | `sensory_dissonance_kr` | `kr` / `control` | `fft` (`signal`; default `0`)<br>`maxpeaks` (`int`; default `100`)<br>`peakthreshold` (`float`; default `0.1`)<br>`norm` (`float`; default `0.0001`)<br>`clamp` (`float`; default `1.0`) | 1 | `available` |
| `SpectralEntropy` | `spectral_entropy_kr` | `kr` / `control` | `fft` (`signal`; default `0`)<br>`fftsize` (`int`; default `2048`)<br>`numbands` (`int`; default `1`) | 1 | `available` |

## `sc3_sl.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_sl.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_sl.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Breakcore` | `breakcore_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`capturein` (`signal`; default `0`)<br>`capturetrigger` (`signal`; default `0`)<br>`duration` (`float`; default `0.1`)<br>`ampdropout` (`float`; default `0`) | 1 | `available` |
| `Brusselator` | `brusselator_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`rate` (`float`; default `0.01`)<br>`mu` (`float`; default `1.0`)<br>`gamma` (`float`; default `1.0`)<br>`initx` (`float`; default `0.5`)<br>`inity` (`float`; default `0.5`) | 2 | `available` |
| `DoubleWell` | `double_well_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`ratex` (`float`; default `0.01`)<br>`ratey` (`float`; default `0.01`)<br>`f` (`float`; default `1`)<br>`w` (`float`; default `0.001`)<br>`delta` (`float`; default `1`)<br>`initx` (`float`; default `0`)<br>`inity` (`float`; default `0`) | 1 | `available` |
| `DoubleWell2` | `double_well2_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`ratex` (`float`; default `0.01`)<br>`ratey` (`float`; default `0.01`)<br>`f` (`float`; default `1`)<br>`w` (`float`; default `0.001`)<br>`delta` (`float`; default `1`)<br>`initx` (`float`; default `0`)<br>`inity` (`float`; default `0`) | 1 | `available` |
| `DoubleWell3` | `double_well3_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`rate` (`float`; default `0.01`)<br>`f` (`float`; default `0`)<br>`delta` (`float`; default `0.25`)<br>`initx` (`float`; default `0`)<br>`inity` (`float`; default `0`) | 1 | `available` |
| `EnvDetect` | `env_detect_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`attack` (`float`; default `100`)<br>`release` (`float`; default `0`) | 1 | `available` |
| `EnvFollow` | `env_follow_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`decaycoeff` (`float`; default `0.99`) | 1 | `available` |
| `EnvFollow` | `env_follow_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`decaycoeff` (`float`; default `0.99`) | 1 | `available` |
| `FitzHughNagumo` | `fitz_hugh_nagumo_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`rateu` (`float`; default `0.01`)<br>`ratew` (`float`; default `0.01`)<br>`b0` (`float`; default `1`)<br>`b1` (`float`; default `1`)<br>`initu` (`float`; default `0`)<br>`initw` (`float`; default `0`) | 1 | `available` |
| `GravityGrid` | `gravity_grid_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`rate` (`float`; default `0.1`)<br>`newx` (`float`; default `0.0`)<br>`newy` (`float`; default `0.0`)<br>`bufnum` (`float`; default `-1`) | 1 | `available` |
| `GravityGrid2` | `gravity_grid2_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`rate` (`float`; default `0.1`)<br>`newx` (`float`; default `0.0`)<br>`newy` (`float`; default `0.0`)<br>`bufnum` (`float`; default `0`) | 1 | `available` |
| `Instruction` | `instruction_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`) | 1 | `available` |
| `KmeansToBPSet1` | `kmeans_to_bp_set1_ar` | `ar` / `audio` | `freq` (`float`; default `440`)<br>`numdatapoints` (`float`; default `20`)<br>`maxnummeans` (`float`; default `4`)<br>`nummeans` (`float`; default `4`)<br>`tnewdata` (`float`; default `1`)<br>`tnewmeans` (`float`; default `1`)<br>`soft` (`float`; default `1.0`)<br>`bufnum` (`float`; default `-1`) | 1 | `available` |
| `LPCError` | `lpc_error_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`p` (`float`; default `10`) | 1 | `available` |
| `LTI` | `lti_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`bufnuma` (`float`; default `0`)<br>`bufnumb` (`float`; default `1`) | 1 | `available` |
| `Max` | `max_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numsamp` (`float`; default `64`) | 1 | `available` |
| `NL` | `nl_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`bufnuma` (`float`; default `0`)<br>`bufnumb` (`float`; default `1`)<br>`guard1` (`float`; default `1000.0`)<br>`guard2` (`float`; default `100.0`) | 1 | `available` |
| `NL2` | `nl2_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`maxsizea` (`float`; default `10`)<br>`maxsizeb` (`float`; default `10`)<br>`guard1` (`float`; default `1000.0`)<br>`guard2` (`float`; default `100.0`) | 1 | `available` |
| `NTube` | `n_tube_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`lossarray` (`signal`; default `1.0`)<br>`karray` (`signal`; default `0`)<br>`delaylengtharray` (`signal`; default `0`) | 1 | `available` |
| `Oregonator` | `oregonator_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`rate` (`float`; default `0.01`)<br>`epsilon` (`float`; default `1.0`)<br>`mu` (`float`; default `1.0`)<br>`q` (`float`; default `1.0`)<br>`initx` (`float`; default `0.5`)<br>`inity` (`float`; default `0.5`)<br>`initz` (`float`; default `0.5`) | 3 | `available` |
| `PrintVal` | `print_val_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`numblocks` (`float`; default `100`)<br>`id` (`float`; default `0`) | 1 | `available` |
| `SLOnset` | `sl_onset_kr` | `kr` / `control` | `input` (`signal`; default `0`)<br>`memorysize1` (`float`; default `20`)<br>`before` (`float`; default `5`)<br>`after` (`float`; default `5`)<br>`threshold` (`float`; default `10`)<br>`hysteresis` (`float`; default `10`) | 1 | `available` |
| `Sieve1` | `sieve1_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`gap` (`float`; default `2`)<br>`alternate` (`float`; default `1`) | 1 | `available` |
| `Sieve1` | `sieve1_kr` | `kr` / `control` | `bufnum` (`float`; default `0`)<br>`gap` (`float`; default `2`)<br>`alternate` (`float`; default `1`) | 1 | `available` |
| `SortBuf` | `sort_buf_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`sortrate` (`float`; default `10`)<br>`reset` (`float`; default `0`) | 1 | `available` |
| `SpruceBudworm` | `spruce_budworm_ar` | `ar` / `audio` | `reset` (`float`; default `0`)<br>`rate` (`float`; default `0.1`)<br>`k1` (`float`; default `27.9`)<br>`k2` (`float`; default `1.5`)<br>`alpha` (`float`; default `0.1`)<br>`beta` (`float`; default `10.1`)<br>`mu` (`float`; default `0.3`)<br>`rho` (`float`; default `10.1`)<br>`initx` (`float`; default `0.9`)<br>`inity` (`float`; default `0.1`) | 2 | `available` |
| `TermanWang` | `terman_wang_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`reset` (`float`; default `0`)<br>`ratex` (`float`; default `0.01`)<br>`ratey` (`float`; default `0.01`)<br>`alpha` (`float`; default `1.0`)<br>`beta` (`float`; default `1.0`)<br>`eta` (`float`; default `1.0`)<br>`initx` (`float`; default `0`)<br>`inity` (`float`; default `0`) | 1 | `available` |
| `TwoTube` | `two_tube_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`k` (`float`; default `0.01`)<br>`loss` (`float`; default `1.0`)<br>`d1length` (`float`; default `100`)<br>`d2length` (`float`; default `100`) | 1 | `available` |
| `VMScan2D` | `vm_scan2d_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`) | 2 | `available` |
| `WaveTerrain` | `wave_terrain_ar` | `ar` / `audio` | `bufnum` (`float`; default `0`)<br>`x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`xsize` (`float`; default `100`)<br>`ysize` (`float`; default `100`) | 1 | `available` |
| `WeaklyNonlinear` | `weakly_nonlinear_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`reset` (`float`; default `0`)<br>`ratex` (`float`; default `1`)<br>`ratey` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`initx` (`float`; default `0`)<br>`inity` (`float`; default `0`)<br>`alpha` (`float`; default `0`)<br>`xexponent` (`float`; default `0`)<br>`beta` (`float`; default `0`)<br>`yexponent` (`float`; default `0`) | 1 | `available` |
| `WeaklyNonlinear2` | `weakly_nonlinear2_ar` | `ar` / `audio` | `input` (`signal`; default `0`)<br>`reset` (`float`; default `0`)<br>`ratex` (`float`; default `1`)<br>`ratey` (`float`; default `1`)<br>`freq` (`float`; default `440`)<br>`initx` (`float`; default `0`)<br>`inity` (`float`; default `0`)<br>`alpha` (`float`; default `0`)<br>`xexponent` (`float`; default `0`)<br>`beta` (`float`; default `0`)<br>`yexponent` (`float`; default `0`) | 1 | `available` |

## `sc3_stk.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_stk.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_stk.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Sflute` | `sflute_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`pressure` (`float`; default `0.5`)<br>`randamp` (`float`; default `0.1`)<br>`dampcoef` (`float`; default `0.0001`)<br>`lipopen` (`float`; default `20.0`)<br>`jetstream` (`float`; default `0.5`)<br>`fullwave` (`float`; default `1.0`) | 1 | `available` |
| `Sflute` | `sflute_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`pressure` (`float`; default `0.5`)<br>`randamp` (`float`; default `0.1`)<br>`dampcoef` (`float`; default `0.0001`)<br>`lipopen` (`float`; default `20.0`)<br>`jetstream` (`float`; default `0.5`)<br>`fullwave` (`float`; default `1.0`) | 1 | `available` |
| `StkBandedWG` | `stk_banded_wg_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`instr` (`float`; default `0.0`)<br>`bowpressure` (`float`; default `0.0`)<br>`bowmotion` (`float`; default `0.0`)<br>`integration` (`float`; default `0.0`)<br>`modalresonance` (`float`; default `64.0`)<br>`bowvelocity` (`float`; default `0.0`)<br>`setstriking` (`float`; default `0.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkBandedWG` | `stk_banded_wg_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`instr` (`float`; default `0.0`)<br>`bowpressure` (`float`; default `0.0`)<br>`bowmotion` (`float`; default `0.0`)<br>`integration` (`float`; default `0.0`)<br>`modalresonance` (`float`; default `64.0`)<br>`bowvelocity` (`float`; default `0.0`)<br>`setstriking` (`float`; default `0.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkBeeThree` | `stk_bee_three_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`op4gain` (`float`; default `10.0`)<br>`op3gain` (`float`; default `20.0`)<br>`lfospeed` (`float`; default `64.0`)<br>`lfodepth` (`float`; default `0.0`)<br>`adsrtarget` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkBeeThree` | `stk_bee_three_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`op4gain` (`float`; default `10.0`)<br>`op3gain` (`float`; default `20.0`)<br>`lfospeed` (`float`; default `64.0`)<br>`lfodepth` (`float`; default `0.0`)<br>`adsrtarget` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkBlowHole` | `stk_blow_hole_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`reedstiffness` (`float`; default `64.0`)<br>`noisegain` (`float`; default `20.0`)<br>`tonehole` (`float`; default `64.0`)<br>`register` (`float`; default `11.0`)<br>`breathpressure` (`float`; default `64.0`) | 1 | `available` |
| `StkBlowHole` | `stk_blow_hole_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`reedstiffness` (`float`; default `64.0`)<br>`noisegain` (`float`; default `20.0`)<br>`tonehole` (`float`; default `64.0`)<br>`register` (`float`; default `11.0`)<br>`breathpressure` (`float`; default `64.0`) | 1 | `available` |
| `StkBowed` | `stk_bowed_ar` | `ar` / `audio` | `freq` (`float`; default `220.0`)<br>`bowpressure` (`float`; default `64.0`)<br>`bowposition` (`float`; default `64.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `64.0`)<br>`loudness` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkBowed` | `stk_bowed_kr` | `kr` / `control` | `freq` (`float`; default `220.0`)<br>`bowpressure` (`float`; default `64.0`)<br>`bowposition` (`float`; default `64.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `64.0`)<br>`loudness` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkClarinet` | `stk_clarinet_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`reedstiffness` (`float`; default `64.0`)<br>`noisegain` (`float`; default `4.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `11.0`)<br>`breathpressure` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkClarinet` | `stk_clarinet_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`reedstiffness` (`float`; default `64.0`)<br>`noisegain` (`float`; default `4.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `11.0`)<br>`breathpressure` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkFlute` | `stk_flute_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`jetDelay` (`float`; default `49.0`)<br>`noisegain` (`float`; default `0.15`)<br>`jetRatio` (`float`; default `0.32`) | 1 | `available` |
| `StkFlute` | `stk_flute_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`jetDelay` (`float`; default `49.0`)<br>`noisegain` (`float`; default `0.15`)<br>`jetRatio` (`float`; default `0.32`) | 1 | `available` |
| `StkMandolin` | `stk_mandolin_ar` | `ar` / `audio` | `freq` (`float`; default `520.0`)<br>`bodysize` (`float`; default `64.0`)<br>`pickposition` (`float`; default `64.0`)<br>`stringdamping` (`float`; default `69.0`)<br>`stringdetune` (`float`; default `10.0`)<br>`aftertouch` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkMandolin` | `stk_mandolin_kr` | `kr` / `control` | `freq` (`float`; default `520.0`)<br>`bodysize` (`float`; default `64.0`)<br>`pickposition` (`float`; default `64.0`)<br>`stringdamping` (`float`; default `69.0`)<br>`stringdetune` (`float`; default `10.0`)<br>`aftertouch` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkModalBar` | `stk_modal_bar_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`instrument` (`float`; default `0.0`)<br>`stickhardness` (`float`; default `64.0`)<br>`stickposition` (`float`; default `64.0`)<br>`vibratogain` (`float`; default `20.0`)<br>`vibratofreq` (`float`; default `20.0`)<br>`directstickmix` (`float`; default `64.0`)<br>`volume` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkModalBar` | `stk_modal_bar_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`instrument` (`float`; default `0.0`)<br>`stickhardness` (`float`; default `64.0`)<br>`stickposition` (`float`; default `64.0`)<br>`vibratogain` (`float`; default `20.0`)<br>`vibratofreq` (`float`; default `20.0`)<br>`directstickmix` (`float`; default `64.0`)<br>`volume` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkMoog` | `stk_moog_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`filterQ` (`float`; default `10.0`)<br>`sweeprate` (`float`; default `20.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `0.0`)<br>`gain` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkMoog` | `stk_moog_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`filterQ` (`float`; default `10.0`)<br>`sweeprate` (`float`; default `20.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `0.0`)<br>`gain` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkPluck` | `stk_pluck_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`decay` (`float`; default `0.99`) | 1 | `available` |
| `StkPluck` | `stk_pluck_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`decay` (`float`; default `0.99`) | 1 | `available` |
| `StkSaxofony` | `stk_saxofony_ar` | `ar` / `audio` | `freq` (`float`; default `220.0`)<br>`reedstiffness` (`float`; default `64.0`)<br>`reedaperture` (`float`; default `64.0`)<br>`noisegain` (`float`; default `20.0`)<br>`blowposition` (`float`; default `26.0`)<br>`vibratofrequency` (`float`; default `20.0`)<br>`vibratogain` (`float`; default `20.0`)<br>`breathpressure` (`float`; default `128.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkSaxofony` | `stk_saxofony_kr` | `kr` / `control` | `freq` (`float`; default `220.0`)<br>`reedstiffness` (`float`; default `64.0`)<br>`reedaperture` (`float`; default `64.0`)<br>`noisegain` (`float`; default `20.0`)<br>`blowposition` (`float`; default `26.0`)<br>`vibratofrequency` (`float`; default `20.0`)<br>`vibratogain` (`float`; default `20.0`)<br>`breathpressure` (`float`; default `128.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkShakers` | `stk_shakers_ar` | `ar` / `audio` | `instr` (`float`; default `0.0`)<br>`energy` (`float`; default `64.0`)<br>`decay` (`float`; default `64.0`)<br>`objects` (`float`; default `64.0`)<br>`resfreq` (`float`; default `64.0`) | 1 | `available` |
| `StkShakers` | `stk_shakers_kr` | `kr` / `control` | `instr` (`float`; default `0.0`)<br>`energy` (`float`; default `64.0`)<br>`decay` (`float`; default `64.0`)<br>`objects` (`float`; default `64.0`)<br>`resfreq` (`float`; default `64.0`) | 1 | `available` |
| `StkSitar` | `stk_sitar_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkSitar` | `stk_sitar_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkStifKarp` | `stk_stif_karp_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`gain` (`float`; default `1.0`)<br>`pickuppos` (`float`; default `0.0`)<br>`stringsustain` (`float`; default `0.0`)<br>`stringstretch` (`float`; default `0.0`) | 1 | `available` |
| `StkStifKarp` | `stk_stif_karp_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`gain` (`float`; default `1.0`)<br>`pickuppos` (`float`; default `0.0`)<br>`stringsustain` (`float`; default `0.0`)<br>`stringstretch` (`float`; default `0.0`) | 1 | `available` |
| `StkTubeBell` | `stk_tube_bell_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`) | 1 | `available` |
| `StkTubeBell` | `stk_tube_bell_kr` | `kr` / `control` | `freq` (`float`; default `440.0`) | 1 | `available` |
| `StkVoicForm` | `stk_voic_form_ar` | `ar` / `audio` | `freq` (`float`; default `440.0`)<br>`vuvmix` (`float`; default `64.0`)<br>`vowelphon` (`float`; default `64.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `20.0`)<br>`loudness` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |
| `StkVoicForm` | `stk_voic_form_kr` | `kr` / `control` | `freq` (`float`; default `440.0`)<br>`vuvmix` (`float`; default `64.0`)<br>`vowelphon` (`float`; default `64.0`)<br>`vibfreq` (`float`; default `64.0`)<br>`vibgain` (`float`; default `20.0`)<br>`loudness` (`float`; default `64.0`)<br>`trig` (`signal`; default `1`) | 1 | `available` |

## `sc3_summer.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_summer.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_summer.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Summer` | `summer_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`step` (`float`; default `1.0`)<br>`reset` (`signal`; default `0`)<br>`resetval` (`float`; default `0`) | 1 | `available` |
| `Summer` | `summer_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`step` (`float`; default `1.0`)<br>`reset` (`signal`; default `0`)<br>`resetval` (`float`; default `0`) | 1 | `available` |
| `WrapSummer` | `wrap_summer_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`step` (`float`; default `1.0`)<br>`min` (`float`; default `0`)<br>`max` (`float`; default `1.0`)<br>`reset` (`signal`; default `0`)<br>`resetval` (`float`; default `0`) | 1 | `available` |
| `WrapSummer` | `wrap_summer_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`step` (`float`; default `1.0`)<br>`min` (`float`; default `0`)<br>`max` (`float`; default `1.0`)<br>`reset` (`signal`; default `0`)<br>`resetval` (`float`; default `0`) | 1 | `available` |

## `sc3_tag_system.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_tag_system.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_tag_system.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `DbufTag` | `dbuf_tag` | `builder` / `audio` | `.bufnum(num)` (`method`; default `0`)<br>`.v(num)` (`method`; default `0`)<br>`.axiom(array)` (`method`; default `0`)<br>`.rules(array_of_arrays)` (`method`; default `0`)<br>`.recycle(offset)` (`method`; default `0`)<br>`.mode(m)` (`method`; default `0`) | 1 | `documentation_only` |
| `Dfsm` | `dfsm` | `builder` / `audio` | `.rules(array)` (`method`; default `0`)<br>`.n(count)` (`method`; default `0`)<br>`.rgen(ugen)` (`method`; default `0`) | 1 | `documentation_only` |
| `Dtag` | `dtag` | `builder` / `audio` | `.bufsize(size)` (`method`; default `0`)<br>`.v(num)` (`method`; default `0`)<br>`.axiom(array)` (`method`; default `0`)<br>`.rules(array_of_arrays)` (`method`; default `0`)<br>`.recycle(offset)` (`method`; default `0`)<br>`.mode(m)` (`method`; default `0`) | 1 | `documentation_only` |

## `sc3_vbap.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_vbap.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_vbap.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `CircleRamp` | `circle_ramp_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`)<br>`circmin` (`float`; default `-180`)<br>`circmax` (`float`; default `180`) | 1 | `available` |
| `CircleRamp` | `circle_ramp_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lagTime` (`float`; default `0.1`)<br>`circmin` (`float`; default `-180`)<br>`circmax` (`float`; default `180`) | 1 | `available` |
| `VBAP` | `vbap_ar` | `ar` / `audio` | `numChans` (`float`; default `4`)<br>`in` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`spread` (`float`; default `0`) | 4 | `available` |
| `VBAP` | `vbap_kr` | `kr` / `control` | `numChans` (`float`; default `4`)<br>`in` (`signal`; default `0`)<br>`bufnum` (`float`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`spread` (`float`; default `0`) | 4 | `available` |
| `VBAPSpeaker` | `vbap_speaker` | `builder` / `audio` | `.azimuth(deg)` (`method`; default `"0"`)<br>`.elevation(deg)` (`method`; default `"0"`) | 1 | `documentation_only` — Client-side data/helper class for VBAPSpeakerArray; no scsynth UGen is emitted. |
| `VBAPSpeakerArray` | `vbap_speaker_array` | `builder` / `audio` | `.dim(n)` (`method`; default `"2"`)<br>`.speakers(directions)` (`method`; default `0`)<br>`.add_speaker(speaker)` (`method`; default `0`)<br>`.load_to_buffer()` (`method`; default `0`) | 1 | `documentation_only` — Client-side buffer-geometry helper; no scsynth UGen is emitted. |

## `sc3_vosim.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc3_vosim.json`](../../../crates/vibelang-dsp/ugen_manifests/sc3_vosim.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `VOSIM` | `vosim_ar` | `ar` / `audio` | `trig` (`signal`; default `0.1`)<br>`freq` (`signal`; default `400.0`)<br>`nCycles` (`int`; default `1`)<br>`decay` (`float`; default `0.9`) | 1 | `available` |

## `sc_hoa.json`

Source: [`crates/vibelang-dsp/ugen_manifests/sc_hoa.json`](../../../crates/vibelang-dsp/ugen_manifests/sc_hoa.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `HOAAzimuthRotator1` | `hoa_azimuth_rotator1_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`az` (`float`; default `0`) | 4 | `available` |
| `HOAAzimuthRotator2` | `hoa_azimuth_rotator2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`az` (`float`; default `0`) | 9 | `available` |
| `HOABeamDirac2HOA1` | `hoa_beam_dirac2hoa1_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`gain` (`float`; default `0`)<br>`on` (`float`; default `1`)<br>`timer_manual` (`float`; default `0`)<br>`crossfade` (`float`; default `1`)<br>`focus` (`float`; default `0`) | 4 | `available` |
| `HOABeamDirac2HOA2` | `hoa_beam_dirac2hoa2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`gain` (`float`; default `0`)<br>`on` (`float`; default `1`)<br>`timer_manual` (`float`; default `0`)<br>`crossfade` (`float`; default `1`)<br>`focus` (`float`; default `0`) | 9 | `available` |
| `HOABeamHCardio2HOA1` | `hoa_beam_h_cardio2hoa1_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`int_float` (`float`; default `0`)<br>`order` (`float`; default `1`) | 4 | `available` |
| `HOABeamHCardio2HOA2` | `hoa_beam_h_cardio2hoa2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`int_float` (`float`; default `0`)<br>`order` (`float`; default `1`) | 9 | `available` |
| `HOABeamHCardio2Mono1` | `hoa_beam_h_cardio2_mono1_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`output_gain` (`float`; default `0`) | 1 | `available` |
| `HOABeamHCardio2Mono2` | `hoa_beam_h_cardio2_mono2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`azimuth` (`float`; default `0`)<br>`elevation` (`float`; default `0`)<br>`output_gain` (`float`; default `0`) | 1 | `available` |
| `HOADecLebedev061` | `hoa_dec_lebedev061_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`inputs_gain` (`float`; default `0`)<br>`outputs_gain` (`float`; default `0`)<br>`yes` (`float`; default `1`)<br>`speakers_radius` (`float`; default `1`) | 6 | `available` |
| `HOADecLebedev262` | `hoa_dec_lebedev262_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`inputs_gain` (`float`; default `0`)<br>`outputs_gain` (`float`; default `0`)<br>`yes` (`float`; default `1`)<br>`speakers_radius` (`float`; default `1`) | 26 | `available` |
| `HOADecLebedev501` | `hoa_dec_lebedev501_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`inputs_gain` (`float`; default `0`)<br>`outputs_gain` (`float`; default `0`)<br>`yes` (`float`; default `1`)<br>`speakers_radius` (`float`; default `1`) | 50 | `available` |
| `HOADecLebedev502` | `hoa_dec_lebedev502_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`inputs_gain` (`float`; default `0`)<br>`outputs_gain` (`float`; default `0`)<br>`yes` (`float`; default `1`)<br>`speakers_radius` (`float`; default `1`) | 50 | `available` |
| `HOAEncLebedev061` | `hoa_enc_lebedev061` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`gain` (`float`; default `0`) | 4 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOAEncoder1` | `hoa_encoder1_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`gain_0` (`float`; default `0`)<br>`radius_0` (`float`; default `2`)<br>`azimuth_0` (`float`; default `0`)<br>`elevation_0` (`float`; default `0`)<br>`plane_spherical` (`float`; default `0`)<br>`speaker_radius_0` (`float`; default `1.07`) | 4 | `available` |
| `HOAEncoder2` | `hoa_encoder2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`gain_0` (`float`; default `0`)<br>`radius_0` (`float`; default `2`)<br>`azimuth_0` (`float`; default `0`)<br>`elevation_0` (`float`; default `0`)<br>`plane_spherical` (`float`; default `0`)<br>`speaker_radius_0` (`float`; default `1.07`) | 9 | `available` |
| `HOAEncoder3` | `hoa_encoder3_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`gain_0` (`float`; default `0`)<br>`radius_0` (`float`; default `2`)<br>`azimuth_0` (`float`; default `0`)<br>`elevation_0` (`float`; default `0`)<br>`plane_spherical` (`float`; default `0`)<br>`speaker_radius_0` (`float`; default `1.07`) | 16 | `available` |
| `HOAEncoder4` | `hoa_encoder4_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`gain_0` (`float`; default `0`)<br>`radius_0` (`float`; default `2`)<br>`azimuth_0` (`float`; default `0`)<br>`elevation_0` (`float`; default `0`)<br>`plane_spherical` (`float`; default `0`)<br>`speaker_radius_0` (`float`; default `1.07`) | 25 | `available` |
| `HOAEncoder5` | `hoa_encoder5_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`gain_0` (`float`; default `0`)<br>`radius_0` (`float`; default `2`)<br>`azimuth_0` (`float`; default `0`)<br>`elevation_0` (`float`; default `0`)<br>`plane_spherical` (`float`; default `0`)<br>`speaker_radius_0` (`float`; default `1.07`) | 36 | `available` |
| `HOALibEnc3D1` | `hoa_lib_enc3d1` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 4 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOALibEnc3D2` | `hoa_lib_enc3d2` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 9 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOALibEnc3D3` | `hoa_lib_enc3d3` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 16 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOALibEnc3D4` | `hoa_lib_enc3d4` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 25 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOALibEnc3D5` | `hoa_lib_enc3d5` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 36 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOAMirror1` | `hoa_mirror1_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`front_back` (`float`; default `0`)<br>`left_right` (`float`; default `0`)<br>`up_down` (`float`; default `0`) | 4 | `available` |
| `HOAMirror2` | `hoa_mirror2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`front_back` (`float`; default `0`)<br>`left_right` (`float`; default `0`)<br>`up_down` (`float`; default `0`) | 9 | `available` |
| `HOARotator1` | `hoa_rotator1_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`pitch` (`float`; default `0`)<br>`roll` (`float`; default `0`)<br>`yaw` (`float`; default `0`) | 4 | `available` |
| `HOARotator2` | `hoa_rotator2_ar` | `ar` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`pitch` (`float`; default `0`)<br>`roll` (`float`; default `0`)<br>`yaw` (`float`; default `0`) | 9 | `available` |
| `HOAmbiPanner1` | `ho_ambi_panner1` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 4 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOAmbiPanner2` | `ho_ambi_panner2` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 9 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOAmbiPanner3` | `ho_ambi_panner3` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 16 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOAmbiPanner4` | `ho_ambi_panner4` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 25 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `HOAmbiPanner5` | `ho_ambi_panner5` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`azi` (`float`; default `0`)<br>`ele` (`float`; default `0`) | 36 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `ITU5001` | `itu5001` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`gain` (`float`; default `1`)<br>`lf_hf` (`float`; default `0`)<br>`mute` (`float`; default `0`)<br>`xover` (`float`; default `400`) | 5 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |
| `ITU5002` | `itu5002` | `builder` / `audio` | `in1` (`signal`; default `0`)<br>`in2` (`signal`; default `0`)<br>`in3` (`signal`; default `0`)<br>`in4` (`signal`; default `0`)<br>`in5` (`signal`; default `0`)<br>`in6` (`signal`; default `0`)<br>`in7` (`signal`; default `0`)<br>`in8` (`signal`; default `0`)<br>`in9` (`signal`; default `0`)<br>`gain` (`float`; default `1`)<br>`lf_hf` (`float`; default `0`)<br>`mute` (`float`; default `0`)<br>`xover` (`float`; default `400`) | 5 | `documentation_only` — Stale or uninstalled SC-HOA wrapper/helper name; no installed binary registers this UGen. |

## `triggers.json`

Source: [`crates/vibelang-dsp/ugen_manifests/triggers.json`](../../../crates/vibelang-dsp/ugen_manifests/triggers.json)

| Class | Identity | Source/runtime rate | Inputs | Outputs | Availability |
|---|---|---|---|---:|---|
| `Changed` | `changed_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`threshold` (`float`; default `0`) | 1 | `available` |
| `Changed` | `changed_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`threshold` (`float`; default `0`) | 1 | `available` |
| `Done` | `done_kr` | `kr` / `control` | `src` (`signal`; default `0`) | 1 | `available` |
| `Free` | `free_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`id` (`float`; default `0`) | 1 | `available` |
| `FreeSelf` | `free_self_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `FreeSelfWhenDone` | `free_self_when_done_kr` | `kr` / `control` | `src` (`signal`; default `0`) | 1 | `available` |
| `Gate` | `gate_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `Gate` | `gate_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `InRange` | `in_range_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `InRange` | `in_range_ir` | `ir` / `scalar` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `InRange` | `in_range_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `InRect` | `in_rect_ar` | `ar` / `audio` | `x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`rect` (`signal`; default `0`) | 1 | `available` |
| `InRect` | `in_rect_ir` | `ir` / `scalar` | `x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`rect` (`signal`; default `0`) | 1 | `available` |
| `InRect` | `in_rect_kr` | `kr` / `control` | `x` (`signal`; default `0`)<br>`y` (`signal`; default `0`)<br>`rect` (`signal`; default `0`) | 1 | `available` |
| `LastValue` | `last_value_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`diff` (`float`; default `0.01`) | 1 | `available` |
| `LastValue` | `last_value_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`diff` (`float`; default `0.01`) | 1 | `available` |
| `Latch` | `latch_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `Latch` | `latch_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `LeastChange` | `least_change_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `LeastChange` | `least_change_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `MostChange` | `most_change_ar` | `ar` / `audio` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `MostChange` | `most_change_kr` | `kr` / `control` | `a` (`signal`; default `0`)<br>`b` (`signal`; default `0`) | 1 | `available` |
| `Pause` | `pause_kr` | `kr` / `control` | `gate` (`signal`; default `1`)<br>`id` (`float`; default `0`) | 1 | `available` |
| `PauseSelf` | `pause_self_kr` | `kr` / `control` | `in` (`signal`; default `0`) | 1 | `available` |
| `PauseSelfWhenDone` | `pause_self_when_done_kr` | `kr` / `control` | `src` (`signal`; default `0`) | 1 | `available` |
| `Peak` | `peak_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `Peak` | `peak_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`trig` (`signal`; default `0`) | 1 | `available` |
| `Poll` | `poll_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`in` (`signal`; default `0`)<br>`label` (`float`; default `0`)<br>`trigid` (`float`; default `-1`) | 1 | `available` |
| `Poll` | `poll_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`in` (`signal`; default `0`)<br>`label` (`float`; default `0`)<br>`trigid` (`float`; default `-1`) | 1 | `available` |
| `PulseCount` | `pulse_count_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`) | 1 | `available` |
| `PulseCount` | `pulse_count_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`) | 1 | `available` |
| `PulseDivider` | `pulse_divider_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`div` (`float`; default `2`)<br>`start` (`float`; default `0`) | 1 | `available` |
| `PulseDivider` | `pulse_divider_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`div` (`float`; default `2`)<br>`start` (`float`; default `0`) | 1 | `available` |
| `Schmidt` | `schmidt_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `Schmidt` | `schmidt_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`lo` (`float`; default `0`)<br>`hi` (`float`; default `1`) | 1 | `available` |
| `SendPeakRMS` | `send_peak_rms_ar` | `ar` / `audio` | `sig` (`signal`; default `0`)<br>`replyRate` (`float`; default `20`)<br>`peakLag` (`float`; default `3`)<br>`cmdName` (`float`; default `0`)<br>`replyID` (`float`; default `-1`) | 0 | `available` |
| `SendPeakRMS` | `send_peak_rms_kr` | `kr` / `control` | `sig` (`signal`; default `0`)<br>`replyRate` (`float`; default `20`)<br>`peakLag` (`float`; default `3`)<br>`cmdName` (`float`; default `0`)<br>`replyID` (`float`; default `-1`) | 0 | `available` |
| `SendReply` | `send_reply_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`cmdName` (`float`; default `0`)<br>`values` (`signal`; default `0`)<br>`replyID` (`float`; default `-1`) | 0 | `available` |
| `SendReply` | `send_reply_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`cmdName` (`float`; default `0`)<br>`values` (`signal`; default `0`)<br>`replyID` (`float`; default `-1`) | 0 | `available` |
| `SendTrig` | `send_trig_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`id` (`float`; default `0`)<br>`value` (`float`; default `0`) | 0 | `available` |
| `SendTrig` | `send_trig_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`id` (`float`; default `0`)<br>`value` (`float`; default `0`) | 0 | `available` |
| `SetResetFF` | `set_reset_ff_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`) | 1 | `available` |
| `SetResetFF` | `set_reset_ff_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`) | 1 | `available` |
| `Stepper` | `stepper_ar` | `ar` / `audio` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`)<br>`min` (`float`; default `0`)<br>`max` (`float`; default `7`)<br>`step` (`float`; default `1`)<br>`resetval` (`float`; default `0`) | 1 | `available` |
| `Stepper` | `stepper_kr` | `kr` / `control` | `trig` (`signal`; default `0`)<br>`reset` (`signal`; default `0`)<br>`min` (`float`; default `0`)<br>`max` (`float`; default `7`)<br>`step` (`float`; default `1`)<br>`resetval` (`float`; default `0`) | 1 | `available` |
| `TDelay` | `t_delay_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`dur` (`float`; default `0.1`) | 1 | `available` |
| `TDelay` | `t_delay_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`dur` (`float`; default `0.1`) | 1 | `available` |
| `ToggleFF` | `toggle_ff_ar` | `ar` / `audio` | `trig` (`signal`; default `0`) | 1 | `available` |
| `ToggleFF` | `toggle_ff_kr` | `kr` / `control` | `trig` (`signal`; default `0`) | 1 | `available` |
| `Trig` | `trig_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`dur` (`float`; default `0.1`) | 1 | `available` |
| `Trig` | `trig_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`dur` (`float`; default `0.1`) | 1 | `available` |
| `Trig1` | `trig1_ar` | `ar` / `audio` | `in` (`signal`; default `0`)<br>`dur` (`float`; default `0.1`) | 1 | `available` |
| `Trig1` | `trig1_kr` | `kr` / `control` | `in` (`signal`; default `0`)<br>`dur` (`float`; default `0.1`) | 1 | `available` |
| `TrigControl` | `trig_control_kr` | `kr` / `control` | none | 1 | `available` |
