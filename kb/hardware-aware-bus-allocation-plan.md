# Hardware-Aware Bus Allocation — Plan

**Status:** in flight
**Epic:** `epic-hardware-aware-bus-allocation`
**Tracking issue:** drums silent on `-i 8 -o 10` setups; root cause is `AudioBusAllocator::default() = new(16)` colliding with hardware input bus 16 (channel 6)

## 1. Problem

scsynth lays out audio buses as a single contiguous index space. With `-o N -i M`:

- `[0 .. N)` — hardware output buses
- `[N .. N+M)` — hardware input buses
- `[N+M .. )` — private buses (zeroed each block)

`sound_in_channel(c)` correctly resolves to `NumOutputBuses.ir + c` at synthdef compile time, so it always reads the right hardware input bus.

But the runtime's `AudioBusAllocator::default()` hardcodes `start = 16`. Whenever `N + M > 16`, the runtime hands out user buses inside the hardware input range. A group's mix bus aliases a hardware input, and writes from the group's link synth clobber the hardware capture data — or vice versa, depending on synth ordering.

For the studio setup on `buddy`: `-i 8 -o 10` puts the hardware input boundary at bus 18. The first allocated user bus lands on 16, which is hardware input #7 (Line12 = drums input). Whichever root group wins the `HashMap`-iteration coin flip in `order_group_creations` ends up at bus 16 — which is why the regression appeared intermittently.

Bumping the constant to 32 just moves the boundary; it breaks again on `-o 16 -i 16` or `-o 24` ADAT setups.

## 2. Design principles

1. **Single source of truth: scsynth's actual `-i M -o N`.** The CLI knows it (it spawned scsynth); the runtime should consume it through a constructor, not guess.
2. **Make the unsafe path impossible to use accidentally.** `Default` impl is a footgun — replace with required configuration.
3. **Verify against scsynth.** When `--no-boot` connects to an externally-started server, query `/status` to confirm the layout matches.
4. **Determinism.** Two reloads of the same script must produce identical bus assignments. No `HashMap` iteration-order dependence.

## 3. Story breakdown

```
Story 1 (plumb config) ──┬─► Story 2 (kill Default footgun)
                         ├─► Story 3 (verify against /status)
                         └─► Story 4 (deterministic root ordering)
                                              │
Story 1, 4 ─────────────────────────────► Story 6 (integration tests)
Story 5 (parallel allocator audit) ────────────────► Story 6
Story 1, 2, 3, 4 ───────────────────► Story 7 (docs + REVIEW_TODO)
```

### Story 1 — Plumb hardware bus config from CLI to allocator

**Goal:** runtime knows scsynth's `-i M -o N` and starts user-bus allocation at exactly `M + N`.

**Tasks:**

- 1a. Add `State::with_audio_config(output_channels: u32, input_channels: u32) -> Self` that builds a default state then overrides `audio_buses = AudioBusAllocator::new(output_channels + input_channels)`.
- 1b. Add `Runtime::new_with_audio_config(backend, output_channels, input_channels)` that calls into a private `new_with_state(backend, state)` (existing `Runtime::new` becomes a wrapper that passes `State::default()`).
- 1c. CLI: `run_simple_mode` and `run_tui_mode` already have `output_channels`/`input_channels` in scope — switch their `Runtime::new(...)` call sites to `Runtime::new_with_audio_config(...)`.
- 1d. Audit other `Runtime::new` callers in the workspace (HTTP eval, websocket, tests). Tests can keep `Runtime::new` (zero hardware buses is fine for mock backends).

**Acceptance:**

- Running `vibe run --input-channels 8 --output-channels 10 main.vibe` allocates first user bus at 18, not 16.
- Existing tests pass.
- Mock backend tests can still construct a `Runtime` without specifying audio config.

**Owner:** worker A.

### Story 2 — Kill `AudioBusAllocator::default()` footgun

**Goal:** the only way to construct an `AudioBusAllocator` is to specify the start bus explicitly. No guessing constants in the codebase.

**Tasks:**

- 2a. Remove `impl Default for AudioBusAllocator`.
- 2b. Update `State::default()` to allocate `audio_buses: AudioBusAllocator::new(0)` *or* mark `audio_buses` as `Option<AudioBusAllocator>` and force callers through `with_audio_config`. Recommend the latter: `State` without an audio config is logically incomplete.
- 2c. Same for `BusAllocator::default()` in `crates/vibelang-core/src/reload/bus_pool.rs` (Story 5 covers this in more detail).
- 2d. All test sites currently relying on `AudioBusAllocator::new(16)` keep working — they're already explicit.

**Acceptance:**

- `cargo build -p vibelang-core` fails on any code path that tries to allocate audio buses without configuring the start.
- No `AudioBusAllocator::default()` in the codebase.

**Owner:** worker B (after Story 1 lands).

### Story 3 — Verify against scsynth `/status`

**Goal:** when the runtime connects to scsynth (boot or `--no-boot`), it queries `/status` and confirms the layout matches the configured `output_channels + input_channels`. Mismatch is a hard error at startup, not a silent collision at runtime.

**Tasks:**

