# VibeLang Emacs Mode

Major mode for editing `.vibe` files with live runtime integration, beat-position visualization, LSP support, and a project sidebar.

## Installation

Add the `emacs/` directory to your load path and require the mode:

```elisp
(add-to-list 'load-path "/path/to/vibelang/emacs")
(require 'vibelang-mode)

;; Auto-activate for .vibe files
(add-to-list 'auto-mode-alist '("\\.vibe\\'" . vibelang-mode))
```

### Optional: LSP (eglot, Emacs 29+)

LSP is enabled automatically when `vibelang-enable-lsp` is non-nil (the default). You need the `vibelang` binary on your PATH:

```elisp
;; To use lsp-mode instead of eglot:
(setq vibelang-lsp-use-eglot nil)
```

### Optional: Templates (tempel or yasnippet)

Add this to your mode hook to activate snippet expansion:

```elisp
(add-hook 'vibelang-mode-hook #'vibelang-setup-templates)
```

Tempel is preferred; yasnippet is used as a fallback if tempel is not available.

---

## Quick Start

1. Start the runtime: `vibe run mytrack.vibe` (or `C-c r s` inside Emacs)
2. Open `mytrack.vibe` — the mode connects automatically if the runtime is running
3. Edit and save — changes hot-reload into the running runtime
4. Press `C-c C-s` to start playback; `C-c C-x` to stop
5. Press `C-c C-h` for the full in-Emacs help reference

---

## Key Bindings

### Connection & Runtime

| Key       | Command                        |
|-----------|-------------------------------|
| `C-c C-c` | Connect to runtime             |
| `C-c C-d` | Disconnect                     |
| `C-c r s` | Start runtime (`vibe run`)     |
| `C-c r k` | Kill runtime                   |
| `C-c r r` | Restart runtime                |

### Transport

| Key       | Command                        |
|-----------|-------------------------------|
| `C-c C-s` | Start playback                 |
| `C-c C-x` | Stop playback                  |
| `C-c C-t` | Tap tempo                      |
| `C-c +`   | BPM +1                         |
| `C-c -`   | BPM -1                         |
| `C-c C-q` | Set BPM (prompt)               |

### Eval & Reload

| Key       | Command                                         |
|-----------|-------------------------------------------------|
| `C-c C-e` | Eval dwim (region → entity at point → buffer)   |
| `C-c e b` | Eval buffer                                     |
| `C-c e r` | Eval region                                     |
| `C-c e l` | Eval line                                       |
| `C-c e p` | Eval expression (prompt)                        |
| `C-c C-r` | Reload script from disk                         |

### Entity Commands

| Key       | Command                        |
|-----------|-------------------------------|
| `C-c m`   | Mute entity at point           |
| `C-c s`   | Solo entity at point           |
| `C-c C-i` | Inspect entity (live buffer)   |
| `C-c C-p` | Edit numeric param at point    |
| `C-c n r` | Rename entity definition       |
| `C-c n c` | Clone entity definition        |
| `C-c n .` | Show entity name at point      |

### Navigation

| Key       | Command                        |
|-----------|-------------------------------|
| `M-n`     | Next entity definition         |
| `M-p`     | Previous entity definition     |
| `C-M-h`   | Mark (select) current entity   |
| `C-c /`   | Jump to entity by name (imenu) |

### Visualization & UI

| Key       | Command                        |
|-----------|-------------------------------|
| `C-c C-v` | Toggle beat-position overlays  |
| `C-c C-b` | Toggle sidebar                 |
| `C-c b o` | Open sidebar                   |
| `C-c b i` | Focus sidebar on entity at point |
| `C-c h t` | Toggle transport header line   |
| `C-c .`   | Open cockpit buffer            |

### LSP

| Key       | Command                        |
|-----------|-------------------------------|
| `C-c C-l` | Enable LSP for buffer          |
| `C-c l r` | Restart LSP server             |

### Help & Diagnostics

| Key        | Command                        |
|------------|-------------------------------|
| `C-c C-h`  | Open this help buffer          |
| `C-c C-?`  | Run connection diagnostics     |
| `C-c C-.`  | Open transient command menu    |

---

## Live Coding Workflow

A typical session:

1. **Start the runtime** — `C-c r s` (or run `vibe run track.vibe` in a terminal).
2. **Connect** — happens automatically if `vibelang-auto-connect` is `if-running` (the default). Otherwise `C-c C-c`.
3. **Enable LSP** — `C-c C-l` for completions, hover docs, and inline diagnostics. Or set `vibelang-enable-lsp t` to have it start automatically.
4. **Start playback** — `C-c C-s`.
5. **Edit** — beat-position indicators appear inside pattern and melody blocks as the runtime plays. Active clips in sequences are highlighted.
6. **Dial in tempo** — tap `C-c C-t` in time to the beat. Three or more taps average to a new BPM. Use `C-c +` / `C-c -` for fine adjustment.
7. **Edit params live** — place the cursor on a number inside a method call (e.g. `.volume(0.8)`) and press `C-c C-p`. Drag or type to adjust the value with instant feedback.
8. **Inspect** — `C-c C-i` opens a live buffer showing the current runtime state for the entity at point: parameters, meters, fade progress.
9. **Sidebar** — `C-c C-b` opens the project sidebar with the entity tree, live audio meters, clip progress, and an inspector that follows point.

---

## Templates

Expand these keys with tempel (`M-RET` or configured expansion key) or yasnippet (`TAB`):

