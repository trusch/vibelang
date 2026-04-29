# SynthDef + Rhai Arity Limits — Architecture Plan

**Status:** research only, no code.
**Ticket:** `research-plan-lift-the-synthdef-param-and-rhai-arg-arity-limits`
**Date:** 2026-04-28

Two related caps are throttling the synthdef API. This plan traces both to their sources, lays out the option space, and recommends a phased path.

---

## 1. Where the limits actually come from

### 1.1 The synthdef-param cap (10) is **ours**, but bounded by Rhai's `FnPtr::call`

The 10-param ceiling is **not** an arbitrary builder choice — it sits at `crates/vibelang-dsp/src/builder.rs:379-493` in `build_body_closure_with_options`, as an exhaustive `match` on `param_nodes.len()`:

```rust
let result: rhai::Dynamic = match param_nodes.len() {
    0 => closure.call(&engine, &empty_ast, ()),
    1 => closure.call(&engine, &empty_ast, (param_nodes[0].clone(),)),
    2 => closure.call(&engine, &empty_ast, (param_nodes[0].clone(), param_nodes[1].clone())),
    ...
    10 => closure.call(&engine, &empty_ast, (..., param_nodes[9].clone())),
    _ => return Err(SynthDefError::RhaiError("Too many parameters (max 10)".to_string())),
};
```

The same `0..=10` match is duplicated in:

- `build_effect_closure` (lines 148-270) — `input` + 10 user params (11 total)
- `build_modulator_closure` (lines 696-810) — 10 user params

The ceiling is **forced by Rhai's `FnPtr::call`**, whose `IntoFuncArgs` blanket impls cover tuples of length 0..=10. To pass 11+ args positionally, we'd need either a tuple-of-tuples trick, slice-based call, or a Rhai-side macro bump. Calling `closure.call(&engine, &empty_ast, (a, b, c, ..., k))` for 11 args fails to compile under stock Rhai.

**Net:** the 10 cap is jointly enforced by us *and* by Rhai's `IntoFuncArgs` trait family. Lifting only one side does nothing.

### 1.2 The Rhai-arg cap (20) is the `def_register!` macro

`~/.cargo/registry/src/index.crates.io-*/rhai-1.23.6/src/func/register.rs:245`:

```rust
def_register!(A:20, B:19, C:18, D:17, E:16, F:15, G:14, H:13, J:12, K:11,
              L:10, M:9, N:8, P:7, Q:6, R:5, S:4, T:3, U:2, V:1);
```

This recursively expands to **four `RegisterNativeFunction` impls per arity** (Fn / Fn+Ctx / Fn→Result / Fn+Ctx→Result), giving 21 arities × 4 = 84 impls. Compile cost is approximately O(N²) in the macro invocation — going to 32 adds 11 more arities × 4 impls and is reportedly survivable; going to 64 is not.

The **escape hatch is `register_raw_fn`** (`rhai-1.23.6/src/api/register.rs:99-132`):

```rust
pub fn register_raw_fn<T: Variant + Clone>(
    &mut self,
    name: impl AsRef<str> + Into<Identifier>,
    arg_types: impl AsRef<[TypeId]>,
    func: impl Fn(NativeCallContext, &mut FnCallArgs) -> RhaiResultOf<T> + SendSync + 'static,
) -> &mut Self
```

`arg_types: impl AsRef<[TypeId]>` is a slice — **no arity limit**. The closure receives `&mut FnCallArgs` (`&mut [&mut Dynamic]`) and must do its own type-unpacking. This is exactly the slot we'd plug high-arity UGens into.

### 1.3 Current vibelang usage

- **All UGen registrations** go through `register_fn` via codegen in `crates/vibelang-dsp/build.rs:323-402`. The codegen emits a separate `register_fn` per arity (0..N) to support default arguments — so a 22-input UGen would emit a 22-arg `register_fn` and **fail to compile** at the rhai layer.
- **No `register_raw_fn` usage anywhere** in the workspace (yet).
- **No `[patch.crates-io]` for rhai** — workspace pins `rhai = "1"` / `"1.17"`, resolving to 1.23.6.
- **No OteyPiano UGen exists in the current tree** (the ticket reference is forward-looking — a UGen we want to add but cannot under the current stack).

