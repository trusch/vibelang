---
title: Group API
id: group-api
status: open
tags:
- reference
labels:
  area: api
  topic: group
created: 2026-03-11T08:36:06.401468223+01:00
updated: 2026-05-08T21:17:32+02:00
---

# Group API

## Functions

- `define_group("Name", || { ... })` — create a group with a closure
- `group("Name")` → Group — create or retrieve a group handle
- `get_group("Name")` → Group — retrieve existing group

## Group Methods

- `.body(|| { ... })` — evaluate content in this group
- `.alias("alternative")` — register an alternate group name
- `.gain(db(n))` — set group volume
- `.mute()` — mute entire group
- `.unmute()` — unmute
- `.solo(bool)` — solo this group

## Example

```rhai
define_group("Drums", || {
    // voices, patterns, effects go here
    fx("verb").synth("reverb").param("room", 0.2).apply();
});

get_group("Drums").gain(db(-3));
```

## Repeated Bodies

Calling `.body(|| { ... })` more than once for the same canonical group appends
another body contribution to that group. This is useful when one file owns the
playable structure, another file imports input voices or fills, and another file
adds shared effects. The bodies merge into one group, so later files can extend a
bus without replacing the voices, patterns, melodies, or effects that earlier
bodies registered.

Body calls are applied in total script evaluation order, including imported
files. For example, in
`examples/multi_body_authoring/main.vibe`, the main file creates the `Drums`
voices and patterns, then imports `examples/multi_body_authoring/drum_fx.vibe`;
the imported file calls `group("Drums").body(...)` again to add the room reverb
to the same group.

Content inside merged bodies follows the normal builder conflict rules:

- New voices, patterns, melodies, effects, and routes are added in first-seen
  order across all bodies.
- Re-declaring the same voice or group parameter keeps the original order slot
  but uses the later configuration, so later bodies can deliberately override
  gain, synth, route destination, or other replacement-style settings.
- Shared group effects append to the group chain in evaluation order.
- Authoring errors still abort evaluation. A conflicting alias, invalid alias
  name, or route conflict is not made safe by putting the call in a separate
  body.

## Group Aliases

Group aliases are author-facing names for existing groups. The syntax is
`group("canonical").alias("alternative")`; later `group("alternative")`
returns the canonical group handle instead of creating another group.

Aliases are resolved before contextual group creation. This matters inside group
bodies and included files: if `drums` is an alias for `main/Arrangement/Drums`,
then `group("drums")` resolves to `main/Arrangement/Drums` everywhere, even from
inside another group body where `group("drums")` would otherwise create a nested
contextual group.

Aliases live in one global script namespace. Use short, lower-case
single-segment aliases such as `drums`, `kit`, or `send`; do not use aliases that
look like paths. Aliases map directly to the canonical group target, so chaining
through an alias is normalized:

```rhai
group("main/Arrangement/Drums").alias("drums");
group("drums").alias("kit"); // kit also points to main/Arrangement/Drums
```

Alias targets and repeated bodies compose: `group("kit").body(...)` appends to
`main/Arrangement/Drums` after `kit` is registered, even when the call appears in
another file or inside a different current group body.

### Multi-File Example

```rhai
// groups.vibe
let drums = group("main/Arrangement/Drums")
    .alias("drums")
    .alias("kit")
    .gain(db(-3));
```

```rhai
// parts/kick.vibe
// After groups.vibe has been included/evaluated, this resolves to
// main/Arrangement/Drums instead of creating parts/kick's contextual "kit".
voice("kick")
    .synth("kick_808")
    .group("kit");

group("kit").effect("reverb_jpverb");
```

### Conflict Rules

Registering the same alias for the same canonical group is idempotent.
Registering the same alias for a different group is a hard authoring error.
Aliases must be single relative names: `main`, names containing `/`, and
absolute-looking names such as `main/Drums` are invalid.

Aliases also claim their raw token for the whole evaluation. If a script has
already used `group("kit")` contextually to create `main/Song/kit`, then a later
`group("main/Drums").alias("kit")` must fail unless it points to that same
canonical group. Declaring the alias first makes later `group("kit")` references
resolve to the alias target instead of creating contextual groups.
