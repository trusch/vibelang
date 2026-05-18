# Voice Named-Port Routing — How-To

> User manual for declaring named output ports on a synthdef, routing
> them per-port from `.vibe` scripts, and using the companion named audio
> input surface. Pairs with the deeper proposals in
> `kb/voice-multi-output-cv-routing-plan.md` (v1) and
> `kb/voice-multi-output-v2-plan.md` (v2).
>
> **Status snapshot:** v1 (audio-only) + v2 (kr-rate ports, CV-to-param
> routing) + v3 (per-source attenuverter shaping, ar→param coercion,
> sample-accurate trigger ports, multi-target audio fan-out) all
> shipped. The canonical kr-routing surfaces are `.to_param`
> (source-first SET) and `.param(...).modulate_by(...)` (target-first
> BEND), see §3c, both with chainable `.scale(s)` / `.offset(o)`
> per-source shaping (§3d). Audio-rate sources reach kr params via
> `.to_param_audio(...)` (§3e). Sample-accurate triggers use the
> dedicated `Out.tr` path: `.output_tr(...)` on the synthdef, then
> `.to_trigger(target, "param")` on the route (§3f). `.to(group)` is
> additive — chaining `.to(g_a).to(g_b)` installs both, no more voice
> duplication for splitter / mult patterns. The per-port FX feature
> was implemented and then reverted: sub-group routing covers every
> use case with shared FX state and fewer buses (see §3b).

A vibelang voice can expose **N named output ports**. Each port is an
independent signal that the script routes with the verbs for that port's
rate: audio-rate ports can go to group/main mix buses, kr/tr ports can
drive params/triggers, and any rate can be muted. One synthdef can fan out
to several destinations without splitting it into sibling voices.

The companion named-input surface lets synthdefs expose patchable audio
input jacks. Declare inputs with `.input(...)`, read them in `body_map` as
`p.inputs.<name>`, and patch them from scripts with
`target.input("name").from(source)`.

