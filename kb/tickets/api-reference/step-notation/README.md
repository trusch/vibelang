---
title: Step Notation
id: step-notation
status: open
tags:
- reference
labels:
  area: api
  topic: notation
created: 2026-03-11T08:36:06.076785047+01:00
updated: 2026-03-11T08:36:06.076785047+01:00
---

# Step Notation

Step notation is a string format for defining rhythmic patterns.

## Characters

| Char | Meaning |
|------|---------|
| `x` | Trigger at full velocity |
| `.` | Rest (silence) |
| `1`–`9` | Trigger at velocity level (1=quiet, 9=loud) |
| space | Ignored (use for readability) |

## Examples

```
"x... x... x... x..."    — four-on-the-floor kick (16th notes)
".... x... .... x..."    — snare on beats 2 and 4
".x .x .x .x"           — offbeat hi-hats (8th notes)
"..3. x.3. ..3. x..."   — snare with ghost notes (velocity 3)
"x.x. x.x. x.x. x.xx"  — busy hi-hat pattern
```

## Resolution

Token count per bar (in 4/4 time):
- 4 tokens → quarter notes
- 8 tokens → eighth notes
- 16 tokens → sixteenth notes
- 32 tokens → thirty-second notes