| Key    | Expands to                                      |
|--------|-------------------------------------------------|
| `voi`  | `voice().synth("…").volume(1.0)` scaffold       |
| `grp`  | `group().add(voice_name)` scaffold              |
| `pat`  | `pattern("…")` with step string                 |
| `mel`  | `melody([…])` with notes array                  |
| `fx`   | `fx().synth("reverb")` scaffold                 |
| `loop` | `midi_device("…").looper().to(voice)` scaffold  |
| `cc`   | `midi_device("…").map_cc(14).to(…)` scaffold    |

---

## Configuration

All variables are in the `vibelang` customization group (`M-x customize-group RET vibelang`).

### Connection

| Variable                       | Default        | Description                                                        |
|-------------------------------|----------------|--------------------------------------------------------------------|
| `vibelang-ws-host`             | `"127.0.0.1"`  | WebSocket host (use IP, not `localhost`, to avoid IPv6 ambiguity)  |
| `vibelang-ws-port`             | `1606`         | WebSocket port                                                     |
| `vibelang-auto-connect`        | `if-running`   | Auto-connect on file open: `nil`, `t`, or `if-running`            |
| `vibelang-api-host`            | `nil`          | HTTP API host; `nil` reuses `vibelang-ws-host`                     |
| `vibelang-api-port`            | `nil`          | HTTP API port; `nil` reuses `vibelang-ws-port`                     |
| `vibelang-http-timeout`        | `2.0`          | Timeout (seconds) for HTTP API requests                            |
| `vibelang-ws-reconnect-delay`  | `1.0`          | Seconds between auto-reconnect attempts                            |
| `vibelang-ws-max-reconnect-attempts` | `10`    | Max reconnect attempts before giving up (`nil` = forever)          |
| `vibelang-ws-stale-threshold`  | `2.5`          | Seconds without a tick before the connection is marked stale       |

### Runtime

| Variable                       | Default        | Description                                         |
|-------------------------------|----------------|-----------------------------------------------------|
| `vibelang-executable`          | `"vibe"`       | Path or name of the VibeLang binary                 |
| `vibelang-runtime-args`        | `'("run")`     | Arguments prepended before the script file           |

### Editing

| Variable                       | Default  | Description                                            |
|-------------------------------|----------|--------------------------------------------------------|
| `vibelang-auto-reload-on-save` | `nil`    | Reload script on every save                            |
| `vibelang-eval-save-before-run`| `t`      | Save buffer before reload/eval commands                |
| `vibelang-indent-offset`       | `2`      | Spaces per indentation level                           |
| `vibelang-tap-tempo-timeout`   | `3.0`    | Seconds of inactivity before tap-tempo resets          |

### Visualization & UI

| Variable                           | Default  | Description                                           |
|-----------------------------------|----------|-------------------------------------------------------|
| `vibelang-visualization-enabled`   | `t`      | Show beat-position overlays by default                |
| `vibelang-eval-flash-duration`     | `0.15`   | Seconds the eval flash overlay stays visible          |
| `vibelang-enable-header-line`      | `t`      | Show transport header line in VibeLang buffers        |
| `vibelang-sidebar-on-connect`      | `nil`    | Auto-open sidebar when connecting                     |
| `vibelang-sidebar-width`           | `40`     | Sidebar window width in columns                       |
| `vibelang-sidebar-position`        | `left`   | Sidebar placement: `left` or `right`                  |
| `vibelang-sidebar-show-meters`     | `t`      | Show live audio level meters in sidebar               |
| `vibelang-sidebar-meter-width`     | `10`     | Width of the meter bar                                |
| `vibelang-sidebar-auto-inspect-point` | `t`  | Inspector follows entity name at point                |

### LSP

| Variable                       | Default               | Description                                     |
|-------------------------------|-----------------------|-------------------------------------------------|
| `vibelang-enable-lsp`          | `t`                   | Enable LSP when opening .vibe files             |
| `vibelang-lsp-server-command`  | `'("vibelang" "lsp")` | Command to start the LSP server                 |
| `vibelang-lsp-use-eglot`       | `t`                   | Use eglot (Emacs 29+); `nil` to use lsp-mode   |

---

## Architecture

The mode is split across several files, each with a focused responsibility:

| File                      | Responsibility                                             |
|---------------------------|------------------------------------------------------------|
| `vibelang-mode.el`        | Major mode, keybindings, font-lock, runtime control        |
| `vibelang-websocket.el`   | WebSocket client — receives live playback ticks from runtime; HTTP client for commands |
| `vibelang-visualization.el` | Beat-position and clip overlays; re-renders on every `playback.tick` |
| `vibelang-lsp.el`         | eglot/lsp-mode registration, semantic token faces, inlay hints |
| `vibelang-sidebar.el`     | Project entity tree, live meters, clip progress, inspector |
| `vibelang-transient.el`   | Transient command menu (`C-c C-.`)                         |
| `vibelang-templates.el`   | tempel/yasnippet template definitions                      |
| `vibelang-indent.el`      | Indentation (brace-depth, method-chain rules)              |

**Two channels to the runtime:**
- **WebSocket** (`ws-port`) — receives high-frequency `playback.tick` / `playback.bar` / transport events for visualization and header-line updates.
- **HTTP** (`api-port`, same port by default) — sends commands: eval, transport start/stop, mute/solo, BPM changes.

The LSP server runs as a separate process (`vibelang lsp`) and communicates over stdio, independent of the WebSocket.
