# Voice Multi-Output Routing — How-To

> User manual for declaring named output ports on a synthdef and routing
> them per-port from `.vibe` scripts. Pairs with the deeper proposals in
> `kb/voice-multi-output-cv-routing-plan.md` (v1) and
> `kb/voice-multi-output-v2-plan.md` (v2).
>
> **Status snapshot:** v1 (audio-only) + v2 (kr-rate ports, CV-to-param
> routing) shipped. The legacy `modulator` builder + `voice.modulate`
> surface has been removed — the canonical kr-routing surfaces are
> `.to_param` (source-first SET) and `.param(...).modulate_by(...)`
> (target-first BEND), see §3c. The per-port FX feature was implemented
> and then reverted: sub-group routing covers every use case with shared
> FX state and fewer buses (see §3b). The remaining v3 work
> — ar→param coercion, fan-out to multiple groups — is documented in §8.

A vibelang voice can expose **N named output ports**. Each port is an
independent signal that the script routes wherever it likes — to a group
mix bus, straight to the main hardware output, or muted. One synthdef can
fan out to several destinations without splitting it into sibling voices.

Worked example: `examples/spectraphon_multiout.vibe` (committed at
`2f03112`) wires the four ports of `spectraphon_side` (`sine`, `sub`,
`odd`, `even`) into different destinations — clean fundamental and
sub-CV to main, odd partials to a dry mix group, even partials through
a dedicated reverb group.

---

## 1. Declaring ports in a synthdef

Ports are declared on the `define_synthdef` builder before `body` /
`body_map`. Two shapes:

```rhai
define_synthdef("my_synth", |builder| {
    builder
        .output("left")              // 1-channel ar port (default)
        .output("right")
        .output("verb_send", 2)      // 2-channel (stereo) ar port
        .output_kr("env_cv")         // 1-channel kr port (CV tap)
        .param("freq", 220.0)
        .body_map(|p| {
            // ... DSP graph ...
            // Body returns one signal per declared port, in order:
            [left_sig, right_sig, verb_send_stereo_sig, env_kr_sig]
        })
});
```

Rules:

* **One signal per port**, in declared order. Mono ports take a
  scalar signal, stereo ports take a `[L, R]` array.
* **`.output(name)`** — single-channel **audio-rate (ar)** port. The vast
  majority of audio ports — mono mix sends, individual oscillator taps —
  are one-channel.
* **`.output(name, channels)`** — explicit channel count, `1..=255`.
  Use `2` for proper stereo ports (e.g. a reverb send that is intrinsically
  stereo). Channels above 2 are valid but no terminal verb yet does
  anything meaningful with them — see §9.
* **`.output_kr(name)` / `.output_kr(name, channels)`** — single- or
  multi-channel **control-rate (kr)** port. The body's signal for a kr
  port is rendered via `Out.kr` and lives on a control bus. Use kr ports
  for CV-style outputs that drive params on other voices (envelopes,
  LFOs, S&H pulses) — the port's bus rate matches scsynth's `/n_map`
  semantics for parameter mapping (see §3c). Mixed-rate synthdefs are
  allowed — declare ar ports for audio and kr ports for CV in the same
  builder. Spectraphon-style synths with audio outs *and* a kr CV tap
  work out of the box.
* **Names are unique within a synthdef** across rates. Empty names and
  duplicates are rejected at synthdef-load time.
* **Legacy synthdefs** that don't call `.output(...)` keep the implicit
  single port `("out", 2 channels, ar)` — i.e. existing stereo
  `Out.ar(out, [L,R])` synthdefs work unchanged.

> Source: `crates/vibelang-dsp/src/builder.rs::SynthDefBuilder::output{,_kr}` /
> `crates/vibelang-dsp/src/api.rs::SynthDefBuilderHandle::output{,_with_channels,_kr,_kr_with_channels}`.

---

## 2. Default routing rule (count-based)

