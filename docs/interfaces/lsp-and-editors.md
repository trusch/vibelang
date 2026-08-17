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
and initializes UGen/validation caches. Completion, hover, signature, semantic
classification, and inlays consume the Rhai table generated from
`api/public-api-manifest-v1.json`; the shared artifact check rejects stale
handwritten rows and fictional emitted calls in the LSP and VS Code sources.

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

API metadata is manifest-generated. Numeric quantization overloads and the
`sample` constructor are projected directly; removed legacy identifiers are not
retained as signature, completion, inlay, or semantic-token rows.

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

Static extraction finds 31 `defcustom` declarations. Every exact option and
literal default is listed here; `nil` API host/port inherit the WebSocket value.

| Area | Exact option | Default |
|---|---|---|
| Connection | `vibelang-ws-host` | `"127.0.0.1"` |
| Connection | `vibelang-ws-port` | `1606` |
| Connection | `vibelang-auto-connect` | `'if-running` |
| Runtime | `vibelang-executable` | `"vibe"` |
| Runtime | `vibelang-runtime-args` | `'("run")` |
| Runtime | `vibelang-auto-reload-on-save` | `nil` |
| Runtime | `vibelang-tap-tempo-timeout` | `3.0` seconds |
| UI | `vibelang-visualization-enabled` | `t` |
| UI | `vibelang-enable-lsp` | `t` |
| UI | `vibelang-enable-header-line` | `t` |
| UI | `vibelang-sidebar-on-connect` | `nil` |
| UI | `vibelang-eval-flash-duration` | `0.15` seconds |
| UI | `vibelang-indent-offset` | `2` |
| HTTP | `vibelang-api-host` | `nil` |
| HTTP | `vibelang-api-port` | `nil` |
| HTTP | `vibelang-http-timeout` | `2.0` seconds |
| HTTP | `vibelang-eval-save-before-run` | `t` |
| WebSocket | `vibelang-ws-stale-threshold` | `2.5` seconds |
| WebSocket | `vibelang-ws-monitor-interval` | `0.5` seconds |
| WebSocket | `vibelang-ws-reconnect-delay` | `1.0` second |
| WebSocket | `vibelang-ws-max-reconnect-attempts` | `10` |
| WebSocket | `vibelang-ws-resync-throttle` | `1.5` seconds |
| Logs | `vibelang-cockpit-log-limit` | `20` |
| Logs | `vibelang-command-preview-limit` | `240` |
| Sidebar | `vibelang-sidebar-width` | `40` |
| Sidebar | `vibelang-sidebar-position` | `'left` |
| Sidebar | `vibelang-sidebar-meter-width` | `10` |
| Sidebar | `vibelang-sidebar-show-meters` | `t` |
| Sidebar | `vibelang-sidebar-auto-inspect-point` | `t` |
| LSP | `vibelang-lsp-server-command` | `'("vibelang" "lsp")` |
| LSP | `vibelang-lsp-use-eglot` | `t` |

The default LSP executable is wrong: the actual command is `vibe lsp`, not
`vibelang lsp`.

### Interactive commands and default keys

Static extraction finds 56 public `vibelang-*` defuns containing an interactive
form. “Unbound” means no key in the checked-in VibeLang, cockpit, or sidebar
maps; the command remains available through `M-x`.

| Exact command | Default key(s) / map |
|---|---|
| `vibelang-indent-line` | Mode indentation command; no explicit key |
| `vibelang-lsp-enable` | `C-c C-l` |
| `vibelang-lsp-disable` | Unbound |
| `vibelang-lsp-restart` | `C-c l r` |
| `vibelang-current-entity` | `C-c n .` |
| `vibelang-beginning-of-entity` | Unbound |
| `vibelang-end-of-entity` | Unbound |
| `vibelang-forward-entity` | `M-n` |
| `vibelang-backward-entity` | `M-p` |
| `vibelang-mark-entity` | `C-M-h` |
| `vibelang-rename-entity-definition` | `C-c n r` |
| `vibelang-clone-entity` | `C-c n c` |
| `vibelang-connect` | `C-c C-c`; cockpit `c` |
| `vibelang-disconnect` | `C-c C-d`; cockpit `d` |
| `vibelang-start-runtime` | `C-c r s`; cockpit `s` |
| `vibelang-stop-runtime` | `C-c C-k`, `C-c r k`; cockpit `k` |
| `vibelang-restart-runtime` | `C-c r r`; cockpit `R` |
| `vibelang-eval-dwim` | `C-c C-e`; cockpit `e` |
| `vibelang-eval-region` | `C-c e r` |
| `vibelang-eval-buffer` | `C-c e b` |
| `vibelang-eval-line` | `C-c e l` |
| `vibelang-eval-prompt` | `C-c e p` |
| `vibelang-transport-start` | `C-c C-s`; cockpit `p` |
| `vibelang-transport-stop` | `C-c C-x`; cockpit `x` |
| `vibelang-bpm-nudge-up` | `C-c +` |
| `vibelang-bpm-nudge-down` | `C-c -` |
| `vibelang-set-bpm` | `C-c C-q` |
| `vibelang-tap-tempo` | `C-c C-t` |
| `vibelang-toggle-visualization` | `C-c C-v` |
| `vibelang-reload-script` | `C-c C-r`; cockpit `r` |
| `vibelang-toggle-header-line` | `C-c h t` |
| `vibelang-cockpit-open` | `C-c .` |
| `vibelang-cockpit-refresh` | Cockpit `g` |
| `vibelang-tweak-param-at-point` | `C-c P` |
| `vibelang-edit-param-at-point` | `C-c C-p` |
| `vibelang-mute-at-point` | `C-c m` |
| `vibelang-solo-at-point` | `C-c s` |
| `vibelang-describe-entity` | `C-c C-i` |
| `vibelang-help` | `C-c C-h` |
| `vibelang-toggle-auto-reload` | Unbound |
| `vibelang-sidebar-open` | `C-c b o` |
| `vibelang-sidebar-close` | Unbound |
| `vibelang-sidebar-toggle` | `C-c C-b`; cockpit `b` |
| `vibelang-sidebar-quit` | Sidebar `q` |
| `vibelang-sidebar-refresh` | Sidebar `g` |
| `vibelang-sidebar-focus-current-entity` | `C-c b i`; sidebar `i` |
| `vibelang-sidebar-request-resync` | `C-c b r`; sidebar `r` |
| `vibelang-sidebar-transport-toggle` | Sidebar `SPC` |
| `vibelang-sidebar-toggle-expand` | Sidebar `TAB` |
| `vibelang-sidebar-action` | Sidebar `RET` |
| `vibelang-sidebar-toggle-mute` | Sidebar `m` |
| `vibelang-sidebar-toggle-solo` | Sidebar `s` |
| `vibelang-sidebar-play-stop` | Sidebar `p` |
| `vibelang-ws-disconnect` | Unbound |
| `vibelang-ws-request-resync` | Unbound |
| `vibelang-ws-diagnose` | `C-c C-?`; cockpit `?` |

`C-c C-.` invokes the separately macro-defined transient command
`vibelang-menu`. The four interactive mode commands are `vibelang-mode`,
`vibelang-cockpit-mode`, `vibelang-sidebar-mode`, and
`vibelang-visualization-mode`; they are not part of the 56 `defun` count.

Emacs uses the REST API for commands/eval and the WebSocket snapshot/events for
visualization, staleness detection, and resynchronization. `/eval` inherits the
CLI extension sandbox policy.
