---
title: Array Functions
id: array-functions
status: open
tags:
- reference
labels:
  area: api
  topic: helpers
created: 2026-03-11T08:36:06.772002617+01:00
updated: 2026-03-11T08:36:06.772002617+01:00
---

# Array Functions

## Functions

- `array_zip(a, b)` → Array — interleave two arrays
- `array_shuffle(arr)` → Array — randomly shuffle
- `array_rotate(arr, n)` → Array — rotate by n positions
- `array_reverse(arr)` → Array — reverse order
- `array_flatten(arr)` → Array — flatten nested arrays
- `array_repeat(arr, n)` → Array — repeat n times
- `array_take(arr, n)` → Array — first n elements
- `array_skip(arr, n)` → Array — skip first n elements

## Example

```rhai
let notes = [60, 64, 67];
let doubled = array_repeat(notes, 2);  // [60, 64, 67, 60, 64, 67]
let reversed = array_reverse(notes);   // [67, 64, 60]
```