When the script declares **zero** routes for a voice, vibelang installs a
count-based default so the synth still makes sound. The rule is purely a
function of the declared port count:

| Port count | Default behaviour |
|---|---|
| 0 | No ports — voice produces no audio (degenerate). |
| 1 | The single port routes to the voice's group bus. |
| 2 | Both ports route to the voice's group bus (treated as L/R when summed). |
| N > 2 | The **first two** ports route to the group bus; **all remaining ports are silent** until the script routes them. |

Why "first two then silent": the common pattern is that the first ports
in declaration order are the canonical mix output (e.g. `sine` first),
and any extras are CV-style or specialised sends that should not leak
into the main mix without explicit user intent.

> Source: `crates/vibelang-core/src/handlers/routes.rs::default_routes_for_voice`.

Default routes can be **overridden** by an explicit `voice.output(...)`
call — explicit routes always win over the default in the merge step
(`merge_default_routes`).

---

## 3. Overriding routes — terminal verbs

`voice.output(name)` returns a chainable `RouteHandle`. Pick exactly one
terminal verb to commit the route:

| Verb | Effect | Port rate |
|---|---|---|
| `.to(group)` | Sum this port into the named group's mix bus. | ar only |
| `.to_main()` | Send straight to main hardware output (bus 0), bypassing groups. | ar only |
| `.to_current_group()` | Sum into whichever group the voice was last `.group("name")`-ed into. Errors when the voice has no explicit group set. | ar only |
| `.mute()` | Discard the port's signal. | ar or kr |
| `.to_param(target, "param")` | **Source-first (SET).** Map this port's control bus into `target.param` via scsynth `/n_map` (see §3c). | kr only |

The dual surface lives on the **target** voice via
`voice.param("name")`, which returns a `ParamHandle`:

| Verb | Effect | Port rate |
|---|---|---|
| `.modulate_by(source, "port")` | **Target-first (BEND).** Same registry entry as `.to_param`, written from the receiving side. Multi-source fan-in to one param is supported here (see §3c). | kr only |

To send a port through an FX chain, route it into a sub-group that owns
the effects. The sub-group's mix bus auto-mixes back into its parent
group, so a per-port FX wet-send is just two routes (one for the dry
port, one for the FX-bound port → sub-group). See §3b.

```rhai
let v = voice("spec")
    .synth("spectraphon_side")
    .group("leads");

v.output("sine").to_main();                 // dry fundamental → main
v.output("sub").to_main();                  // CV-style sub → main
v.output("odd").to_current_group();         // == .to(group("leads"))
v.output("even").to(group("fx_evens"));     // distinct FX bus
```

Re-routing the same `(voice, port)` pair **replaces** the prior
destination — semantics inherited from `HashMap::insert` on the routes
map. Additive fan-out (one port → many destinations) is deferred; if you
need that today, use a second voice on the same synthdef.

> Source: `crates/vibelang-rhai/src/api/route.rs::RouteHandle`.

### 3a. `.to_current_group()` failure

`to_current_group()` errors when the voice has no explicit group set —
i.e. it's still in the implicit `main` group. The error names the port
and points at the two fixes:

```
to_current_group() on port 'even': voice has no explicit group set —
call `.group("name")` on the voice first, or write
`.to(group("name"))` to target a group explicitly
```

The implicit-`main` distinction matters: `to_current_group()` is
specifically a "use whatever I already configured" shortcut, not a
synonym for `to_main()`. If main is what you want, say `to_main()`
explicitly.

### 3b. Routing a port through FX — sub-group pattern

To send a port through an FX chain, define a group that owns the
effects, then route the port to that group. The sub-group's mix bus
auto-mixes up into its parent group, so the FX'd signal sums back into
the main path without any extra wiring.