### 1.4 Reload-reconciler context

`crates/vibelang-core/src/reload/mod.rs` does **not** track synthdef body signatures or arities — it diffs *parameter values* per voice/group/effect/modulator/pattern. Changing the body API surface is therefore reload-safe in principle, **provided** the synthdef name + param-name set stay stable. No reconciler logic needs to change for any option below.

### 1.5 Real-world impact (corroborates the ticket)

`crates/vibelang-std/stdlib/instruments/spectral/spectraphon_dual.vibe:40-79` is sitting *exactly* at 10 params, with a comment block enumerating what was folded away to fit:

```
Parameter folding (Rhai 10-arg body cap; spectraphon_side.vibe hits the same wall):
- mode_a / mode_b dropped — both sides fixed at SAO with the Story A 1/k saw default.
- array_idx_a / array_idx_b dropped — implicit Array 0.
- partials, slide, focus shared between sides.
```

This is the canonical "the cap is shaping the API" smoking gun.

---

## 2. Option space — synthdef-param body

| Opt | Idea | Lifts cap? | Fork rhai? | Static names? | BC? | Compile cost |
|---|---|---|---|---|---|---|
| **A** | Map-typed body: `.body(|p: Map| p.freq + p.cutoff)` | ∞ | no | no¹ | yes (additive) | none |
| **B** | Typed struct body: `.body(|p: Params|)` | ∞ | partial² | yes | yes (additive) | low |
| **C** | Fork rhai: bump `def_register!` to 32 **and** `IntoFuncArgs` tuple impls to 32 | 32 | yes | yes | yes (transparent) | +O(N²) compile time |
| **D** | Generate per-arity wrappers in synthdef API (macro_rules!/build.rs) | gated by C | no alone | yes | yes | low |

¹ Names exist as map keys but the LSP / ergonomic story is weaker — see §2.1.
² A custom Rhai `CustomType` for `Params` is doable but is essentially a typed wrapper around a map.

### 2.1 Option A — Map-typed body (recommended primary)

```rhai
define_synthdef("hoa_decoder", |b| {
    b.param("freq", 220.0)
     .param("partials", 1.0)
     .param("cutoff", 8000.0)
     .param("res", 0.3)
     // ... 30 more params
     .body_map(|p| {
         let freq = p.freq;
         let cutoff = p.cutoff;
         // ...
     })
});
```

- **No arity limit** — `Map` is a single `Dynamic` arg.
- **No rhai fork.**
- **Builder change is local**: add `body_map(closure: rhai::FnPtr)` next to `body(closure: rhai::FnPtr)`. Inside, build a Rhai `Map` from `param_nodes` and call `closure.call(&engine, &empty_ast, (map,))` — the 1-arg path is already in the existing `match`.
- **Cost**: param destructuring lives in user code (`let freq = p.freq`); LSP can autocomplete map keys if we extend the LSP's per-synthdef param table to feed completion (it already knows the param list).
- **BC**: existing `.body(|a, b, ...|)` keeps working unchanged. Authors opt into `.body_map` only when they exceed 10.

**Risk**: divergence between two body styles in the stdlib. Mitigate with a stdlib lint / convention: ≤6 params positional, 7+ via map.

### 2.2 Option B — Typed struct body

A `CustomType`-registered `Params` newtype with `get_<name>` / `set_<name>` accessors generated per synthdef. Strictly nicer than A (real names, real type), but Rhai is dynamically typed and the runtime would still resolve `p.freq` through Dynamic dispatch. The implementation cost is meaningfully higher (per-synthdef codegen of a Rhai `CustomType`) for ergonomics that mostly come from the LSP — which can already provide them on top of A.

Verdict: **deferred**. Revisit if A's ergonomics prove painful in stdlib authoring.

### 2.3 Option C — Fork rhai to 32

Two macro changes are needed, not one:

1. `def_register!(A:32, B:31, ..., V:1)` in `func/register.rs:245`.
2. `IntoFuncArgs` blanket tuple impls (in `func/args.rs` / `func/call.rs`) extended to 32-tuples — this is what gates `FnPtr::call((a,b,c,...,k))`. Without this, the synthdef builder still cannot pass 11+ args positionally.