- 3a. Add `ScsynthBackend::query_status() -> StatusReply` (sends `/status`, awaits `/status.reply`). Reply fields needed: `numOutputBusChannels`, `numInputBusChannels`. Response format: `/status.reply 1 numUgens numSynths numGroups numSynthDefs avgCpu peakCpu sampleRate actualSampleRate numOutputBusChannels numInputBusChannels` per [SC docs](https://doc.sccode.org/Reference/Server-Command-Reference.html#/status). Note: the historical `/status.reply` order varies by sclang version — verify against `crates/vibelang-core/src/backends/scsynth.rs` if any parsing already exists.
- 3b. After `Runtime::new_with_audio_config`, call `query_status()` and compare. Mismatch → log error and bail.
- 3c. Optional: expose status fields via the HTTP API for diagnostic purposes (`/api/status` or similar).

**Acceptance:**

- Starting `vibe run --no-boot --output-channels 4 --input-channels 4` against an externally-started `scsynth -o 8 -i 8` fails at startup with a clear message.
- Normal CLI-spawned scsynth flow shows no warning (matches by construction).

**Owner:** worker C (after Story 1).

### Story 4 — Deterministic root-group allocation order

**Goal:** two reloads of the same script produce identical bus assignments. No `HashMap`-iteration-order dependence on bus IDs.

**Tasks:**

- 4a. `crates/vibelang-core/src/reload/mod.rs::order_group_creations` — inside the `while !remaining.is_empty()` loop, sort `batch` deterministically before extending `ordered`. Recommended sort: by `GroupId::raw()`. Already-stable IDs (FNV-1a hash of full path) make this consistent across processes.
- 4b. Apply the same determinism to other `HashMap`-iteration sites that affect ordering of bus/node allocation: scan `apply_reload` for `for ... in diff.X` patterns where order matters and add a sort.
- 4c. Add a regression test: build a `ScriptState` with three root groups, run `order_group_creations` 100 times in a process, assert the same order each time. (Process-local determinism is enough; cross-process determinism is implied by sorting on stable IDs.)

**Acceptance:**

- `cargo test -p vibelang-core order_group_creations -- --test-threads=1` passes.
- Manual: launching the studio script twice in a row reports the same `audio_bus` for `master` both times in `/groups`.

**Owner:** worker D.

### Story 5 — Audit and fix `BusAllocator::default()` parallel footgun

**Goal:** the second allocator type at `crates/vibelang-core/src/reload/bus_pool.rs` follows the same hardware-aware policy, or is removed if it has no production callers.

**Tasks:**

- 5a. Grep for non-test uses of `BusAllocator::new(...)` and `BusAllocator::default()` across the workspace.
- 5b. If unused outside tests, delete the `Default` impl (least invasive).
- 5c. If used in production, route its construction through the same audio config and remove its hardcoded `Default`.

**Acceptance:**

- No remaining hardcoded `start = 16` constants in either allocator outside tests.
- Tests still build and pass.

**Owner:** worker E (parallel with Story 1, no dependency).

### Story 6 — Integration tests / regression coverage

**Goal:** the studio-setup-style configuration (`-i 8 -o 10`, master + es-3 + many subgroups) is exercised in CI, and a future regression to `start = 16` would fail tests.

**Tasks:**

- 6a. New test in `crates/vibelang-core/tests/`: build a `Runtime::new_with_audio_config(mock_backend, 10, 8)` with mock backend, apply a `ScriptState` with two root groups (`master` + `es-3`), assert no allocated `audio_bus` falls within `[0, 18)`.
- 6b. Test: same script applied twice produces identical group-to-bus assignments.
- 6c. Test: `Runtime::new(mock_backend)` (no config) — assert that calling `state.alloc_audio_bus(2)` panics or errors meaningfully. Locks in the Story 2 footgun fix.

**Acceptance:**

- `cargo test -p vibelang-core hardware_bus_allocation` — three new tests pass.

**Owner:** worker F (after Stories 1, 2, 4 land).

### Story 7 — Documentation + tracker update

**Goal:** the bus model is documented, the new constructor is in the public API docs, and `REVIEW_TODO.md` reflects what changed.

**Tasks:**

- 7a. Update `crates/vibelang-core/src/lib.rs` module-level docs (the bus-routing section already exists) — add a paragraph on hardware vs private bus boundaries and link to the constructor.
- 7b. Update project `CLAUDE.md` "Key Architecture" or "Audio routing" section.
- 7c. Add an entry in `REVIEW_TODO.md` under "Resolved" with the CR-XX number.
- 7d. Add a one-line note in `RELEASE_NOTES.md` under the next version.

**Acceptance:**

- Docs explain that the runtime trusts `output_channels + input_channels` from the CLI.
- `cargo doc -p vibelang-core` builds without warnings on the touched modules.

**Owner:** worker G (last, after everything else).

## 4. Out of scope (future work)

- **Dynamic reconfig.** Changing `-i` or `-o` at runtime is not supported. Restart the runtime.
- **Sub-allocator partitioning.** No need for separate bus pools for groups vs voices vs FX — they all share `audio_buses` and that's fine.
- **Bus number stability across script edits.** `BusAllocator` (the affinity allocator) already handles this; we're not touching its semantics.
- **Cross-version `/status` reply field-order quirks.** Document the parsing assumption in the Story 3 task; revisit if it bites.

## 5. Sequencing

```
Wave 1 (parallel):  Story 1, Story 4, Story 5
Wave 2 (parallel):  Story 2 (after 1), Story 3 (after 1)
Wave 3:             Story 6 (after 1, 2, 4)
Wave 4:             Story 7 (after all)
```

A worker per story; each lands as a focused commit.
