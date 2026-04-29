# Voice Multi-Output + CV Routing — Architecture Plan

**Status:** research / proposal
**Ticket:** `research-voice-multi-output-cv-routing-architecture`
**Driver:** gateway feature for the eurorack-recreation hotlist (Spectraphon, Maths, Marbles,
Wogglebug, Tides, Stages, Cold Mac, Frames, EOSG triggers).

---

## 1. Current state

### 1.1 The model in one sentence

> A voice is a polyphonic synth instance whose **single stereo output** is mixed into its
> group's **stereo audio bus**, which a `system_link_audio` link synth then sums into the
> parent group's bus (or the main hardware bus 0).

### 1.2 Concrete topology

```
                 ┌────────────────────────── main bus (0/1) ──────────────► JACK
                 ▲
              link_synth (group=main)
                 ▲
        ┌────────┴───────────────┐
        │                        │
  group "drums" (bus 16/17)   group "leads" (bus 18/19)
        ▲                        ▲
   ┌────┼────┐              ┌────┼─────┐
  voice voice voice        voice    voice
  (kick)(snare)(hat)       (bass)   (pad)
        ▲                        ▲
        │                        │
   modulator → ctrl bus 1000   modulator → ctrl bus 1001
   (mapped to voice params via scsynth MapN)
```

### 1.3 Code anchors

| What | Where |
|------|-------|
| Stereo bus pair allocator (audio buses 16+) | `crates/vibelang-core/src/state.rs:817–821` (`next_bus_id += 2`) |
| Group `audio_bus` field | `state.rs:31–69` |
| Voice synth gets `out=group.audio_bus` | `handlers/voices.rs:319, 680, 992` |
| `system_link_audio` instantiation | `handlers/groups.rs:28–103` (`finalize`) |
| Group → parent / main routing | `handlers/groups.rs:42–49` |
| Effects read/write same group bus | `handlers/effects.rs:98–178`, `:159–160` |
| Modulator → control bus alloc | `handlers/modulators.rs:264, 289` |
| Voice param mapped to control bus (`MapN`) | `handlers/voices.rs:365–370, 847–860` |
| Stereo `Out.ar(out, [L, R])` codegen | `synthdefs/mod.rs:76–120` |
| Backend trait — `map_param_to_bus`, `set_param`, `create_synth` | `crates/vibelang-core/src/backends/scsynth.rs` |
| Audio-bus IDs are monotonic, never reclaimed | `state.rs:752` |
| Control-bus IDs use a free-list | `state.rs:706, 416–423` |

### 1.4 What this implies

- **Audio domain** is hierarchical and stereo-only. Voice → Group → Parent → Main. There is no
  way for a voice's signal to fan out to two different destinations, and no way for it to
  produce a non-stereo signal.
- **Control domain already exists.** Modulators already produce kr signals on control buses
  and scsynth's `MapN` already wires them into voice params. We are not inventing the CV path
  from scratch — we are **extending it so that voices, not just modulators, can be sources**.
- **Backend is scsynth via OSC.** scsynth natively supports per-channel audio buses,
  multi-channel synthdefs, and arbitrary `Out.ar`/`Out.kr` topologies. The constraint is
  vibelang's own model, not the engine.

---

## 2. The two desired features

### 2.1 (a) Multi-channel audio output per voice

A synthdef declares `N` audio output channels (e.g. Spectraphon-side = 4: sine, sub_cv, odd,
even). Each output channel is independently routable: into an fx chain, into another group,
into the main bus, or into nothing.

Use cases:
- **Spectraphon side** → 4 outs (sine / sub_cv / odd / even); user wants reverb only on the
  even-harmonic out, sub_cv → CV destination, sine + odd → main.
- **Drum bus splits** — kick → its own compressor sidechain.
- **Stereo→quad** synthdefs for surround experiments.

### 2.2 (b) DC-coupled CV outputs

A synthdef declares `M` kr-rate (or DC-coupled ar-rate) outputs that drive **another voice's
parameters**. This is the scsynth `Out.kr` path. Already used internally by modulators; the
new ask is to expose it as a first-class voice/synthdef capability.

Use cases:
- **Maths/Tides/Stages** as voices that output CV envelopes used to modulate a second voice's
  filter cutoff.
- **Marbles/Wogglebug** clock/random voices feeding rate / pitch on N other voices.
- **Spectraphon's sub_cv channel** is fundamentally a CV output — same mechanism.
- **EOSG triggers** — gate-rate signal triggers a sample-playback voice's note_on.

The two features share most of the architecture. CV is "audio at kr, mapped not summed."

---

## 3. Architecture options

Options ranked from least to most invasive.

