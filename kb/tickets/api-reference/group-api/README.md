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

- `.alias("alternative")` — planned: register an alternate group name
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

## Planned: Group Aliases

Group aliases are planned author-facing names for existing groups. The syntax is
`group("canonical").alias("alternative")`; later `group("alternative")`
returns the canonical group handle instead of creating another group.

Aliases are resolved before contextual group creation. This matters inside group
bodies and included files: if `drums` is an alias for `main/Arrangement/Drums`,
then `group("drums")` resolves to `main/Arrangement/Drums` everywhere, even from
inside another group body where `group("drums")` would otherwise create a nested
contextual group.

Planned group aliases live in one global script namespace. Use short, lower-case
single-segment aliases such as `drums`, `kit`, or `send`; do not use aliases that
look like paths. Aliases map directly to the canonical group target, so chaining
through an alias is normalized:

```rhai
group("main/Arrangement/Drums").alias("drums");
group("drums").alias("kit"); // kit also points to main/Arrangement/Drums
```

### Planned Multi-File Example

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

### Planned Conflict Rules

Registering the same alias for the same canonical group is idempotent.
Registering the same alias for a different group is a hard authoring error.
Aliases must be single relative names: `main`, names containing `/`, and
absolute-looking names such as `main/Drums` are invalid.

Aliases also claim their raw token for the whole evaluation. If a script has
already used `group("kit")` contextually to create `main/Song/kit`, then a later
`group("main/Drums").alias("kit")` must fail unless it points to that same
canonical group. Declaring the alias first makes later `group("kit")` references
resolve to the alias target instead of creating contextual groups.
