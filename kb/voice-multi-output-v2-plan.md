# Voice Multi-Output v2 — Plan (CV-to-param routing + per-port FX)

**Status:** planning
**Epic:** `epic-voice-multi-output-v2-cv-to-param-routing-per-port-fx`
**Builds on:** `epic-voice-multi-output-routing-per-port-group-routing-v1` (closed 2026-04-29)

---

## 1. Goals

1. **kr-rate output ports** — synthdefs declare control-rate outputs.
2. **CV-to-param routing** — `voice.output("env").to_param(other, "cutoff")` maps a kr port at another voice's parameter via scsynth's `MapN`.
3. **Per-port FX chain** — a chainable modifier on the route handle that inserts FX synths *before* the port joins the group bus. (Implemented under Stories 6a/6b, then reverted post-implementation — see §9.)
4. **Modulator obsolescence** — existing `modulator()` becomes sugar over a single-kr-port voice with a `to_param` route. Old API keeps working; deletion is v3.

## 2. v1 recap (already in tree)

- `OutputPort { name, channels }` + per-port `Out.ar` codegen.
- Per-voice audio-bus map; AudioBusAllocator with free-list.
- `RoutesHandler` with `RouteDest::{Group, Main, Muted}`, name-keyed registry, finalize emits `port_to_group_link_<n>` mixer synths.
- Reload reconciler diffs port adds/removes/renames; routes survive name-stable.
- Rhai `voice.output(name|idx).to(...)` + plural `outputs([…])` + `to_current_group()`.
- Default count-based routing (1-pan / 2-LR / N>2-first2-rest-silent).

## 3. Architecture additions

### 3.1 `OutputPort.rate: PortRate { Ar, Kr }`

Default Ar (legacy zero-regression). Codegen at `synthdefs/mod.rs` picks `Out.kr` for kr ports, `Out.ar` for ar. Rhai builder gains `.output_kr(name)` / `.output_kr(name, channels)` alongside the existing ar form.

### 3.2 Bus allocator picks audio vs control bus by port rate

Voice's `output_buses: Vec<(String, BusId)>` already exists. For kr ports, alloc via the existing control-bus free-list at `state.rs:706, 416-423`. Mixed-rate voices own a mix of audio + control bus IDs; the rate disambiguates which allocator to use at create/drop.

### 3.3 `RouteDest::Param { voice_id, param_name }`

Add the variant. Group/Main/Muted unchanged. `RoutesHandler::finalize` on Added Param: emit a `MapN`-equivalent backend message that maps `target_voice.<param>` to the source port's control bus. Mirror the existing modulator path at `handlers/voices.rs:847-860`. Removed Param: unmap (set param back to its default value or unmap-to-direct).

### 3.4 Per-port FX chain

`RouteHandle.fx(names: Vec<String>)` returns the same handle (chainable). Internally each FX in chain owns an intermediate audio bus:

```
port_bus → fx[0]_bus → fx[1]_bus → ... → port_to_group_link → group_bus
```

Bus alloc: reuse the port's source bus as `fx[0]`'s input; allocate one fresh intermediate per subsequent FX. v1 group-level FX continues working unchanged for any port not using per-port FX.

### 3.5 Modulator-as-sugar

Existing `modulator("env").adsr(...)` keeps working. Internally: builder generates a synthdef with a single kr port `"out"` and the equivalent UGen graph; voice creation goes through the standard voice path. `voice.modulate("amp", m)` becomes sugar for `m.output("out").to_param(voice, "amp")`. No user-visible API change; one fewer core type internally.

## 4. Story breakdown

```
Story 1 (kr port descriptor + codegen)        ─┬─► Story 2 (control-bus alloc per kr port)
                                               │
                                               ├─► Story 3 (RouteDest::Param + finalize MapN)
                                                          │
                                                          ├─► Story 4 (Rhai .to_param API)
                                                          │           │
                                                          │           └─► Story 5 (Modulator-as-sugar refactor)
                                                          │
                                                          └─► Story 7 (Eurorack proof: port cv_maths)
                                                          │
                                                          └─► Story 8 (reload port-rate diff)

Story 1 ────► Story 6a (per-port FX bus alloc) ────► Story 6b (Rhai RouteHandle.fx API)

Story 4, 6b ─► Story 9 (tests) ────► Story 10 (docs)
```

| # | Story | SP | Dep |
|---|-------|----|-----|
| 1 | kr-port descriptor + codegen | 2 | — |
| 2 | control-bus alloc per kr port | 2 | 1 |
| 3 | RouteDest::Param + finalize MapN | 2 | 2 |
| 4 | Rhai `.to_param(voice, name)` | 2 | 3 |
| 5 | Modulator-as-sugar refactor | 2 | 4 |
| 6a | per-port FX intermediate bus alloc + finalize | 2 | 1 |
| 6b | Rhai `RouteHandle` per-port FX API | 1 | 6a |
| 7 | Eurorack proof: port cv_maths to kr + example | 2 | 4 |
| 8 | reload reconciler — port-rate change diff | 2 | 3 |
| 9 | tests | 2 | 4, 6b |
| 10 | docs | 1 | 9 |

**Total: 20 SP**