### Option A — *N* output buses per synthdef, routed via runtime

Synthdef declares its output count. Runtime allocates an *N*-channel bus per voice (not per
group), and each channel can be independently routed (mixed into a group bus, or mapped to a
param).

- **Pros:** Clean separation between synthdef shape and routing. Reuses existing bus
  allocator. Composes naturally with effects (each fx becomes a per-channel option). Maps
  cleanly to scsynth.
- **Cons:** Voices stop sharing the group bus directly — every voice now needs its own output
  buses + a per-channel mixer into the group bus. Allocation pressure rises (each voice owns
  *N* buses instead of 0).
- **Effort:** medium-high; touches voice creation, group finalize, effects placement, reload
  reconciler.

### Option B — Named output buses on the synthdef ("ports")

The synthdef declares **named ports** (`out_sine`, `out_odd`, ...). The runtime exposes them
as routing endpoints. Internally same as Option A, but the API is name-based, not index-based.

- **Pros:** Self-documenting. Ports survive synthdef edits even if the order changes — the
  reload reconciler can match by name. Maps to the spectraphon-synthdef-plan.md vocabulary.
- **Cons:** Slightly more synthdef metadata; small risk of name collisions. Still needs all
  the per-voice bus allocation work from Option A.
- **Effort:** medium-high; a thin name-resolution layer on top of Option A.

### Option C — Dedicated "modulator voice" class for CV-only synthdefs

Keep audio voices stereo-only. Add a separate `Modulator` (or "CV voice") concept whose
output is *always* control-rate, producing one or more control buses. Re-uses the existing
modulator/control-bus path — basically promotes the modulator to a first-class voice that
can be sequenced with notes / patterns.

- **Pros:** Solves (b) without touching audio path. Tiny diff. Composes with the existing
  `voice.modulate` (param, modulator) API. *(Historical: this surface has since been removed; modern equivalent is `target.param(name).modulate_by(source, port)`.)*
- **Cons:** Solves only **half** the ticket. Still no multi-channel audio. Forces the
  Spectraphon model to split into "audio synthdef + sibling CV synthdef," which fights the
  hardware module's natural shape.
- **Effort:** low.

### Option D — Synthdef-level multi-channel return (scalar bus, channel count metadata)

Voice continues to own a single bus, but the bus is *N* channels wide. Routing decisions
happen by channel index on that bus.

- **Pros:** Smallest change to bus allocator (still one bus per voice, but width *N*
  instead of 2).
- **Cons:** scsynth's `Out`/`In` UGens deal with consecutive bus indices, not "wide buses,"
  so there is no real saving — the runtime still has to track *N* indices. Routing API ends
  up indexing into a tuple anyway. No real win over Option A. Effects assume stereo width on
  the bus they read.
- **Effort:** medium; mostly a worse Option A.

### Option E — Control-bus injection at runtime, no synthdef changes

Stay with stereo-only audio voices. For CV, add a runtime hook: **pluck** the voice's audio
bus, downsample to kr, expose as a control bus, allow mapping to params.

- **Pros:** No synthdef changes. Zero effort on the voice/synthdef side.
- **Cons:** A control voice would have to fake itself as audio, then be coerced back to kr —
  semantically wrong and lossy. Doesn't solve multi-output. CV signals aren't really audio
  (DC, no AA filter, sample-rate matters differently).
- **Effort:** low, but a dead end for hotlist features.

---

## 4. Recommended path

**Option B (named ports) for both audio and CV outputs**, implemented in three phases.

Why Option B:

- It generalises cleanly: an output port is `(name, rate, channels)` where `rate ∈ {ar, kr}`
  and channels is usually 1 or 2. Multi-channel audio (a) is just `("even", ar, 2)`. CV (b)
  is just `("sub_cv", kr, 1)`. One mechanism for both halves of the ticket.
- Names are stable across synthdef edits, which the reload reconciler needs.
- It fits the way the eurorack-recreation hotlist already talks about modules (every plan doc
  refers to outputs by name, not index).
- Adds the smallest possible API surface area: one builder method (`route()`).

Why not C (CV-only voice class): it splits the model artificially. The hotlist is full of
modules that produce audio *and* CV from the same engine; making the language shape match
the hardware shape is the whole point.

### 4.1 Phasing

**Phase 1 — Synthdef ports + multi-channel audio (no CV yet)**
- Add `OutputPort { name: String, rate: Rate, channels: u8 }` to the synthdef descriptor.
- Default for legacy synthdefs: a single port `{ name: "out", rate: Ar, channels: 2 }`.
  Codegen unchanged — existing `Out.ar(out, [L,R])` becomes the implementation of port `out`.