```rhai
// Wet group: owns the FX chain. Every voice routed here gets the
// reverb + delay applied, then the result mixes up into `leads`.
let evens_fx = group("evens_fx")
    .effect("reverb_jpverb")
    .effect("delay_short");
let leads = group("leads");

let v = voice("spec")
    .synth("spectraphon_side")
    .group(leads);

v.output("even").to(evens_fx);    // wet: through reverb + delay → leads
v.output("odd").to(leads);        // dry: straight into leads
```

Why this is the canonical idiom for per-port FX:

* **Shared FX state across voices.** All voices that route into
  `evens_fx` share the same reverb/delay instance, so the verb tail
  rings out continuously instead of restarting per voice. A per-port
  FX modifier would have spawned a fresh FX synth per `(voice, port)`,
  which is rarely what you want.
* **Fewer buses.** The sub-group needs one mix bus, not one per FX
  per port. A 3-FX chain on 4 voices via the sub-group pattern uses
  one bus chain; per-port FX would have used 12.
* **Smaller API surface.** No new verb, no FX chain validation, no
  per-port FX state to track across reloads — just the existing
  group/effect machinery.

If you genuinely need per-voice independent FX state (e.g. a voice
with its own reverb tail that must not bleed into a sibling), put
that voice in its own group with the FX attached.

> Source: `crates/vibelang-core/src/handlers/groups.rs` (group bus
> auto-mix into parent), `crates/vibelang-rhai/src/api/group.rs`.

### 3c. CV-to-param routing — SET (`.to_param`) and BEND (`.param().modulate_by`)

CV-to-param routing has two equivalent surfaces, distinguished by
direction and intent:

* **`source.output("port").to_param(target, "param")` — source-first
  SET.** Read as "this port drives that param." Single source per
  param; later `.to_param` calls into the same `(target, param)`
  replace earlier ones (`/n_map` binds one bus). Use this when the
  source is the natural subject — e.g. a Maths channel "sets" a bass
  cutoff.
* **`target.param("param").modulate_by(source, "port")` — target-first
  BEND.** Read as "this param is bent by these sources." Multi-source
  fan-in is supported here: chain `.modulate_by` calls and the runtime
  spawns a `param_kr_sum_<N>` summer that mixes each source bus before
  `/n_map`-ing the target. Use this when the target is the natural
  subject — e.g. a voice's freq is bent by env + LFO together.

Both surfaces feed the same `ScriptState::add_param_route` registry —
direction is purely an authoring choice. The two registries (SET vs.
BEND) are separate however, so calling both `.to_param` and
`.modulate_by` on the same `(target, param)` is rejected with a clean
cross-verb conflict error pointing at the verb you should pick.

Common rules:

* **Kr only.** Source ports must have been declared with
  `.output_kr(...)`. Calling either verb on an ar-rate port is rejected
  with an error citing the rate, port name, and the `output_kr`
  remediation. Auto-coercion via `A2K.kr` is deferred to v3.
* **Param must exist on the target synthdef.** Unknown params produce
  a clean error citing the target voice, its synthdef, and the full
  set of available params.
* **Group/Main/Muted destinations stay single** — re-routing replaces
  the prior dest. Both kr-routing surfaces are additive across
  distinct `(target, param)` pairs.

#### SET — source-first

```rhai
let env = voice("env")
    .synth("maths")               // declares kr ports ch1..ch4
    .group("ctrl")                // logical container; nothing audible
    .set_param("rise1", 0.05)
    .set_param("fall1", 0.45)
    .set_param("cycle1", 1.0);

let bass = voice("bass")
    .synth("tb303_bass")
    .group("bass")
    .set_param("cutoff", 800.0);

env.output("ch1").to_param(bass, "cutoff");   // ch1 kr → bass.cutoff
```

See `examples/maths_to_param.vibe` for the full v2 smoke test
(committed at `c56241c`).

* **Fan-out across multiple targets is allowed.** One source kr port
  can drive params on many voices: just call `.to_param` again with a
  different `(target, param)` pair. Duplicates `(same target, same
  param)` are deduplicated.
