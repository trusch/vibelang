# LSP and editor integrations

Editor commands/settings consume the LSP, REST, and WebSocket contracts; they
are not `.vibe` functions.

## Language server

Start with `vibe lsp`. It uses stdio, identifies as `vibelang-lsp2`, and uses
full-document text synchronization. Source:
[`server.rs`](../../crates/vibelang-lsp/src/server.rs) and
[`lib.rs`](../../crates/vibelang-lsp/src/lib.rs).

### Advertised capability map

| Capability | Current contract / limitation |
|---|---|
| Lifecycle | initialize, initialized, shutdown; deprecated `root_uri` used, workspace folders/file operations unsupported |
| Document sync | open/change/close, full document |
| Completion | Triggers `.`, `"`, `'`, `/`, `(`, `,`; no resolve |
| Hover / signature | Hover; signature trigger and retrigger |
| Navigation | Go-to-definition, references, prepareRename+rename; definition resolves imports/variables only, rename/references are document-local |
| Symbols | Nested document symbols; workspace symbols search open DocumentStore entries only |
| Actions/hints | Code actions, inlay hints, folding ranges, document links |
| Formatting | Whole-document only; respects tab size/insert spaces; no range/on-type format |
| Diagnostics | Pull diagnostic identifier `vibelang`, inter-file dependency flag, no workspace diagnostic |
| Semantic tokens | Full tokens, no range; current legend mismatch described below |
| Not provided | Code lens, execute-command, workspace diagnostics, workspace folders/file operations |

On open/change, diagnostics combine syntax, semantic, lint, core validation, and
unknown synth/effect checks, then are deduplicated/cached/pushed. Pull
diagnostics rerun only syntax/semantic/lint, so push and pull can differ. Close
clears state. References treat quoted strings as entity names and identifiers
as variables in the one open document.

Initialization loads workspace/stdlib import paths, scans stdlib definitions,
and initializes UGen/validation caches. Completion, hover, signature, and
inlays currently combine static hand-maintained API data with manifests and
scanned synthdefs; that table is incomplete/stale relative to registration.

### Semantic token protocol defect

Advertised token types are:

```text
function variable string number keyword comment type parameter property method
synthdef voice pattern melody group note patternToken
```

with modifiers `declaration`, `definition`, `readonly`.

The generator instead emits indices for a different 15-type legend:
`namespace,type,class,function,method,property,variable,string,number,keyword,
operator,comment,macro,parameter,enumMember`, and can set modifier bit 5 for
`defaultLibrary`. Tokens are therefore misclassified or out of legend. This is
a current protocol bug, not a client interpretation issue.

Static metadata also contains fictional/stale calls such as string-valued
`set_quantization`, two-argument `note`, `load_sample`, `at_bar`, `every`,
`after`, `fade_in`, `fade_out`, `midi_out`, `midi_in`, and `export_audio`, while
omitting many actual builders, helpers, properties, MIDI calls, and UGens.

## VS Code extension

Language ID is `vibe`, file extension `.vibe`. Contributed views are Explorer
`vibelang.sessionExplorer` and panel webview `vibelang.mixerView`. Source:
[`package.json`](../../vscode-extension/package.json) and
[`src/extension.ts`](../../vscode-extension/src/extension.ts).

### Commands

| Group | Exact command IDs |
|---|---|
| LSP/connection | `vibelang.restartLsp`, `vibelang.toggleConnection`, `vibelang.configureConnection`, `vibelang.refreshSession` |
| Transport | `vibelang.toggleTransport`, `vibelang.startTransport`, `vibelang.stopTransport`, `vibelang.setBpm`, `vibelang.seekBeat` |
| Studio panels | `vibelang.openInspector`, `vibelang.openMixer`, `vibelang.openArrangement`, `vibelang.openSliders`, `vibelang.openSoundDesigner`, `vibelang.openPatternEditor`, `vibelang.openMelodyEditor`, `vibelang.openSampleBrowser`, `vibelang.openEffectRack` |
| Editing/entity | `vibelang.formatDocument`, `vibelang.goToSource`, `vibelang.selectEntity` |
| Entity playback | `vibelang.startPattern`, `vibelang.stopPattern`, `vibelang.startMelody`, `vibelang.stopMelody`, `vibelang.startSequence`, `vibelang.stopSequence` |
| Group | `vibelang.muteGroup`, `vibelang.soloGroup` |
| Runtime process | `vibelang.bootRuntime`, `vibelang.killRuntime`, `vibelang.restartRuntime` |

### Settings and defaults

