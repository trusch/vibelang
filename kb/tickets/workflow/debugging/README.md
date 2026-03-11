---
title: Debugging
id: debugging
status: open
tags:
- reference
labels:
  area: workflow
  topic: debug
created: 2026-03-11T08:36:07.977766812+01:00
updated: 2026-03-11T08:36:07.977766812+01:00
---

# Debugging

## Log Levels

```bash
RUST_LOG=debug vibe my_song.vibe    # verbose logs
RUST_LOG=info vibe my_song.vibe     # info level
```

## Running Examples

```bash
cd examples/sfz
../../target/release/vibelang run -w -I ../.. example.vibe
```

## Building

Always wrap cargo commands:

```bash
bash -c "cargo build --release"
bash -c "cargo test"
```