- New `Out.ar(out_<name>, ...)` codegen for additional ports; one synthdef param per port.
- Voice now owns `Vec<BusId>` (one entry per port) instead of relying on group bus.
- Default routing for each port: sum into the voice's group bus (current behaviour for the
  legacy `out` port; new ports default to *unrouted = silent* until the user writes a
  `.route()` line). Backwards compatible with every existing example.
- Rhai: `voice.route(name).fx(...).to_group("...")` — see §5.

**Phase 2 — CV ports + voice→param mapping**
- Same port machinery, with `rate=Kr`. Synthdef writes `Out.kr(out_<name>, signal)`.
- Routing target: another voice's parameter. Internally calls the existing
  `backend.map_param_to_bus` (`voices.rs:847–860`) — the path the modulator system uses
  today. A modulator becomes a special-cased voice with a single kr port; the existing
  modulator handler can either stay as sugar or be reduced to that case in a follow-up.
- `voice.route("sub_cv").cv_to(voice("bass"), "amp")`.

**Phase 3 — Reload reconciler & quality of life**
- Audio-bus free-list (eliminate the monotonic-bus leak in `state.rs:752`).
- Port-name diffing across synthdef reloads: keep the bus when the port name survives, free
  on rename or delete. CV connections keyed by `(source_voice, port_name) → (target_voice,
  param)`, surviving synthdef recompiles as long as port names match.
- Per-port mute/solo, `tap()` for a probe/scope, examples for spectraphon split routing.

---

## 5. Rhai API sketch

### 5.1 Source side — declaring outputs in a synthdef

Synthdefs are Rust today; the port set is declared in the synthdef descriptor (whatever
struct lives in `vibelang-std/synthdefs/`):

```rust
SynthDef::new("spectraphon_side")
    .port("sine",    Rate::Ar, 1)
    .port("sub_cv",  Rate::Kr, 1)
    .port("odd",     Rate::Ar, 2)
    .port("even",    Rate::Ar, 2)
    // ... params, ugen graph
```

Legacy synthdefs that don't call `.port()` keep the implicit `{ "out", Ar, 2 }` port.

### 5.2 User side — routing in `.vibe` scripts

```rhai
let speak = voice("speak")
    .synth("spectraphon_side")
    .group("leads");

// audio routing — fan-out to different fx chains
speak.route("odd").fx("reverb_jpverb").to_group("leads");
speak.route("even").fx("delay_pingpong").to_group("leads");
speak.route("sine").to_group("leads");          // dry, default
// "sub_cv" left implicit → silent unless cv_to'd

// CV routing — drive another voice's param
let bass = voice("bass").synth("acid_303").group("bass");
speak.route("sub_cv").cv_to(bass, "cutoff");

// shorthand index form for one-off cases / when names are awkward
maths.route(0).cv_to(bass, "amp");
```

`route(name)` returns a `RouteHandle` builder. Terminal verbs:

| Verb | Domain | Effect |
|------|--------|--------|
| `to_group(g)` | ar | Sum this port into group `g`'s audio bus |
| `to_main()` | ar | Sum into bus 0 |
| `fx(name)` | ar | Insert an fx (chainable). Auto-allocates an intermediate bus if there's any later routing |
| `cv_to(voice, param)` | kr | Map this port onto another voice's param via `MapN` |
| `mute()` / `solo()` | ar/kr | Per-port mute/solo |
| `tap()` / `scope()` | ar/kr | Probe — phase 3, used by GUI/lsp |

Default routing rule when a synthdef has multiple ports and the user routes none of them:
the legacy port named `"out"` (if present) goes to group; everything else is silent. This
keeps single-port synthdefs working unchanged.

---

## 6. Hot-reload safety

Three categories of change to plan for:

### 6.1 Synthdef recompiles (same synthdef, edited)
- **Port set unchanged:** keep all bus allocations, keep all `.route()` connections. Only the
  synth nodes get freed/recreated, exactly like today.
- **Port added:** allocate a new bus; default route (silent for new non-`out` ports). Existing
  routes survive.
- **Port removed:** free the bus, drop dependent routes, log a warning naming the removed
  route(s) so the user can clean up the script. Reconciler must avoid stale `MapN` mappings
  pointing at a freed control bus — the modulator code already has the right shape for this
  (see free-list at `state.rs:416–423`); extend the same pattern to audio.
- **Port renamed:** treat as remove+add. Document that renames break routes; rationale is that
  guessing intent on rename is worse than a clear error.

### 6.2 Voice replaces its synthdef
- Recompute the port set from the new synthdef.
- For each existing route on the voice, match by port name. Dropped names → drop route.
- Same warning semantics as 6.1.