Plus mirror changes to the matching macro in vibelang's `builder.rs` (extend the `match param_nodes.len()` arms 11..=32).

**Costs**:
- Maintenance: pinned fork, rebase on each rhai upstream bump. Not theoretical — rhai is alive (1.23.6 was recent).
- Compile time: O(N²) blow-up in the macro. 20→32 is ~2.5× the impl count for `def_register!`; tolerable but measurable.
- Doesn't help the long tail (HOA UGens 30+, Eurorack module recreations) — 32 is just a higher wall.

**Lift quotient**: bounded. Pays the fork tax for a finite ceiling.

### 2.4 Option D — Per-arity wrappers in our API

Without C, D buys nothing on the body side: even if we expose `.body_11(|...|)`, Rhai's `IntoFuncArgs` still won't compile for 11-tuples. D is only meaningful as a *consumer* of C — it doesn't stand alone for synthdef bodies.

(D is conceptually distinct from the UGen codegen in §3, which is a similar idea but for *registration*, and is independently viable.)

---

## 3. Option space — UGen registration

| Opt | Idea | Lifts cap? | Fork rhai? | Codegen change |
|---|---|---|---|---|
| **A** | Bump rhai's `def_register!` to 32 (same fork as §2.3) | 32 | yes | none |
| **B** | Use `register_raw_fn` for **all** UGens | ∞ | no | rewrite codegen to emit `&mut FnCallArgs` unpacking |
| **C** | Hybrid: `register_fn` for arity ≤ 20, `register_raw_fn` for arity > 20 | ∞ | no | branch in codegen |

### 3.1 Option C (hybrid) is the sweet spot

`build.rs` already inspects each UGen's input count when generating per-arity overloads. Adding a branch at codegen time — "if total inputs > 20, emit `register_raw_fn` with manual type-unpacking; else keep `register_fn`" — is a localised codegen change.

Sketch of what the >20 branch generates (illustrative; **not** for inclusion in code, this is research):

```
engine.register_raw_fn(
    "otey_piano_ar",
    &[TypeId::of::<Dynamic>(); 24],
    |_ctx, args: &mut FnCallArgs| {
        let arg0 = std::mem::take(args[0]).cast::<Dynamic>();
        let arg1 = std::mem::take(args[1]).cast::<Dynamic>();
        // ... 22 more
        otey_piano_ar(&arg0, &arg1, ..., &arg23).map_err(...)
    },
);
```

The unpacking is mechanical — codegen handles it once. Default-arg overloads up to 20 still go through `register_fn` (same path as today).

**Addendum (2026-04-29, hotfix `arity-2-hotfix-register-raw-fn-must-use-array-param-not-dynamic-typed-slots`):** The sketch above is wrong on a load-bearing detail — `register_raw_fn` performs **exact** `TypeId` matching at dispatch, so a slot list of `&[TypeId::of::<Dynamic>(); N]` never matches a user call like `big_arity24_ar(1.0, 2.0, …)` (Rhai builds a lookup of `N × TypeId::of::<f64>()`). The function is registered but never called. There is no "wildcard" `Dynamic` slot in `register_raw_fn` — that exists only in the `register_fn` typed-tuple path. The actual fix is to register a **single `rhai::Array` parameter** (`&[TypeId::of::<rhai::Array>()]`), unpack the array inside the closure, and validate `array.len() == N` with a clean runtime error otherwise. User-side calls for >20-input UGens therefore **must** wrap arguments in `[…]`: `big_arity24_ar([1.0, 2.0, …, 24.0])`. Defaults are not filled by Rhai on this path — exact arity is required. The `≤20` `register_fn` path is unchanged and continues to support default-argument overloads. Codegen lives in `crates/vibelang-dsp/build.rs` and the round-trip is exercised by `crates/vibelang-dsp/tests/high_arity_codegen.rs`.

### 3.2 Why not B alone?

- Slower: every UGen call goes through `Dynamic`-cast unpacking instead of typed trait dispatch.
- Higher binary size: lose `register_fn`'s type-elaborated impls.
- Loses some of rhai's automatic coercions that `register_fn` gives for free.

