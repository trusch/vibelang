---
title: CLI Commands
id: cli-commands
status: open
tags:
- reference
labels:
  area: workflow
  topic: cli
created: 2026-03-11T08:36:07.857370914+01:00
updated: 2026-03-11T08:36:07.857370914+01:00
---

# CLI Commands

## Installation

```bash
cargo install vibelang-cli
```

## Commands

```bash
vibe my_song.vibe                  # play with hot reload
vibe run my_song.vibe -w           # explicit watch mode
vibe my_song.vibe -I ../lib        # additional import search path
vibe --help                        # show help
```

## Prerequisites

- SuperCollider installed and running
- JACK Audio active (Linux/Mac)
- Rust toolchain (for cargo install)