Worked example: `examples/spectraphon_multiout.vibe` (committed at
`2f03112`) wires the four ports of `spectraphon` (`sine`, `sub`,
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

## 2. Default routing rule (audio-rate count)

When the script declares **zero** routes for a voice, vibelang installs a
count-based default so audio synths still make sound. The count is computed
over **audio-rate (`ar`) ports only**; kr/tr ports never receive implicit
audio group routes and must be routed explicitly or muted.

| Audio-rate port count | Default behaviour |
|---|---|
| 0 | No default routes. Zero-output side-effect synthdefs, kr-only synthdefs, and tr-only synthdefs produce no default routes at all. |
| 1 | The single ar port routes to the voice's group bus. |
| 2 | Both ar ports route to the voice's group bus (treated as L/R when summed). |
| N > 2 | The **first two ar ports** route to the group bus; **all remaining ar ports are silent** until the script routes them. |

Why "first two then silent": the common pattern is that the first ports
in declaration order are the canonical mix output (e.g. `sine` first), and
any extras are specialised sends that should not leak into the main mix
without explicit user intent. For mixed-rate synthdefs, "first two" means
the first two **ar** ports in declaration order, skipping any intervening
kr/tr ports.

> Source: `crates/vibelang-core/src/handlers/routes.rs::default_routes_for_voice`;
> covered by `default_routes_kr_only_port_returns_empty`,
> `default_routes_mixed_rate_defaults_only_audio_ports`,
> `unrouted_kr_output_does_not_default_route_or_block_valid_audio_route`, and
> `create_voice_zero_output_synthdef_has_no_default_routes_or_mixers`.

Default routes can be **overridden** by an explicit `voice.output(...)`
call — explicit routes always win over the default in the merge step
(`merge_default_routes`).

---

## 3. Overriding routes — terminal verbs

`voice.output(name)` returns a chainable `RouteHandle`. Pick exactly one
terminal verb to commit the route:

| Verb | Effect | Port rate |
|---|---|---|
| `.to(group)` | Sum this port into the named group's mix bus. **Additive** — chaining `.to(g_a).to(g_b)` installs both edges (one mixer synth per (port, group)). | ar only |
| `.to_main()` | Send straight to main hardware output (bus 0), bypassing groups. Replace semantics. | ar only |
| `.to_current_group()` | Sum into whichever group the voice was last `.group("name")`-ed into. Errors when the voice has no explicit group set. Additive (same as `.to`). | ar only |
| `.mute()` | Discard the port's signal. Replace semantics. | ar, kr, or tr |
| `.to_param(target, "param")` | **Source-first (SET).** Map this port's control bus into `target.param` via scsynth `/n_map` (see §3c, §3d for shaping). | kr only |
| `.to_param_audio(target, "param")` | **ar→kr coerced SET.** Same SET semantics as `.to_param`, but the source is ar — the runtime spawns a shared `a2k_adapter_1` synth that downsamples to kr, then feeds the standard summer (see §3e). | ar only |
| `.to_trigger(target, "param")` | **Sample-accurate trigger routing.** Forwards a Tr-rate port through `port_tr_to_param_link_1` so an `Out.tr` edge lands on the target param's input at sample-accurate timing — distinct from kr-rate `.to_param` (no scale/offset shaping, no fan-in). See §3f. | tr only |

The dual surface lives on the **target** voice via
`voice.param("name")`, which returns a `ParamHandle`:

| Verb | Effect | Port rate |
|---|---|---|
| `.modulate_by(source, "port")` | **Target-first (BEND).** Same registry entry as `.to_param`, written from the receiving side. Multi-source fan-in to one param is supported here (see §3c). | kr only |

Both `.to_param(...)` and `.modulate_by(...)` accept chainable
`.scale(s)` / `.offset(o)` modifiers (see §3d) for per-source
attenuverter shaping. `.to_param_audio(...)` does too — the coerced
ar→kr bus runs through the same summer. `.to_trigger(...)` does **not**
expose `.scale` / `.offset` (triggers are sample-accurate edges, not
analog levels).

To send a port through an FX chain, route it into a sub-group that owns
the effects. The sub-group's mix bus auto-mixes back into its parent
group, so a per-port FX wet-send is just two routes (one for the dry
port, one for the FX-bound port → sub-group). See §3b.

```rhai
let v = voice("spec")
    .synth("spectraphon")
    .group("leads");

v.output("sine").to_main();                 // dry fundamental → main
v.output("sub").to_main();                  // CV-style sub → main
v.output("odd").to_current_group();         // == .to(group("leads"))
v.output("even").to(group("fx_evens"));     // distinct FX bus
```

Re-route semantics by terminal verb:

* **`.to(group)`** is **additive** (post-v3 B3) — `.to(g_a).to(g_b)`
  on the same `(voice, port)` installs both edges. Repeated `.to(g_a)`
  is deduplicated. The runtime spawns one mixer synth per distinct
  (port, group) pair on finalize.
* **`.to_main()`** and **`.mute()`** keep replace semantics — Main is
  the hardware bus, Muted is silence; neither benefits from fan-out.
* **Switching variants** (e.g. `.to_main()` after `.to(g_a)`) clears
  the prior dest — the variants are mutually exclusive.

The pre-v3 voice-duplication workaround for splitter / mult patterns
is no longer needed.

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
    .synth("spectraphon")
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

SET and BEND feed sibling registries: `param_routes_set` (source-first
authoring) and `param_routes_bend` (target-first authoring). They share
the same `param_kr_modulate_<n>` summer infrastructure on finalize, so
direction is purely an authoring choice — but the registries are
separate, so calling both `.to_param` and `.modulate_by` on the same
`(target, param)` is rejected with a clean cross-verb conflict error
pointing at the verb you should pick. Trigger routes (`.to_trigger`,
§3f) live in a third registry (`param_routes_trigger`); the three are
mutually exclusive on a single `(target, param)`.

Common rules:

* **Kr only on the source** for `.to_param` / `.modulate_by`. Source
  ports must have been declared with `.output_kr(...)`. Calling either
  verb on an ar-rate port is rejected with a clean rate-mismatch error
  pointing at `.to_param_audio(...)` (see §3e) for ar sources, or
  `.output_kr(...)` if you want to declare the source as kr instead.
* **Param must exist on the target synthdef.** Unknown params produce
  a clean error citing the target voice, its synthdef, and the full
  set of available params.
* **Group/Main/Muted destinations stay single per replace-variant** —
  switching from `.to_main()` to `.to_param(...)` etc. clears the
  prior dest. Both kr-routing surfaces are additive across distinct
  `(target, param)` pairs (and `.to(group)` is additive across
  distinct groups, post-B3).

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

**All param routes go through a `param_kr_modulate_<N>` summer.**
Post-v3 A1.a, even a single SET source no longer `/n_map`s directly to
the source bus — `finalize_params` allocates an intermediate kr bus,
spawns a `param_kr_modulate_1` summer (with `baseline=0`, `scale_1=1`,
`offset_1=0` by default — the identity), and `/n_map`s the target to
the intermediate. This unification is what lets §3d's `.scale(s)` /
`.offset(o)` modifiers work uniformly across SET / BEND / SET-via-
`.to_param_audio` without special-casing direct vs. summer-routed
routes. For multi-source BEND, N grows to 2..=8; teardown respawns a
smaller summer when N shrinks and unmaps at N=0. Per-target summer
state lives in `State::param_summers: HashMap<(VoiceId, String),
ParamSummerState>`.

> Source: `crates/vibelang-rhai/src/api/route.rs::RouteHandle::to_param`
> and `ParamHandle::modulate_by`,
> `crates/vibelang-core/src/handlers/routes.rs::finalize_params`,
> `RouteDest::Param { voice_id, param_name }`,
> `crates/vibelang-dsp/src/system_synthdefs/routing.rs` for the
> `param_kr_modulate_<N>` summer + `a2k_adapter_1` +
> `port_tr_to_param_link_1` synthdefs.

### 3d. Per-source attenuverter — `.scale(s)` / `.offset(o)`

Both `.to_param(...)` and `.modulate_by(...)` (and
`.to_param_audio(...)`, §3e) accept chainable `.scale(s)` /
`.offset(o)` modifiers that shape the source signal entering the
target's summer. Defaults are `scale=1.0`, `offset=0.0` — the
identity, matching v2 behaviour.

```rhai
// SET form: source-first scale/offset.
env.output("ch1").to_param(bass, "cutoff").scale(3000.0).offset(500.0);

// BEND form: target-first.
bass.param("freq")
    .modulate_by(env, "ch4")
    .scale(60.0)
    .offset(0.0);
```

What lands on the target is per-source affine shaping inside the
summer:

```
target_param = baseline + Σ_i (scale_i * In.kr(in_i, 1) + offset_i)
```

where `baseline=0` and `i` ranges over each `(source_voice,
source_port)` in the summer's input slots. Each call to `.scale(s)` /
`.offset(o)` updates the slot for the **most-recently-installed**
`.to_param(...)` / `.modulate_by(...)` on the handle (tracked via
`last_param_target` / `last_modulate_source`); chaining
`.to_param(A, "p1").scale(0.5).to_param(B, "p2").scale(2.0)` attaches
0.5 to A and 2.0 to B without bleed-through. Multi-call: last
`.scale` / `.offset` wins independently — they do not reset each
other.

Calling `.scale(...)` / `.offset(...)` before any `.to_param(...)` /
`.modulate_by(...)` install errors with a clean message pointing at
the missing install verb.

> Source: `RouteHandle::scale` / `RouteHandle::offset` and the
> `ParamHandle` duals at `crates/vibelang-rhai/src/api/route.rs`;
> `ScriptState::param_route_set_shaping` /
> `param_route_bend_shaping` side-tables in
> `crates/vibelang-core/src/reload/script_state.rs`.

### 3e. Audio-rate sources via `.to_param_audio()`

`.to_param_audio(target, "param")` mirrors `.to_param` but flips the
rate constraint: the source port must be `output(...)` (ar). The
runtime spawns one shared `a2k_adapter_1` synth per `(source_voice,
source_port)` pair (`Out.kr(out_bus, A2K.kr(In.ar(in_bus, 1)))`),
routes the resulting kr bus into the same `param_kr_modulate_<n>`
summer infrastructure as a kr-native source, and `/n_map`s the target.

```rhai
// `audio_lfo`'s `wobble` port is ar-rate. .to_param_audio coerces it
// to kr via a shared a2k_adapter_1 synth, then attenuverter shaping
// (§3d) still applies because the coerced bus enters the same summer.
pad_lfo.output("wobble")
       .to_param_audio(bass, "amp")
       .scale(0.4)
       .offset(0.3);
```

* **Adapter sharing.** One adapter per `(source_voice, source_port)`,
  reused across every SET/BEND route from that pair. Orphaned
  adapters are freed on the next `finalize_params` when no param
  route still references the source.
* **Cross-verb conflicts** behave the same as `.to_param` —
  `.to_param_audio` lands in the SET registry, so a later
  `.modulate_by(...)` on the same `(target, param)` is rejected.
* **Rate guards stay strict in both directions.** `.to_param_audio()`
  on a kr or tr source errors with a hint pointing at `.to_param()`;
  `.to_param()` on an ar source still errors with a hint pointing at
  `.to_param_audio()`. Auto-coercion is opt-in, not silent.

> Source: `RouteHandle::to_param_audio` at
> `crates/vibelang-rhai/src/api/route.rs`; adapter spawn / share /
> teardown at `crates/vibelang-core/src/handlers/routes.rs`;
> `a2k_adapter_1` synthdef at
> `crates/vibelang-dsp/src/system_synthdefs/routing.rs`.

### 3f. Sample-accurate triggers — `.output_tr(...)` + `.to_trigger(...)`

For sample-accurate trigger edges (`Out.tr` semantics), declare the
port as **trigger-rate** with `.output_tr(name)` on the synthdef
builder. Trigger ports share scsynth's control-bus pool with kr ports
but emit `Out.tr` so single-sample edges survive the bus boundary.

```rhai
define_synthdef("step_trig", |builder| {
    builder
        .output_tr("trig_out")          // tr-rate port
        .param("rate", 4.0)
        .body(|rate| { impulse_kr(rate, 0.0) })
});
```

On the route side, `.to_trigger(target, "param")` validates the source
is Tr-rate and wires it through `port_tr_to_param_link_1` — a 1:1
`Out.kr ∘ In.kr` link synthdef — onto an intermediate kr bus that
`/n_map`s the target's param input. Edges are preserved end-to-end at
sample-accurate timing; this is genuinely sample-accurate (via
`Out.tr` on the source and the link's tight forwarding loop), **not**
the kr summer treated as a trigger path.

```rhai
seq.output("trig_out").to_trigger(kick, "trig");
```

Constraints:

* **Tr-rate sources only.** ar / kr ports on `.to_trigger()` error
  with rate-mismatch + a hint pointing at `.to_param()` /
  `.to_param_audio()` / `.to(group)` / `.to_main()`.
* **No `.scale(...)` / `.offset(...)` slot.** Triggers are
  sample-accurate edges, not analog levels — there is no attenuverter
  shaping. Use a separate kr envelope on the target side if you want
  to shape the trigger's downstream effect.
* **No multi-source fan-in.** Trigger routing is 1:1 per
  `(target_voice, target_param)` — `.to_trigger` from a second
  source onto the same target/param errors with a single-source
  conflict.
* **Three-way cross-verb exclusion.** TRIGGER, SET, and BEND can't
  coexist on the same `(target, param)`; the runtime rejects collisions
  in all three pairs (TRIGGER↔SET, TRIGGER↔BEND, SET↔BEND).
* **Reload reconcile.** Source-port rate flips that turn a kr/ar port
  into a Tr port (or vice versa) drop the previously registered route
  on that source-side key with a `tracing::warn!` and free the bus
  back to its allocator pool, mirroring the kr↔ar flip behaviour.

> Source: `PortRate::Tr` + `.output_tr(...)` at
> `crates/vibelang-dsp/src/builder.rs`; `RouteHandle::to_trigger`
> at `crates/vibelang-rhai/src/api/route.rs`;
> `ScriptState::param_routes_trigger` +
> `add_param_route_trigger` at
> `crates/vibelang-core/src/reload/script_state.rs`;
> `port_tr_to_param_link_1` synthdef at
> `crates/vibelang-dsp/src/system_synthdefs/routing.rs`;
> `param_triggers: HashMap<(VoiceId, String), (NodeId, BusId)>`
> teardown tracker on `State`.

### 3g. Group → hardware output

A group's mix bus normally folds back into its parent (or, for a
top-level group, into the main hardware pair on bus 0). Pin a group
to a specific hardware bus with `group("name").output(...)`:

| Form | Hardware bus | Link synth | Use |
|---|---|---|---|
| `group("g").output(N)` | single bus `N` (mono) | `system_link_audio_mono` | One DC-coupled CV channel, one mono stem to a separate jack. |
| `group("g").output([N])` | single bus `N` (mono) | `system_link_audio_mono` | Sugar identical to `output(N)`. |
| `group("g").output([L, R])` | consecutive pair `[L, L+1]` | `system_link_audio` | Stereo group output. `R` must be `L+1`; non-consecutive errors out at script-load. |
| `group("g").output(...)` not called | parent / main | `system_link_audio` | Legacy default. |

**Sum, no halve.** The mono link synthdef computes `L + R` from the
group's stereo mix bus and writes that straight to the declared
hardware bus. Two consequences:

* A mono synthdef in a mono group matches a mono synthdef in a
  stereo group in level — both are unity gain at the hardware bus.
  `Pan2.ar(sig)` puts equal energy on L and R; the mono fold sums
  them back. (Halving would bias toward stereo synthdefs by 6 dB.)
* True stereo content (a wide reverb tail, a `[L, R]` body) routed
  into a mono group sums to ~6 dB hot at the bus. That's by design,
  not a bug — pull the group's gain down if you want a balanced
  fold.

#### Single-channel (mono) hardware output — 8-CV-on-ES-3 pattern

The Expert Sleepers ES-3 (and any DC-coupled audio interface)
exposes 8 mono jacks on JACK output channels 1..8 (vibelang bus
indices `0..7`). Eight independent CVs, one per jack, collapse to a
flat list of mono groups using `output(N)`:

```rhai
import "stdlib/cv/lfo/cv_lfo_sine.vibe";
import "stdlib/cv/lfo/cv_lfo_tri.vibe";
import "stdlib/cv/lfo/cv_lfo_square.vibe";
import "stdlib/cv/lfo/cv_lfo_random.vibe";

// One mono group per ES-3 jack (1-indexed jack N → bus index N-1).
group("cv1").output(0);
group("cv2").output(1);
group("cv3").output(2);
group("cv4").output(3);
group("cv5").output(4);
group("cv6").output(5);
group("cv7").output(6);
group("cv8").output(7);

// One mono CV synthdef per group — each writes one channel of CV
// to its own jack. No `[0, signal]` wrapper, no overlapping bus
// pairs, no wasted `R` half.
voice("slow_sine").synth("cv_lfo_sine").group("cv1")
    .set_param("rate", 0.1).set_param("depth_v", 5.0).start();

voice("ramp").synth("cv_lfo_tri").group("cv2")
    .set_param("rate", 0.25).set_param("depth_v", 8.0).start();

voice("gate").synth("cv_lfo_square").group("cv3")
    .set_param("rate", 2.0).set_param("depth_v", 5.0).start();

voice("noise").synth("cv_lfo_random").group("cv4")
    .set_param("rate", 4.0).set_param("depth_v", 3.0).start();

// ... cv5..cv8 the same way, one synthdef per jack.
```

Why this is the canonical CV-on-DC-interface pattern:

* **No bus waste.** The pre-mono-group form (`group("cvN").output([2*k, 2*k+1])`)
  reserved a stereo pair per CV and silenced the `R` half — half
  the routing budget went unused. `output(N)` claims exactly one
  bus per jack.
* **No overlapping pairs.** Eight CVs no longer collide pairwise
  (`[0,1]`, `[2,3]`, ...) — each group owns one bus index, so the
  layout reads jack-by-jack instead of pair-by-pair.
* **No `[0, signal]` wrapper in the synthdef.** A mono CV synthdef
  body returns a single scalar (`[clipped]`), not `[signal, 0]`
  — `system_link_audio_mono` does the fold at the group → hardware
  boundary, so the synthdef stays one-channel end-to-end.

Run with `--output-channels 8` so scsynth allocates the JACK ports
(see `crates/vibelang-cli/src/main.rs`); the channel-index range
guard in `group.output(N)` rejects `N >= 16` at script-load time.

> Source: `crates/vibelang-rhai/src/api/group.rs::output_mono` /
> `output(channels: Array)` (mono int form, mono `[N]` sugar,
> stereo `[L, R]`); dispatch at
> `crates/vibelang-core/src/handlers/groups.rs::finalize`;
> `system_link_audio_mono` synthdef at
> `crates/vibelang-dsp/src/system_synthdefs/`.

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

## 5a. Named audio inputs companion

Named inputs are target-side audio jacks on a synthdef. They are useful for
processors, filters, mixers, ring modulators, sidechains, and other modules
whose signal source should be patchable from the script.

Declare them on the `define_synthdef` builder before `body_map`:

```rhai
define_synthdef("lowpass_mono")
    .input("in")             // mono ar input
    .output("out")           // mono ar output
    .param("cutoff", 1200.0)
    .param("resonance", 0.3)
    .body_map(|p| {
        rlpf_ar(p.inputs.in, p.cutoff, p.resonance)
    });

define_synthdef("passthrough_stereo")
    .input("in", 2)          // stereo ar input
    .output("out", 2)        // stereo ar output
    .param("level", 1.0)
    .body_map(|p| {
        [
            p.inputs.in[0] * p.level,
            p.inputs.in[1] * p.level,
        ]
    });
```

Rules:

* **Audio-rate only.** The public Rhai input builder has no kr/tr form.
* **Mono/stereo only.** `.input(name)` defaults to one channel;
  `.input(name, 2)` declares stereo. Wider inputs are not public yet.
* **`body_map` required.** Declared inputs are exposed through
  `p.inputs.<name>`, so positional `.body(...)` is not valid for
  synthdefs with inputs.
* **Strict width matching.** `target.input("name").from(source)` links the
  source voice's default output port, `"out"`, into the target input. Source
  width must equal target input width. Group sources are stereo; they do not
  downmix into mono inputs.
* **Unpatched jacks are silent.** Use
  `target.input("name").disconnect()` to explicitly return a jack to silence.
* **`in` autofeed.** A stereo audio input named exactly `in` autofeeds from
  the parent group's pre-fader bus when no explicit route overrides it. Mono
  `in` against the stereo parent group stays silent and logs a warning. This
  is implemented in sibling Wave-1 ticket
  `task-implement-parent-group-in-autofeed-for-declared-in-input`; the code
  path is `runtime.rs::effective_input_routes`.

Script-side patching is target-first:

```rhai
let src = voice("src").synth("mono_source"); // declares mono output "out"
let lp = voice("lp").synth("lowpass_mono");

lp.input("in").from(src);        // reads src output port "out"
lp.input("in").disconnect();     // back to silence
```

API references:

* `kb/tickets/api-reference/custom-synthdef-api/README.md` — declaring
  `.input(name)` / `.input(name, channels)` and reading `p.inputs.<name>`.
* `kb/tickets/api-reference/voice-api/README.md` — routing with
  `target.input("name").from(source)` and `.disconnect()`.

---

## 6. Reload semantics

The reload reconciler matches named output ports and named input ports **by
name** across synthdef edits. Input-port route reconciliation is implemented
in sibling Wave-1 ticket `task-reconcile-synthdef-input-port-hot-reload-routes`;
the user-visible rule mirrors outputs: rename is remove+add, and dependent
routes are dropped with warnings instead of guessed.

Output-port changes to plan for:

| Change | Effect on existing routes |
|---|---|
| Body changed, port set unchanged | All routes survive. Buses are kept; only the synth nodes re-instantiate. |
| Port added | New bus allocated (audio or control depending on the port's rate). Default route for the new port is `mute()` unless the script already declared a route for it (the script's intent always wins). |
| Port removed | Bus freed (returned to the matching audio or control free-list). Routes targeting the removed port are dropped, with a `tracing::warn!` log naming each dropped route. |
| Port renamed | Surfaces as **remove + add** — the old name's bus is freed, routes referring to the old name are dropped (with the same warning), and the new name gets a fresh bus + default `mute()`. Renames break routes; the rationale is that guessing intent on rename is worse than a clear warning. |
| Port rate changed (ar ↔ kr ↔ tr) | Surfaces as a **rate-flip remove+add** — the old-rate bus is freed back to its allocator, a new bus is allocated from the appropriate pool (audio for ar; control for kr and tr — they share the control-bus pool), and any routes that were rate-incompatible with the new rate are dropped with a warning (`.to(group)` / `.to_main()` / `.to_current_group()` require ar; `.to_param(...)` / `.modulate_by(...)` require kr; `.to_param_audio(...)` requires ar; `.to_trigger(...)` requires tr). Mute routes survive a rate flip since `.mute()` is rate-agnostic. |

> Source: `crates/vibelang-core/src/reload/port_diff.rs::reconcile_voice_port_set`.

Input-port changes to plan for:

| Change | Effect on existing input routes |
|---|---|
| Body changed, input set unchanged | Routes survive. The target synth node can be rebuilt while the declared input buses remain stable. |
| Input added | New input bus allocated. The input defaults to silence unless it is a stereo input named `in`, in which case the Wave-1 autofeed rule applies when no explicit route overrides it. |
| Input removed | Input bus freed. Routes targeting the removed input are dropped with a warning naming the target voice, synthdef, input, and reason. |
| Input renamed | Surfaces as **remove + add**. The old input's bus is freed, routes referring to the old name are dropped with warning, and the new name gets a fresh default route. Renames break routes by design. |
| Input width changed (mono ↔ stereo) | Surfaces as **remove + add**. Existing routes to that input are dropped with warning because width matching is strict and the old cable cannot be safely reinterpreted. |

The "rename = remove+add" rule is worth internalising: if you're
refactoring a synthdef and want to keep your script routes working,
**don't rename ports or inputs** in the same edit unless you also update the
script routes. For outputs, you can keep the old name as an alias
temporarily (declare both `.output("old")` and `.output("new")`). For inputs,
prefer a two-step migration: add the new input, update scripts to route it,
then remove the old input.

---

## 7. Worked example — `examples/spectraphon_multiout.vibe`

The committed multi-output proof-of-life is the canonical reference for
the v1 surface:

```rhai
import "stdlib/instruments/spectral/spectraphon.vibe";
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
    .synth("spectraphon")
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
caveat (the per-route multiplier the v2 example calls "deferred to v3"
is exactly what `.scale(s)` / `.offset(o)` now provides — see §3d and
the v3 worked example below).

### 7b. v3 worked example — `examples/v3_modular_demo.vibe`

The v3 integration demo exercises every feature shipped in this round
(`.scale` / `.offset`, `.to_param_audio`, `.output_tr` + `.to_trigger`,
multi-target `.to`) in a single self-contained patch. Headline shape:

```rhai
// 1. Attenuverter shaping on SET + BEND.
env.output("ch1").to_param(bass, "cutoff").scale(3000.0).offset(500.0);
bass.param("freq").modulate_by(env, "ch4").scale(60.0).offset(0.0);
bass.param("resonance").modulate_by(env, "eor1").scale(0.4);

// 2. ar→kr coercion via .to_param_audio (shared a2k_adapter_1).
pad_lfo.output("wobble").to_param_audio(bass, "amp").scale(0.4).offset(0.3);

// 3. Sample-accurate triggers via .output_tr + .to_trigger.
//    `step_trig` declares its port with `.output_tr("trig_out")`.
seq.output("trig_out").to_trigger(kick, "trig");

// 4. Multi-target audio fan-out — kick.out reaches BOTH groups.
kick.output("out").to(group("main_mix")).to(group("verb_send"));
```

Topology highlights:

```
env (maths)  ch1   ──.to_param + scale/offset──► bass.cutoff   (SET — summer slot 1)
             ch4   ──.modulate_by + scale──────► bass.freq     (BEND — distinct param)
             eor1  ──.modulate_by + scale──────► bass.resonance(BEND — distinct param)

pad_lfo      wobble (ar)  ──a2k_adapter_1──.to_param_audio──► bass.amp

seq          trig_out (Tr)  ──port_tr_to_param_link_1──► kick.trig

kick         out  ──.to(main_mix).to(verb_send)──► both groups (additive)
```

Read the file for the long-form commentary on each route choice;
particularly the why-this-target-not-that-one explanation for the
SET/BEND split and the cross-verb conflict guards.

---

## 8. Limitations

The v1 + v2 + v3 surfaces are all shipped end-to-end. Wire kr sources
into params directly via the SET / BEND verbs (§3c)
with optional `.scale` / `.offset` shaping (§3d). Crossed-out items
below were v2 limitations that v3 has now resolved; the remaining
non-crossed bullets are deferred non-features.

* ~~**No ar→param coercion.**~~ **Resolved in v3 (commit 6870961).**
  `.to_param_audio(target, "param")` coerces ar→kr through a shared
  `a2k_adapter_1` synth and feeds the standard summer (§3e). The
  pure-kr verbs (`.to_param`, `.modulate_by`) still require kr — the
  rate-mismatch error now points at `.to_param_audio()` so picking the
  right verb is one rename away.
* ~~**No fan-out to multiple groups.**~~ **Resolved in v3 (commit
  9cb9b73).** `.to(g_a).to(g_b)` is additive — finalize spawns one
  mixer synth per (port, group) edge. `.to_main()` and `.mute()`
  remain replace-only since neither benefits from fan-out.
* ~~**No per-route attenuverter (kr-unit-to-Hz scaling caveat).**~~
  **Resolved in v3 (commits 4860106 / fd88ba0).** Chainable `.scale(s)`
  / `.offset(o)` modifiers on `.to_param` / `.modulate_by` /
  `.to_param_audio` apply per-source affine shaping inside the
  `param_kr_modulate_<n>` summer (§3d).
* ~~**No sample-accurate trigger ports.**~~ **Resolved in v3 (commits
  b9f17d5 / d0f9243 / 61821bb).** `.output_tr(...)` declares a Tr-rate
  port (codegen flips to `Out.tr`); `.to_trigger(target, "param")`
  routes it through `port_tr_to_param_link_1` for sample-accurate
  edge forwarding (§3f). No scale/offset on triggers and no fan-in;
  trigger routing is 1:1.
* **No `solo()`, `tap()`, `scope()`** — the wider verb table from the
  plan §5.2 is still deferred. Today the verb set is `to`, `to_main`,
  `to_current_group`, `mute`, `to_param` (kr SET), `to_param_audio`
  (ar SET), `to_trigger` (Tr), and `param().modulate_by` (kr BEND).
  Per-port FX is covered by the sub-group pattern (§3b); a per-port FX
  modifier was reverted because sub-groups gave better economics
  (shared FX state, fewer buses).
* **No multi-source fan-in for triggers.** `.to_trigger` is 1:1;
  if you need to OR several edges together, sum them on the source-side
  synthdef and expose a single `.output_tr` port. Multi-source kr
  fan-in (`.modulate_by` chained from N sources) is unaffected.
* **No fan-out from `.to_main()` / `.mute()`.** `.to_main()` targets the
  one hardware bus, `.mute()` targets silence — neither variant benefits
  from a `Vec<Dest>`, so they keep replace semantics and `.to_main()`
  after `.to(g)` clears the prior dest.

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
* `examples/v3_modular_demo.vibe` — v3 integration demo exercising all
  four v3 features (`.scale` / `.offset`, `.to_param_audio`,
  `.output_tr` + `.to_trigger`, multi-target `.to(...)`) in a single
  patch.
* `crates/vibelang-std/stdlib/instruments/spectral/spectraphon.vibe`
  — multi-port spectral synthdef (ar).
* `crates/vibelang-std/stdlib/instruments/eurorack/maths.vibe`
  — multi-port CV synthdef with kr ports `ch1..ch4`.
* `crates/vibelang-rhai/src/api/route.rs` — `RouteHandle` /
  `MultiRouteHandle` Rhai surface (`.to`, `.to_main`,
  `.to_current_group`, `.mute`, `.to_param`, `.to_param_audio`,
  `.to_trigger`, `.scale`, `.offset`); `ParamHandle.modulate_by` +
  `.scale` / `.offset`.
* `crates/vibelang-rhai/src/api/voice.rs::Voice` — `output_by_name`,
  `output_by_idx`, `outputs`, `param`.
* `crates/vibelang-core/src/handlers/routes.rs` —
  `RouteDest::{Group, Main, Muted, Param}`, default routing rule,
  finalize (audio + param paths, summer/adapter/link spawn + teardown).
* `crates/vibelang-core/src/reload/port_diff.rs` /
  `crates/vibelang-core/src/reload/script_state.rs` — reload
  reconciler (incl. port-rate change diff for ar↔kr↔tr), the three
  param-route maps (`param_routes_set` / `param_routes_bend` /
  `param_routes_trigger`), and the per-source shaping side-tables
  (`param_route_set_shaping` / `param_route_bend_shaping`).
* `crates/vibelang-dsp/src/builder.rs` — `PortRate::{Ar, Kr, Tr}` +
  `.output(...)` / `.output_kr(...)` / `.output_tr(...)` builder.
* `crates/vibelang-dsp/src/system_synthdefs/routing.rs` — system
  synthdefs underpinning the v3 routing pipeline:
  `param_kr_modulate_<n>` (1..=8 source summer with per-slot scale +
  offset), `a2k_adapter_1` (ar→kr coercion), and
  `port_tr_to_param_link_1` (1:1 Tr forwarder).
* `kb/spectraphon-howto.md` — Spectraphon user manual; the multi-output
  example links back into routing here.