### 6.3 Voice deleted / group deleted
- Free all owned audio buses (via the new free-list).
- For every CV route *targeting* the deleted voice's params, drop the `MapN` and free
  the source-side route.
- Reverse direction: routes *from* the deleted voice — drop them, log a warning at any
  remaining target voice.

### 6.4 Bus reuse hazards
- Audio bus ID reuse becomes possible once we add a free-list. Reload races (free → realloc
  in same tick) must guarantee the old `MapN`/`Out` synth has been freed before the new
  consumer maps the same ID. Today's groups handler has the right pattern (`finalize` runs
  after diff is applied). Mirror it for routes: a `RoutesHandler::finalize()` after both the
  voices/effects/modulators handlers have run.
- Control-bus reuse is already correct via the free-list.

### 6.5 Allocation
- Per-voice port count is bounded (small), so we can pre-allocate at synth creation. Worst
  case is the spectraphon (4 ar ports + 1 kr) per voice — a handful of buses. Fine for
  hundreds of voices.

---

## 7. Estimated effort — story breakdown

> SP using the project's ~1 SP ≈ 10-min agent task convention.

| # | Story | Crate(s) | SP |
|---|-------|----------|----|
| 1 | `OutputPort` struct on synthdef descriptor; default port for legacy synthdefs | `vibelang-core/src/synthdefs/`, `vibelang-std` | 2 |
| 2 | Per-voice bus allocation (`Vec<BusId>` keyed by port name) | `vibelang-core/src/state.rs`, `handlers/voices.rs` | 3 |
| 3 | Audio bus free-list (replace monotonic `next_bus_id`) | `vibelang-core/src/state.rs` | 2 |
| 4 | `Out.ar(out_<name>, …)` codegen for multi-port synthdefs | `vibelang-core/src/synthdefs/`, `synthdefs/mod.rs:76–120` | 2 |
| 5 | Default per-port routing (legacy `out` → group, others silent) + group sum | `handlers/groups.rs`, `handlers/voices.rs` | 2 |
| 6 | `RoutesHandler` + diff/finalize, mirrors `EffectsHandler` shape | `vibelang-core/src/handlers/`, `state.rs`, `reload/mod.rs` | 4 |
| 7 | Effects: route audio through a port-owned bus instead of group bus | `handlers/effects.rs:98–178` | 3 |
| 8 | Rhai `voice.route(name)` builder + `RouteHandle` terminal verbs | `vibelang-rhai/src/api/voice.rs`, new `route.rs` | 3 |
| 9 | CV routing: `cv_to(target, param)` reusing `map_param_to_bus` | `vibelang-rhai/src/api/voice.rs`, `handlers/voices.rs:847–860` | 2 |
| 10 | Reload reconciler — port diff (add/remove/rename) + warnings | `vibelang-core/src/reload/` | 3 |
| 11 | Convert `Modulator` to be sugar over a single-kr-port voice (or document keeping it parallel) | `handlers/modulators.rs`, `vibelang-rhai/src/api/modulator.rs` | 2 |
| 12 | Examples: spectraphon split routing, maths→bass cutoff, marbles→pitch | `examples/` | 2 |
| 13 | Docs: kb update, CLAUDE.md routing section, GETTING_STARTED snippet | `kb/`, `CLAUDE.md`, `GETTING_STARTED.md` | 1 |
| 14 | Tests: route reload survives port-set unchanged; warns on port removal; CV route reflects in target param | `tests/` | 3 |

**Total: ~34 SP** (≈ 5–6 hours of focused agent work, comfortably split across phases 1/2/3
above: ~12 / ~10 / ~12 SP).

---

## 8. Open questions (defer until implementation)

- Should `route()` return a handle that survives across hot-reloads (i.e. is identified by
  port name), or is each call a fresh route description that the reload reconciler matches by
  position? Lean: name-based, matches modulator semantics.
- Should we allow a single port to fan out to multiple destinations (`speak.route("sine")
  .to_group("a"); speak.route("sine").to_group("b");`)? scsynth supports this trivially via
  multiple `In.ar` readers; the question is API ergonomics. Lean: yes, additive — successive
  calls add destinations, with an explicit `.clear_routes(port)` to reset.
- For kr ports, do we expose ar-rate routing as well (e.g. for trigger pulses needing
  sample-accurate timing)? Lean: yes, `Rate::Ar` ports can also be `cv_to`'d — `MapN` works on
  any bus.
- Modulator deprecation: keep current `Modulator` API as sugar, or delete after CV port
  voices land? Lean: keep as sugar, tag for deprecation in a follow-up ticket.