* **Multi-source SET on the same `(target, param)` is rejected** —
  `/n_map` binds one bus, so two distinct source ports `to_param`-ing
  the same param is a conflict. The error suggests using
  `.modulate_by` instead.

#### BEND — target-first (multi-source fan-in)

```rhai
let env = voice("env").synth("lfo_sine").set_param("rate", 0.3);
let lfo = voice("vib").synth("lfo_sine").set_param("rate", 5.0);

let bass = voice("bass")
    .synth("tb303_bass")
    .group("bass");

bass.param("freq")
    .modulate_by(env, "out")
    .modulate_by(lfo, "out");      // env + lfo → bass.freq (summer spawned)
```

When ≥2 kr sources point at the same `(target_voice, target_param)`
via `.modulate_by`, the runtime allocates an intermediate control bus
and spawns a `param_kr_sum_<N>` summer (N in 2..=8) that sums each
source bus, then `/n_map`s the target to the intermediate bus.
Teardown respawns a smaller summer when N shrinks, collapses to direct
`/n_map` at N=1, and unmaps at N=0. Per-target summer state lives in
`State::param_summers: HashMap<(VoiceId, String), (NodeId, BusId)>`.

> Source: `crates/vibelang-rhai/src/api/route.rs::RouteHandle::to_param`
> and `ParamHandle::modulate_by`,
> `crates/vibelang-core/src/handlers/routes.rs::finalize_params`,
> `RouteDest::Param { voice_id, param_name }`.

---

## 4. Plural sugar — `voice.outputs([…])`

When several ports share the same destination, use `outputs([...])` and
fan a single terminal verb across the list:

```rhai
v.outputs(["odd", "even"]).to(group("leads"));   // both → leads
v.outputs(["sine", "sub"]).to_main();            // both → main
v.outputs([0, 2, 3]).mute();                     // mute by index
v.outputs(["odd", 1, "even"]).to_current_group();// mixed names + indices
```

The list accepts string names, integer indices (see §5), or a mix
(Rhai arrays are heterogeneous). Each entry resolves through the same
validator as the singular `output(...)` call, so a typo or out-of-range
index fails fast with the same error helpers.

Empty lists error: `"outputs() requires at least one port name or index"`.

`MultiRouteHandle` has the same four terminal verbs as `RouteHandle`
(`to`, `to_main`, `mute`, `to_current_group`). Plural fan-out via
`.to_param` was removed when SET/BEND split — `/n_map` binds one bus,
so multi-source SET into one param is a conflict. For multi-source
fan-in to one param, use the target-first BEND surface
(`target.param("p").modulate_by(s1, "out").modulate_by(s2, "out")`)
documented in §3c. For `to_current_group()` on a multi, the
**first failing port short-circuits** the iteration so you don't end
up with a partially-applied fan-out silently committed before the
error.

> Source: `crates/vibelang-rhai/src/api/voice.rs::Voice::outputs` /
> `crates/vibelang-rhai/src/api/route.rs::MultiRouteHandle`.

---

## 5. Index sugar — `voice.output(N)`

`output(name)` and `output(idx)` are both registered under the same
function name in Rhai, dispatched by argument type. `idx` is **0-based**
into the synthdef's declared port order:

```rhai
// Given .output("sine").output("sub").output("odd").output("even") in declaration order:
v.output(0)  // == v.output("sine")
v.output(1)  // == v.output("sub")
v.output(2)  // == v.output("odd")        // 3rd declared port
v.output(3)  // == v.output("even")
```

Negative indices and indices `>= ports.len()` error with the available
port count and names.

When to use which:

* **Names** — almost always. Self-documenting at the call site, and they
  survive synthdef edits that reorder ports.
* **Indices** — for short demos, generated routing tables, or one-off
  cases where the destination doesn't care which port is which (e.g.
  `outputs([0, 1]).to_main()` to grab the first two of any synthdef).

