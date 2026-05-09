---
title: Group
id: group
status: open
tags:
- concept
labels:
  area: core
  topic: group
created: 2026-03-11T08:35:45.143708354+01:00
updated: 2026-05-08T21:17:32+02:00
---

# Group

A **group** bundles voices, patterns, and melodies into a logical unit — like a bus/submix in a DAW. Groups enable shared effects and volume control.

## Creation

```rhai
define_group("Drums", || {
    let kick = voice("kick").synth("kick_808").gain(db(-6));
    let snare = voice("snare").synth("snare_808").gain(db(-8));

    pattern("kick").on(kick).step("x... x... x... x...").start();
    pattern("snare").on(snare).step(".... x... .... x...").start();

    // Apply effects to the entire group bus
    fx("drum_reverb").synth("reverb").param("room", 0.2).param("mix", 0.15).apply();
});
```

## Repeated Bodies

Groups can be extended with repeated `.body(|| { ... })` calls. Each call
contributes more content to the same canonical group instead of replacing the
previous body. This lets an arrangement file own the main structure, an imported
file add input voices or fills, and another imported file add shared group
effects while all content still lands on one bus.

Bodies merge in total script evaluation order, including imports. Earlier bodies
provide the first order slot for their voices, routes, patterns, melodies, and
effects. Later bodies append new content. When a later body re-declares a
replacement-style setting, such as group gain or a voice with the same name, the
later configuration wins while the original ordering remains stable. Group
effects append in evaluation order. Conflicts that are already hard authoring
errors, such as incompatible aliases or route conflicts, still fail even when the
calls are split across bodies.

Use this style when the musical responsibilities are separate:

```rhai
// main.vibe
group("Drums").gain(db(-8)).body(|| {
    let kick = voice("kick").synth("kick_909");
    pattern("kick").on(kick).step("x... x... x... x...").start();
});

import "./drum_fx.vibe";
```

```rhai
// drum_fx.vibe
group("Drums").body(|| {
    fx("drum_room").synth("reverb").param("mix", 0.12).apply();
});
```

See `examples/multi_body_authoring/main.vibe` and
`examples/multi_body_authoring/drum_fx.vibe` for a runnable cross-file example.

## Group Controls

```rhai
let drums = get_group("Drums");
drums.gain(db(-3));      // adjust group volume
drums.mute();            // mute entire group
drums.unmute();
drums.solo(true);        // solo (mute all others)
```

## Audio Routing

- Bus 0: main audio output (JACK/hardware)
- Buses 16+: group buses for submixing
- Link synths route group buses to main output automatically

## Aliases

Group aliases are convenient authoring names for canonical groups. After a group
registers `.alias("alternative")`, later `group("alternative")` calls and
explicit group string targets resolve to the canonical group instead of creating
a second group.

```rhai
// groups.vibe
group("main/Arrangement/Drums").alias("drums").alias("kit");
```

```rhai
// later, from another file or inside another group body
voice("kick").synth("kick_808").group("kit");
group("kit").gain(db(-3));
```

Alias lookup happens before contextual group creation. In the example above,
`kit` resolves to `main/Arrangement/Drums` everywhere after the alias is
registered; it does not become `main/Song/kit` just because the reference appears
inside a `Song` group body.

Aliases work with repeated bodies. Once an alias exists, a later
`group("kit").body(...)` appends to the canonical group, even across imports or
from inside a different current group body.

Aliases are global for a full script evaluation, including includes. Duplicate
aliases to the same group are allowed and treated as idempotent. Duplicate
aliases to different groups are errors. Aliases must be short single relative
names, not `main`, not absolute paths, and not names containing `/`. If a raw
name was already used contextually to create a group, a later alias with that raw
name must either point to that exact group or fail, so reloads cannot silently
retarget earlier references.

## Naming Convention

Capitalized: `"Drums"`, `"Bass"`, `"Synth"`, `"Pad"`

Prefer capitalized canonical group names for structure and lower-case aliases
for cross-file references, for example `main/Arrangement/Drums` with alias
`drums`.