Hybrid (C) keeps the fast path for the 99% of UGens that fit and only pays the raw-fn cost where unavoidable.

### 3.3 Why not A alone?

Same problems as §2.3 — fork tax for a finite ceiling. HOA UGens with 30+ inputs would still need raw_fn anyway; C subsumes A's win at zero fork cost.

---

## 4. Recommendation: **A (synthdef) + C-hybrid (UGen)**

**Combined name:** "Map body + raw-fn fallback".

**Why this combo:**

- **No rhai fork.** Both fixes stay in vibelang. We keep upgrading rhai cleanly.
- **Unbounded.** Removes the 10-cap and the 20-cap entirely, not just raises them.
- **Backwards compatible.** Every existing synthdef and every existing UGen registration keeps compiling unchanged. New code opts into the new path.
- **Localised blast radius.** Two files change: `crates/vibelang-dsp/src/builder.rs` (add `body_map`, ditto for FX/modulator) and `crates/vibelang-dsp/build.rs` (codegen branch at >20 inputs). No reconciler changes, no LSP-breaking changes, no Cargo lock churn.
- **Compile-time neutral.** No O(N²) macro blow-up.
- **Future-proof.** HOA UGens, Maths/Marbles recreations, OteyPiano (24), spectraphon-style dual instruments all unblocked under one consistent rule.

**What we explicitly reject:**

- **Forking rhai (Option C in §2):** the fork tax is real and the ceiling is finite. Only revisit if a future rhai-sourced limitation appears that can't be bypassed via raw-fn or maps.
- **Typed struct body (Option B in §2):** the win over Map is mostly LSP-shaped, and the LSP can deliver that on top of Map cheaper than `CustomType` codegen would.

### 4.1 Phased plan