Indices break on synthdef edits that reorder ports. Names don't.

> Source: `crates/vibelang-rhai/src/api/voice.rs::Voice::output_by_idx`.

---

## 6. Reload semantics

The reload reconciler matches ports **by name** across synthdef edits.
Three categories of change to plan for:

| Change | Effect on existing routes |
|---|---|
| Body changed, port set unchanged | All routes survive. Buses are kept; only the synth nodes re-instantiate. |
| Port added | New bus allocated (audio or control depending on the port's rate). Default route for the new port is `mute()` unless the script already declared a route for it (the script's intent always wins). |
| Port removed | Bus freed (returned to the matching audio or control free-list). Routes targeting the removed port are dropped, with a `tracing::warn!` log naming each dropped route. |
| Port renamed | Surfaces as **remove + add** — the old name's bus is freed, routes referring to the old name are dropped (with the same warning), and the new name gets a fresh bus + default `mute()`. Renames break routes; the rationale is that guessing intent on rename is worse than a clear warning. |
| Port rate changed (ar↔kr) | Surfaces as a **rate-flip remove+add** — the old-rate bus is freed back to its allocator, a new bus is allocated from the *other* rate's free-list, and any routes that were rate-incompatible with the new rate are dropped with a warning (`.to(group)` requires ar; `.to_param(...)` requires kr). Mute routes survive a rate flip since `.mute()` is rate-agnostic. |

> Source: `crates/vibelang-core/src/reload/port_diff.rs::reconcile_voice_port_set`.

The "rename = remove+add" rule is worth internalising: if you're
refactoring a synthdef and want to keep your script routes working,
**don't rename ports** in the same edit. Either rename first and re-route
the script in the same change, or keep the old name as an alias
(declare both `.output("old")` and `.output("new")`) for a transition.

---

## 7. Worked example — `examples/spectraphon_multiout.vibe`

The committed multi-output proof-of-life is the canonical reference for
the v1 surface:

```rhai
import "stdlib/instruments/spectral/spectraphon_side.vibe";
import "stdlib/effects/reverbs/reverb_jpverb.vibe";

set_tempo(72);

define_group("leads", || { });

define_group("fx_evens", || {
    fx("evens_verb")
        .synth("reverb_jpverb")
        .param("time", 4.0)
        .param("size", 0.85)
        .param("mix", 0.55)
        .apply();
});

let v = voice("spec")
    .synth("spectraphon_side")
    .group("leads")
    .set_param("partials", 0.85)
    // ... params ...
    .gain(db(-9));

v.output("sine").to_main();
v.output("sub").to_main();
v.output("odd").to_current_group();          // = leads
v.output("even").to(group("fx_evens"));      // wet bus

melody("spec_arp")
    .on(v)
    .notes("A3 C4 E4")
    .start();
```

Topology:

```
sine ──────────────────────────────► main bus  (raw fundamental)
sub  ──────────────────────────────► main bus  (DC-coupled CV path)
odd  ──► leads (mix bus)            ─► main bus
even ──► fx_evens (reverb_jpverb)   ─► main bus
```

This pattern generalises: any synthdef that exposes multiple ports can
mix per-port FX, dry/wet splits, and bus-bypassing destinations from a
single voice instance.

### 7a. v2 worked example — `examples/maths_to_param.vibe`

The v2 CV-to-param smoke test wires the Maths channel-1 kr envelope
into a tb303 voice's filter cutoff via `/n_map` — no audio bus crosses
voice boundaries:

```rhai
import "stdlib/instruments/eurorack/maths.vibe";
import "stdlib/synths/tb303.vibe";

define_group("ctrl", || { });
define_group("bass", || { });

let m = voice("env")
    .synth("maths")
    .group("ctrl")
    .set_param("rise1", 0.05)
    .set_param("fall1", 0.45)
    .set_param("cycle1", 1.0);

let bass = voice("bass")
    .synth("tb303_bass")
    .group("bass")
    .set_param("cutoff", 800.0)
    .set_param("env_mod", 0.0);     // disable internal env→cutoff

m.output("ch1").to_param(bass, "cutoff");
```

