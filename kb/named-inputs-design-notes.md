# Named Synthdef Inputs - Design Notes

Status: pinned by [P9: Design pinning - defaults, naming, cycle policy](#p9-design-pinning-defaults-naming-cycle-policy).

Wave-1 implementation status: Rhai authoring docs are published in
`kb/tickets/api-reference/custom-synthdef-api/README.md` and
`kb/tickets/api-reference/voice-api/README.md`. Parent-group `in` autofeed is
implemented in sibling ticket
`task-implement-parent-group-in-autofeed-for-declared-in-input`; see
`runtime.rs::effective_input_routes`. Input hot-reload reconciliation is
implemented in sibling ticket
`task-reconcile-synthdef-input-port-hot-reload-routes`. The Wave-1 public API
references document `.from(...)` and `.disconnect()`; fan-in and hardware
input source verbs remain design items until their implementation tickets
land.

These decisions define the named-input surface before the implementation
stories land. Named inputs mirror named outputs structurally, but source
selection has a few deliberately different defaults.

## Decisions

### 1. `.from(x)` Replaces, Fan-In Is Explicit

Decision: `voice.input("name").from(x)` is replace-by-default. A later
`.from(y)` on the same input replaces the previous source. Additive
fan-in is available only through explicit verbs:
`voice.input("name").from_all([a, b])` for a full source set, and
`voice.input("name").add_from(x)` for incremental fan-in. Those fan-in verbs
are deferred from the Wave-1 API reference; this section pins the intended
shape so later implementation does not make `.from(...)` additive by default.

Rationale: most named inputs represent a single patch cable or a single
logical source, so replacement makes edits predictable and avoids stale
routes after reloads. Fan-in is useful for mixer-like patches, but it is
the rarer and more expensive case because [P3: Input routing dispatcher in core](#p3-input-routing-dispatcher-in-core)
must allocate mixer/link synths for it. This intentionally differs from
output `voice.output("name").to(group)`, which is additive for group
destinations. The asymmetry keeps output multing convenient while making
input source ownership explicit in the [P2: Rhai routing surface for inputs](#p2-rhai-routing-surface-for-inputs).

### 2. Unconnected Inputs Read Silent Buses

Decision: every declared input port has a valid bus even when the script
does not route anything into it. Unconnected ar inputs read from one
shared silent audio bus; unconnected kr inputs read from one shared
silent control bus. Both silent buses are allocated at startup. Creating
a voice with unconnected inputs does not error, and unconnected inputs do
not implicitly route to any group, hardware input, parent bus, or output
port except for the `in` autofeed rule in Decision 3.

Rationale: silence is the only safe default for an absent patch cable.
[P1: Synthdef-side input declaration + manifest + bus allocation](#p1-synthdef-side-input-declaration-manifest-bus-allocation)
can guarantee that each `<port>_bus` parameter is always meaningful,
while [P3: Input routing dispatcher in core](#p3-input-routing-dispatcher-in-core)
only has to install route synths for explicit edges. This avoids
create-time failures during partial patch construction and keeps reloads
predictable when an input route is removed.

### 3. Legacy No-Input Effects Keep `in` Param Wiring

Decision: a synthdef with no declared `.input(...)` ports keeps the
current effect-chain convention: the runtime wires its `in` param to the
parent group's pre-fader bus. Once a synthdef declares any `.input(...)`
port, named-input semantics take over. In that mode, a stereo audio input
port literally named `in` auto-wires to the parent group's stereo
pre-fader bus by default, but the script may override it with
`.from(...)` or silence it with `.disconnect()`. A mono `in` against the
stereo parent group stays silent and warns; the runtime does not silently
downmix. All other input names start unconnected and require explicit
routing.

Rationale: existing effect synthdefs and user scripts must keep working
without source changes, which is the hard compatibility gate in
[P5: Legacy effect-chain compatibility](#p5-legacy-effect-chain-compatibility).
The opt-in rule also gives [P1: Synthdef-side input declaration + manifest + bus allocation](#p1-synthdef-side-input-declaration-manifest-bus-allocation)
a clean boundary: no declared inputs means legacy effect behavior, while
any declared input means the manifest owns the input contract.

Public authoring references: declare `.input(name)` /
`.input(name, channels)` and read `p.inputs.<name>` in
`kb/tickets/api-reference/custom-synthdef-api/README.md`; patch routes with
`target.input("name").from(source)` and `.disconnect()` in
`kb/tickets/api-reference/voice-api/README.md`.

### 4. Cycles Are Allowed

Decision: named-input routing does not perform graph cycle detection.
Cycles are allowed, including voice-to-voice or group-mediated feedback
paths. scsynth bus reads naturally observe the previous control/audio
block when a cycle is present, so the effective feedback path includes
the normal one-block delay.

Rationale: modular patches commonly use feedback, and rejecting cycles
would make the routing graph less expressive than the underlying audio
server. Letting [P3: Input routing dispatcher in core](#p3-input-routing-dispatcher-in-core)
emit the requested bus links directly keeps implementation simple and
matches scsynth behavior. Authors who need zero-delay feedback still
need a dedicated synthdef-internal feedback design; named-input routing
is bus-level patching, not an intra-synth sample-delay solver.

### 5. Hardware Output Stays On Groups

Decision: hardware output placement remains only on
`group("g").output(N)` or `group("g").output([left, right])`. Do not add
`.to_hardware(...)` to output ports. Hardware input sources are allowed
on input handles, for example `voice.input("ext").from_hardware(N)`,
because that only resolves a bus reference to feed an input route. The
hardware input source verb is not part of the Wave-1 authoring docs yet.

Rationale: group-level hardware output is already the routing target
concept in the output system, and duplicating it on output ports would
create competing ways to place final hardware edges. Keeping hardware
outputs on groups preserves the existing model from the multi-output
surface while [P6: Hardware input as first-class routing source](#p6-hardware-input-as-first-class-routing-source)
adds the opposite direction: hardware input as a source handle. The
dispatcher work in [P3: Input routing dispatcher in core](#p3-input-routing-dispatcher-in-core)
can then treat hardware inputs as just another source bus, not as a
terminal routing destination.

## P-Ticket Index

<a id="p1-synthdef-side-input-declaration-manifest-bus-allocation"></a>

### P1: Synthdef-side input declaration + manifest + bus allocation

Slug: `p1-synthdef-side-input-declaration-manifest-bus-allocation`.

Adds input ports to the synthdef builder and manifest, allocates per-port
input buses at voice creation, and passes them as `<port>_bus` synth
params.

<a id="p2-rhai-routing-surface-for-inputs"></a>

### P2: Rhai routing surface for inputs

Slug: `p2-rhai-routing-surface-for-inputs`.

Exposes input handles and source-selection verbs in the Rhai API.

<a id="p3-input-routing-dispatcher-in-core"></a>

### P3: Input routing dispatcher in core

Slug: `p3-input-routing-dispatcher-in-core`.

Translates input route maps into scsynth routing and mixer synths.

<a id="p5-legacy-effect-chain-compatibility"></a>

### P5: Legacy effect-chain compatibility

Slug: `p5-legacy-effect-chain-compatibility`.

Ensures existing effect synthdefs and examples keep their legacy `in`
param behavior.

<a id="p6-hardware-input-as-first-class-routing-source"></a>

### P6: Hardware input as first-class routing source

Slug: `p6-hardware-input-as-first-class-routing-source`.

Promotes hardware inputs to route-source handles for named inputs.

<a id="p9-design-pinning-defaults-naming-cycle-policy"></a>

### P9: Design pinning - defaults, naming, cycle policy

Slug: `p9-design-pinning-defaults-naming-cycle-policy`.

Pins these default, naming, cycle, and hardware-placement decisions
before the implementation stories depend on them.
