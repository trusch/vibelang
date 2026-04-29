# Voice Multi-Output Routing — How-To

> User manual for declaring named output ports on a synthdef and routing
> them per-port from `.vibe` scripts. Pairs with the deeper proposal in
> `kb/voice-multi-output-cv-routing-plan.md`.
>
> **Status snapshot:** v1 (audio-only) shipped — multi-output Stories 1–12
> are merged. CV-to-param routing (v2) and modulator-API obsolescence (v3)
> are documented as future work in §8.

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
        .output("left")              // 1-channel port (default)
        .output("right")
        .output("verb_send", 2)      // 2-channel (stereo) port
        .param("freq", 220.0)
        .body_map(|p| {
            // ... DSP graph ...
            // Body returns one signal per declared port, in order:
            [left_sig, right_sig, verb_send_stereo_sig]
        })
});
```

Rules:

* **One signal per port**, in declared order. Mono ports take a
  scalar signal, stereo ports take a `[L, R]` array.
* **`.output(name)`** — single-channel port. The vast majority of
  ports — CV, mono mix sends, individual oscillator taps — are
  one-channel.
* **`.output(name, channels)`** — explicit channel count, `1..=255`.
  Use `2` for proper stereo ports (e.g. a reverb send that is intrinsically
  stereo). Channels above 2 are valid but no terminal verb yet does
  anything meaningful with them — see §8.
* **Names are unique within a synthdef.** Empty names and duplicates are
  rejected at synthdef-load time.
* **Legacy synthdefs** that don't call `.output(...)` keep the implicit
  single port `("out", 2 channels)` — i.e. existing stereo `Out.ar(out,
  [L,R])` synthdefs work unchanged.

> Source: `crates/vibelang-dsp/src/builder.rs::SynthDefBuilder::output` /
> `crates/vibelang-dsp/src/api.rs::SynthDefBuilderHandle::output{,_with_channels}`.

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

| Verb | Effect |
|---|---|
| `.to(group)` | Sum this port into the named group's mix bus. |
| `.to_main()` | Send straight to main hardware output (bus 0), bypassing groups. |
| `.to_current_group()` | Sum into whichever group the voice was last `.group("name")`-ed into. Errors when the voice has no explicit group set. |
| `.mute()` | Discard the port's signal. |

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
(`to`, `to_main`, `mute`, `to_current_group`). For
`to_current_group()` on a multi, the **first failing port short-circuits**
the iteration so you don't end up with a partially-applied fan-out
silently committed before the error.

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
| Port added | New bus allocated. Default route for the new port is `mute()` unless the script already declared a route for it (the script's intent always wins). |
| Port removed | Bus freed. Routes targeting the removed port are dropped, with a `tracing::warn!` log naming each dropped route. |
| Port renamed | Surfaces as **remove + add** — the old name's bus is freed, routes referring to the old name are dropped (with the same warning), and the new name gets a fresh bus + default `mute()`. Renames break routes; the rationale is that guessing intent on rename is worse than a clear warning. |

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

---

## 8. Limitations (v1)

The v1 surface is intentionally narrow. The following are documented
non-features today:

* **No per-port FX chains.** You cannot insert an FX directly on a
  `RouteHandle` (`.fx("reverb_jpverb")`). The current workaround — and
  the pattern the `spectraphon_multiout` example uses — is to send the
  port to a dedicated group whose body installs the FX. Per-port FX is
  on the v2 wishlist (kb plan §5.2 `fx()` row).
* **No CV-to-param routing.** `voice.output("cv").cv_to(target,
  "param")` is in the architecture plan (Phase 2) but not implemented.
  Routes today are audio-domain only — they sum into a destination's
  mix bus. For control-rate signal routing, keep using the
  `Modulator` API.
* **No additive fan-out per port.** Calling `.to(...)` twice on the same
  `(voice, port)` overwrites the prior destination rather than adding
  a second one. Use a second voice on the same synthdef as a workaround.
* **`Modulator` API still active.** The plan envisions `Modulator`
  becoming sugar over a single-`kr` port voice once Phase 2 lands, but
  in v1 the two systems are independent: `Modulator` for parameter
  modulation, `voice.output(...)` for audio routing. Existing
  `voice.modulate(param, modulator)` patches are unaffected.
  **v3 obsoletes `Modulator`** by folding its capabilities into the CV
  port path; until then, treat them as parallel mechanisms.
* **No `solo()`, `tap()`, `scope()`** — the wider verb table from the
  plan §5.2 is deferred. Today it's `to`, `to_main`, `to_current_group`,
  `mute`.

---

## 9. References

* `kb/voice-multi-output-cv-routing-plan.md` — full architecture proposal
  (Options A–E, phased rollout, hot-reload safety analysis, story
  breakdown).
* `examples/spectraphon_multiout.vibe` — Story 12 proof-of-life.
* `crates/vibelang-std/stdlib/instruments/spectral/spectraphon_side.vibe`
  — real-world `.output(name)` builder usage on a 4-port spectral synthdef.
* `crates/vibelang-rhai/src/api/route.rs` — `RouteHandle` /
  `MultiRouteHandle` Rhai surface.
* `crates/vibelang-rhai/src/api/voice.rs::Voice::{output_by_name,output_by_idx,outputs}`
  — voice-side handle constructors and validation.
* `crates/vibelang-core/src/handlers/routes.rs` — default routing rule
  and merge semantics.
* `crates/vibelang-core/src/reload/port_diff.rs` — reload reconciler.
* `kb/spectraphon-howto.md` — Spectraphon user manual; the multi-output
  example links back into routing here.