Topology:

```
env (maths)  ch1 (kr)  ──/n_map──►  bass.cutoff
bass (tb303_bass)  ─group("bass")─► main
```

The `ctrl` group is a logical container — Maths' kr ports drive params
via `/n_map`, so nothing audible flows through it. See the example file
for the long-form explanation, including the kr-unit-to-Hz scaling
caveat (no per-route multiplier in v2; rescale on the synthdef side or
wait for v3).

---

## 8. Limitations (v2)

The v2 surface is shipped end-to-end. The legacy `modulator` builder
and `voice.modulate` surface have been removed — wire kr sources
into params directly via the SET / BEND verbs in §3c. The following
are documented non-features in v2; deferred to v3:

* **No ar→param coercion.** `.to_param` / `.modulate_by` require a
  kr-rate port — declare CV outputs with `.output_kr(...)`.
  Auto-coercion via an internal `A2K.kr` is doable but hides
  rate-mismatch bugs and is intentionally deferred. Calling either
  verb on an ar port produces a clean Rhai error pointing at
  `.output_kr`.
* **No fan-out to multiple groups.** Calling `.to(...)` twice on the
  same `(voice, port)` overwrites the prior destination rather than
  splitting the signal. Use a second voice on the same synthdef as a
  workaround. (`.to_param` *is* additive across different
  `(target, param)` pairs — see §3c.)
* **No `solo()`, `tap()`, `scope()`** — the wider verb table from the
  plan §5.2 is still deferred. Today the verb set is `to`, `to_main`,
  `to_current_group`, `mute`, `to_param` (source-first SET) and
  `param().modulate_by` (target-first BEND). Per-port FX is covered by
  the sub-group pattern (§3b); a per-port FX modifier was reverted
  because sub-groups gave better economics (shared FX state, fewer
  buses).
* **No sample-accurate trigger ports** — `Out.tr` semantics are not
  exposed yet.

---

## 9. References

* `kb/voice-multi-output-cv-routing-plan.md` — v1 architecture proposal
  (Options A–E, phased rollout, hot-reload safety analysis).
* `kb/voice-multi-output-v2-plan.md` — v2 design doc (kr ports,
  `to_param`, per-port FX, modulator-as-sugar).
* `examples/spectraphon_multiout.vibe` — v1 multi-output proof-of-life
  (audio routing, dry/wet split via groups).
* `examples/maths_to_param.vibe` — v2 CV-to-param smoke test
  (Maths kr envelope → tb303 cutoff via `/n_map`).
* `crates/vibelang-std/stdlib/instruments/spectral/spectraphon_side.vibe`
  — multi-port spectral synthdef (ar).
* `crates/vibelang-std/stdlib/instruments/eurorack/maths.vibe`
  — multi-port CV synthdef with kr ports `ch1..ch4`.
* `crates/vibelang-rhai/src/api/route.rs` — `RouteHandle` /
  `MultiRouteHandle` Rhai surface (`.to`, `.to_main`,
  `.to_current_group`, `.mute`, `.to_param`); `ParamHandle.modulate_by`.
* `crates/vibelang-rhai/src/api/voice.rs::Voice` — `output_by_name`,
  `output_by_idx`, `outputs`, `param`.
* `crates/vibelang-core/src/handlers/routes.rs` —
  `RouteDest::{Group, Main, Muted, Param}`, default routing rule,
  finalize (audio + param paths).
* `crates/vibelang-core/src/reload/port_diff.rs` /
  `crates/vibelang-core/src/reload/script_state.rs` — reload
  reconciler (incl. port-rate change diff) and `param_routes`
  storage.
* `kb/spectraphon-howto.md` — Spectraphon user manual; the multi-output
  example links back into routing here.