Phasing:
- **Phase A** (Stories 1, 2 — 4 SP): kr-port plumbing.
- **Phase B** (Stories 3, 4 — 4 SP): CV-to-param works end-to-end.
- **Phase C** (Stories 5, 7 — 4 SP): modulator obsolescence + first eurorack proof.
- **Phase D** (Stories 6a, 6b — 3 SP): per-port FX (parallel with B/C — only deps Story 1).
- **Phase E** (Stories 8, 9, 10 — 5 SP): reload, tests, docs.

## 5. Architectural decisions

**Q1 — ar→param coercion?**
**No, kr only.** Force users to declare CV synthdefs with `output_kr`. Auto-coercion via `A2K.kr` is doable but hides rate-mismatch bugs. Re-evaluate in v3.

**Q2 — per-port FX bus reuse**
**Reuse the port's source bus as fx[0]'s input.** Saves one bus per chain. Each subsequent FX allocs a fresh intermediate.

**Q3 — Modulator removal timeline**
**Keep as sugar in v2. Mark deprecated in CLAUDE.md. Remove in v3 after a "stale modulator" lint warns ≥1 minor version.**

**Q4 — mixed-rate ports per synthdef**
**Allowed.** Spectraphon-style with audio outs (sine, odd, even) plus a kr `sub_cv` port is the natural shape. Codegen + bus alloc already key off rate per port.

**Q5 — multiple `to_param` routes from one source port**
**Allowed.** One source port can drive params on multiple voices. Registry stores `Vec<RouteDest::Param>` per source. Group dests stay single (replace semantics) — fan-out to multiple groups is v3.

**Q5b — multi-source fan-in to one param (`env + lfo → cutoff`)**
**Now supported.** When N ≥ 2 kr sources route to the same `(target_voice, target_param)`, the runtime allocates an intermediate control bus and spawns a `param_kr_sum_<N>` summer synth (N in 2..=8) that sums the source buses; the target's `/n_map` then binds to the intermediate bus instead of any one source. Teardown is incremental: removing one of N sources respawns a smaller summer, dropping to N=1 collapses back to direct `/n_map`, dropping to N=0 emits the unmap sentinel. Hot-reload preserves the summer when the source set is unchanged (the empty-diff short-circuit in `finalize_params`). Both `.to_param` and `.modulate_by` feed the same registry, so the API surface direction doesn't matter for fan-in.

## 6. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Modulator-as-sugar refactor breaks edge cases (ratchet, gate trig) | medium | Keep old code path behind a feature flag for one cycle; integration tests cover each modulator method |
| Per-port FX bus accounting drifts under hot-reload | medium | Reuse v1 reload diff machinery (Story 10); add tests for FX add/remove on a routed port |
| `MapN` with reload-time bus reuse maps wrong source | low | Same finalize-after-teardown ordering invariant from v1 Story 7 |
| Users mix ar/kr rates and confuse error messages | medium | Builder rejects mismatched routes with clear msg: "port 'env' is kr-rate; use `.to_param(voice, param)` not `.to(group)`" |

## 7. Modulator obsolescence path

1. **v2 Story 5**: rewrite `modulator()` builder to emit a single-kr-port synthdef internally; old user-facing API unchanged.
2. **v2 Story 10 docs**: announce "modulator() is now sugar over voice.output_kr() + .to_param()".
3. **v3 (out of this epic)**: deprecation warning + removal.

## 8. NOT in v2

- Sample-accurate trigger ports (would need `Out.tr` semantics) — defer.
- ~~Multi-source fan-in to a single param (averaging/summing modulators)~~ —
  **landed**: `param_kr_sum_<n>` summer synthdefs + `State::param_summers`
  bookkeeping in `RoutesHandler::finalize_params`. Unweighted sum only;
  weighted/scaled-per-source mix is v3.
- Multi-source fan-in beyond 8 sources — truncated with a warning. Build a
  hierarchical summer (`sum(sum(a,b,c,d), sum(e,f,g,h), ...)`) in v3 if a
  patch ever needs it.
- ar-rate fan-in to one bus — keep using sub-groups (the audio-rate fan-in
  pattern is mix-bus summing at the group level, which the v1 routing
  machinery already covers).
- Fan-out to multiple **groups** from one port — registry-side support is
  trivial but each fan-out leg needs its own `port_to_group_link_<n>` mixer
  synth, plus a teardown story that interleaves with the existing single-
  destination replace semantics. v3.
- ar→kr auto-coercion — defer.
- Full Modulator removal — v3.

## 9. Post-implementation revert: Stories 6a / 6b (per-port FX)

Stories 6a (per-port FX intermediate-bus alloc + finalize) and 6b
(`RouteHandle` per-port FX Rhai surface) shipped, then were reverted
post-implementation. Sub-group routing (a group that owns the FX
chain + voices that route a port to that group) covers every musical
use case for "send a port through FX" with strictly better economics:

- **Shared FX state** across every voice routed into the sub-group, so
  reverb tails ring continuously instead of restarting per voice.
- **Fewer buses**: one bus chain on the sub-group, vs. one chain per
  `(voice, port)` pair under the per-port FX modifier.
- **No new API surface**: the canonical idiom is `voice.output("p")
  .to(group("fx_send"))`, which uses only the existing group + effect
  + route machinery.

Indefinitely deferred — won't reintroduce in v3 unless a concrete
use case emerges that sub-groups can't address (e.g. genuinely
per-voice independent FX state where a sub-group-per-voice is too
heavyweight).

See `kb/voice-multioutput-howto.md` §3b for the user-facing pattern.