| Area | Exact keys (`vibelang.` prefix) and defaults |
|---|---|
| LSP | `lsp.enabled=true`; `lsp.diagnostics.enabled=true`; `.delay=300` (0..5000); `.onType=true`; `lsp.completion.enabled=true`; `lsp.hover.enabled=true`; `lsp.gotoDefinition.enabled=true`; `lsp.trace.server="off"` (`off`, `messages`, or `verbose`) |
| Lint | `linting.unknownSynthdef="warning"`; `unknownEffect="warning"`; `unresolvedImport="error"`; `patternSyntax="error"`; `melodySyntax="error"`; `unusedVariable="hint"` |
| Runtime | `runtime.binaryPath="vibe"`; `host="localhost"`; `port=1606` (1..65535); `autoConnect=false`; `reconnectOnDisconnect=true`; `connectionTimeout=5000` (1000..30000); `pollingInterval=100` (50..1000) |
| Format | `format.enabled=true`; `indentSize=4` (1..8); `insertFinalNewline=true`; `trimTrailingWhitespace=true`; `patternSpacing=true`; `patternGroupSize=4` (2..16); `autoImport=true`; `sortImports=true` |
| Editor | `editor.parameterHints=true`; `editor.inlayHints.enabled=false`; `editor.codeLens.enabled=true` |
| Studio | `studio.mixer.autoOpen=false`; `studio.transportBar.showBeatCounter=true`; `.showBpm=true`; `studio.sessionExplorer.showSourceLocations=true`; `studio.mixer.meterRefreshRate=60` (10..120) |
| Samples | `samples.searchPaths=[]`; `samples.previewOnSelect=true` |

Current implementation gaps:

- RuntimeManager is constructed with hard-coded auto-connect true and
  localhost:1606, ignoring host/port/autoConnect settings.
- Polling is hard-coded 500 ms, ignoring the 100 ms setting.
- Several LSP/lint/editor toggles are not forwarded to the Rust server.
- Welcome text says `vibe run -w`; runtime boot constructs positional shorthand
  plus nonexistent `--api`. Both invocations are invalid for the current CLI.
- UI requests include `fade_beats`, quantization, and update fields accepted but
  ignored by HTTP handlers, so success may not mean the requested change.
- Runtime note-on defaults velocity 100 while REST/core body uses Float default
  0.8, a unit mismatch.

## Emacs package

Modes are `vibelang-mode`, `vibelang-cockpit-mode`,
`vibelang-sidebar-mode`, and optional `vibelang-visualization-mode`. Eglot is
preferred with lsp-mode fallback. Sources are [`emacs/*.el`](../../emacs/) and
the package [`README`](../../emacs/README.md).

### User options

| Area | Exact option/default |
|---|---|
| Connection/runtime | `vibelang-ws-host="127.0.0.1"`; `vibelang-ws-port=1606`; `vibelang-auto-connect='if-running`; `vibelang-executable="vibe"`; `vibelang-runtime-args='("run")`; `vibelang-auto-reload-on-save=nil` |
| UI | `vibelang-visualization-enabled=t`; `vibelang-enable-lsp=t`; `vibelang-enable-header-line=t`; `vibelang-sidebar-on-connect=nil`; `vibelang-eval-flash-duration=0.15`; `vibelang-indent-offset=2` |
| HTTP/WS | `vibelang-api-host=nil`; `vibelang-api-port=nil`; `vibelang-http-timeout=2.0`; `vibelang-eval-save-before-run=t`; `vibelang-ws-stale-threshold=2.5`; monitor 0.5; reconnect delay 1.0; max reconnect attempts 10; resync throttle 1.5 |
| Logs | `vibelang-cockpit-log-limit=20`; `vibelang-command-preview-limit=240` |
| Sidebar | width 40; position `left`; meter width 10; show meters true; auto-inspect point true |
| LSP | `vibelang-lsp-server-command='("vibelang" "lsp")`; `vibelang-lsp-use-eglot=t` |

The default LSP executable is wrong: the actual command is `vibe lsp`, not
`vibelang lsp`.

### `vibelang-mode` keymap

| Keys | Command |
|---|---|
| `C-c C-c`, `C-c C-d` | `vibelang-connect`, `vibelang-disconnect` |
| `C-c r s`, `C-c r k`, `C-c r r`; `C-c C-k` | start, stop, restart runtime; stop alias binding |
| `C-c C-s`, `C-c C-x` | transport start/stop |
| `C-c C-t`, `C-c +`, `C-c -`, `C-c C-q` | tap, nudge up/down, set BPM |
| `C-c C-v`, `C-c C-r` | toggle visualization, reload script |
| `C-c C-e`, `C-c e b`, `C-c e r`, `C-c e l`, `C-c e p` | eval DWIM, buffer, region, line, prompt |
| `C-c .`, `C-c C-.` | cockpit, transient menu |
| `C-c C-b`, `C-c b o`, `C-c b i`, `C-c b r` | sidebar toggle/open/focus/resync |
| `M-n`, `M-p`, `C-M-h` | next/previous/mark entity |
| `C-c n r`, `C-c n c`, `C-c n .` | rename, clone, current entity |
| `C-c C-h`, `C-c h t` | help, toggle header |
| `C-c m`, `C-c s`, `C-c C-i` | mute, solo, describe entity |
| `C-c C-p`, `C-c P` | edit/tweak parameter |
| `C-c C-l`, `C-c l r` | enable/restart LSP |
| `C-c C-?` | WebSocket diagnose |

The interactive surface also includes LSP disable, auto-reload toggle,
cockpit refresh, sidebar close/toggle, runtime-running query, and entity
beginning/end commands even where no primary key is assigned.

### Sidebar keymap

`RET` action, `TAB` expand, `g` refresh, `i` focus current entity, `r` resync,
`q` quit, `m` mute, `s` solo, `p` play/stop entity, and `SPC` transport toggle.

Emacs uses the REST API for commands/eval and the WebSocket snapshot/events for
visualization, staleness detection, and resynchronization. `/eval` inherits the
CLI extension sandbox policy.