**Phase 1 — Foundations (1 SP).** Document the convention in `kb/`: "≤6 params positional, 7-10 positional acceptable, >10 must use `body_map`". Add a CI lint that warns on positional `.body` with >10 params (already an error today; promote to documented convention so authors don't trip into it).

**Phase 2 — Map body for synthdef/FX/modulator (3 SP).** Add `body_map(closure: FnPtr)` alongside `body` in `crates/vibelang-dsp/src/api.rs`. In `builder.rs`, the implementation builds a Rhai `Map<String, Dynamic>` from `param_nodes` and dispatches via the existing 1-arg `closure.call` arm. Mirror for FX and modulator builders.

**Phase 3 — Stdlib pilot (2 SP).** Migrate `spectraphon_dual.vibe` to `body_map`, restoring the folded-away params (`mode_a`, `mode_b`, `array_idx_a`, `array_idx_b`, per-side `partials`/`slide`/`focus`). Migrate `spectraphon_side.vibe`. These two are the canonical proof points and immediately deliver the user-visible win.

**Phase 4 — UGen codegen hybrid (3 SP).** Extend `crates/vibelang-dsp/build.rs` to emit `register_raw_fn` for any UGen whose input count exceeds 20. Manual type-unpacking template generated once, applied per high-arity UGen. Add OteyPiano (24 inputs) to the manifest as the proof point.

**Phase 5 — LSP completion for map params (2 SP).** The LSP already has the per-synthdef param list. Wire it into completion for `body_map` closures: typing `p.` inside a `body_map` body offers the synthdef's declared params. Closes the ergonomic gap vs positional.

**Phase 6 — HOA / Eurorack expansion (open-ended).** Implement Maths, Marbles, and the deferred HOA UGens against the new ceiling-free API.

### 4.2 Migration story for existing 100+ synthdefs

**Zero forced migration.** Existing positional `.body` keeps working. Authors migrate to `body_map` only when:

- They exceed 10 params, **or**
- They voluntarily prefer named access for readability.

Stdlib migration is opportunistic, not flag-day.

### 4.3 Tests / guardrails

- **Builder unit tests** in `crates/vibelang-dsp/src/builder.rs`: add round-trip tests for `body_map` at arities 1, 5, 11, 20, 50.
- **Stdlib regression test**: existing `spectraphon_dual` IR before migration vs after migration — assert GraphIR equivalence on the shared params.
- **UGen codegen test**: synthesise a fake 24-input UGen in a test manifest, assert generated code uses `register_raw_fn` and round-trips through a Rhai eval.
- **Reload regression**: hot-reload a `body_map` synthdef with new param values; assert reconciler emits parameter-update patches (not full rebuild).

### 4.4 Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| Map-style stdlib drifts in convention (some `.body`, some `.body_map`) | medium | Documented threshold (§4.1 Phase 1); CI lint |
| `register_raw_fn` has subtler error reporting than `register_fn` | low | Codegen template includes context-aware error wrapping; covered in tests |
| Future rhai upgrade changes `FnCallArgs` shape | low | Pin rhai minor version; unblock during upgrade by patching codegen template (single point of change) |
| Ergonomic regression for users hitting `body_map` at 5 params unnecessarily | low | LSP completion (Phase 5); convention says positional wins under 10 |

---

## 5. Implementation tickets (to spawn)

Each is sized 1-3 SP per the standing convention.

1. **`vibelang-rhai/synthdef-body-map-api`** (3 SP)
   Add `.body_map(closure)` to the synthdef builder in `api.rs`/`builder.rs`. Mirror in FX (`build_effect_closure`) and modulator (`build_modulator_closure`) builders. Param-list → Rhai `Map<String, Dynamic>` construction. Unit tests at arities 1/5/11/20/50.

2. **`vibelang-dsp/codegen-raw-fn-fallback`** (3 SP)
   Branch in `build.rs:323-402`: emit `register_raw_fn` for UGen functions whose input count exceeds 20. Generate manual `FnCallArgs` unpacking template. Test fixture: synthetic 24-input UGen. Add OteyPiano manifest entry.

3. **`vibelang-std/spectraphon-dual-restore-folded-params`** (2 SP)
   Migrate `spectraphon_dual.vibe` and `spectraphon_side.vibe` to `body_map`. Restore `mode_a`/`mode_b`, `array_idx_a`/`array_idx_b`, per-side `partials`/`slide`/`focus`. Update preset banks if any reference positional args.

4. **`vibelang-lsp/body-map-param-completion`** (2 SP)
   Inside a `body_map` closure body, completion on `p.` offers the enclosing synthdef's declared params. Reuse existing per-synthdef param table.

5. **`vibelang-dsp/body-map-stdlib-convention-lint`** (1 SP)
   CI lint: positional `.body` with >10 params is a hard error (already today); >6 emits a deprecation-style warning suggesting `body_map`. Document the convention in `kb/synthdef-authoring-conventions.md`.

6. **`vibelang-std/otey-piano-ugen`** (3 SP)
   Land the OteyPiano (24-input) UGen against the new raw-fn fallback path. Proves the codegen branch end-to-end.

7. **`vibelang-std/maths-marbles-recreation`** (5 SP, story → split into tasks)
   Mutable Instruments Maths and Marbles Eurorack module recreations as synthdefs against `body_map`. Proves the synthdef path end-to-end on real hardware-modeled modules.

8. **`vibelang-dsp/reload-bodymap-regression-test`** (1 SP)
   Hot-reload regression test: change a `body_map` synthdef's param values, assert reconciler emits parameter-update patches not a full rebuild.

---

## 6. Summary

| Question | Answer |
|---|---|
| Where does the 10-cap come from? | `builder.rs:379-493` exhaustive `match`, bounded by Rhai's `IntoFuncArgs` 10-tuple impls |
| Where does the 20-cap come from? | `rhai-1.23.6/src/func/register.rs:245` `def_register!(A:20, ..., V:1)` |
| Is there a rhai-side escape hatch for register_fn? | Yes: `register_raw_fn` — slice-typed, no arity limit |
| Is there a rhai-side escape hatch for body closures? | Yes: pass a single `Map` arg (1-tuple is supported) |
| Should we fork rhai? | No |
| Recommended combo | **A + C-hybrid**: Map body for synthdefs, `register_raw_fn` codegen branch for UGens >20 inputs |
| Net ceiling after fix | Effectively unbounded for both paths |
| BC story | Fully additive; no migration required |
