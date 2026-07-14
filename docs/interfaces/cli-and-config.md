# CLI and configuration

These command/configuration contracts are separate from the `.vibe` Rhai API.
The authoritative parsers are Clap/Serde definitions, not similarly named
script functions.

## `vibe`

Source: [`vibelang-cli/src/main.rs`](../../crates/vibelang-cli/src/main.rs#L28-L178).
Implicit `--help` and `--version` are provided by Clap.

| Invocation | Availability | Purpose |
|---|---|---|
| `vibe FILE` | Always | Exact shorthand for `vibe run FILE` with all defaults; Run options are not part of this shorthand form |
| `vibe run FILE [OPTIONS]` | Always | Evaluate, reconcile, and by default watch a `.vibe` script |
| `vibe render SCORE OUTPUT [OPTIONS]` | Always | Render a `.vibescore` archive through scsynth NRT |
| `vibe devices` | `midi` feature | List midir input and output ports |
| `vibe lsp` | `lsp` feature | Run stdio language server |

No command/file prints usage and exits 1.

### `vibe run`

| Exact option | Type/default | Behavior |
|---|---|---|
| `--no-watch` | false | Disables recursive `.vibe` watch; reload is default |
| `--no-api` | false | Disables HTTP/WS server; API is default with `api` feature |
| `--api-port PORT` | u16, 1606 | HTTP port |
| `--api-bind ADDR` | IP, 127.0.0.1 | Bind address; `0.0.0.0` exposes every interface |
| `--api-allow-extensions` | false | Gives `/eval` the configured local fs/exec/net extensions |
| `-I, --include PATH` | repeatable | Extra import path; stdlib and its parent are also searched |
| `--scsynth-addr ADDR` | String, `127.0.0.1:57110` | Existing/default server address |
| `--no-boot` | false | Connect without starting scsynth |
| `--no-jack-connect` | false | Suppress automatic JACK/PipeWire output links |
| `--jack-connect-to PORTS` | comma list | One destination per output channel; empty entries error |
| `--jack-connect-from PORTS` | comma list | One source per input channel; empty entries error |
| `--device NAME` | optional | Backend audio device |
| `--sample-rate HZ` | u32, 0 | 0 means hardware default |
| `--input-channels COUNT` | u32, default 2 without profile | Hardware inputs |
| `--output-channels COUNT` | u32, default 2 without profile | Hardware outputs |
| `--profile FILE` | optional | Strict hardware startup profile below |
| `--runtime-metrics` | false | Record bounded runtime metrics and print snapshot at shutdown |
| `--no-extensions` | false | Disable all compiled script extensions |
| `--no-fs`; `--no-exec`; `--no-net` | false | Disable one compiled extension |
| `--fs-sandbox PATH` | optional | Filesystem extension base; see security limits in [Extensions](../reference/extensions.md) |

Feature-disabled API/extension arguments are hidden in help. The watcher
debounces about 100 ms and reacts only to `.vibe` changes. Initial errors abort;
reload errors retain current runtime. Transport starts after the initial state
is submitted. `RUST_LOG` controls tracing.

There is no `-w`, `--watch`, `--api`, `--midi-input`, or `--record` option.
Watch/API are enabled by default. Some older README/editor/render messages still
refer to these nonexistent spellings.

### `vibe render`

```text
vibe render SCORE_FILE OUTPUT [-f|--format FORMAT]
    [-s|--sample-rate HZ=48000] [-b|--bit-depth BITS=24]
```

Input must exist and have lowercase `.vibescore`. Format is the explicit string
or lowercase output extension, default `wav`. Bit depths 16/24/32 select int16,
int24/float32; every other value silently falls back to 24 despite the help
text. WAV is stereo. Non-WAV invokes `ffmpeg`; tuned formats are mp3, flac, ogg,
and undocumented aac/m4a, while an unknown format is passed through for ffmpeg
to infer. Sample rate zero is not rejected. Archives accept `score.osc`,
`synthdefs/*.scsyndef`, and `samples/*` entries. Source:
[`render.rs`](../../crates/vibelang-cli/src/render.rs).

## Startup profile TOML

Source: [`startup_profile.rs`](../../crates/vibelang-cli/src/startup_profile.rs).
Unknown fields are rejected. `--profile` wins; otherwise the first 32 script
lines may contain `// vibe-profile: PATH`, resolved beside the script.

| Location | Exact fields and defaults |
|---|---|
| top level | required `version = 1`; required nonempty `name` |
| `[audio]` | required positive `input_channels`, `output_channels`; `client = "SuperCollider"`; optional nonempty `device`; `manage_links = false` |
| `[[audio.input]]` / `[[audio.output]]` | required `channel`, `name`, `external_port`; `required = true` |
| `[[service]]` | unique nonempty `name`, nonempty `unit`; `required = true` |
| `[[endpoint]]` | unique nonempty `name`, nonempty `pattern`; `backend = "pipewire"` or `"midi"` (default pipewire); direction is `"source"`, `"sink"`, or `"any"` (default any); `required = true` |
| `[policy]` | `allow_degraded_start = false`; `readiness_timeout_ms = 2000`, valid 1..60000 |

Exactly one input/output link per declared channel is required. Channel range,
uniqueness, names, and strings are validated. CLI channel/device values must
equal the profile; manual `--jack-connect-to/from` cannot accompany a profile.
Missing required services block before scsynth. Probes run about every 100 ms
until timeout. Optional losses also wait unless degraded start is allowed.
`manage_links=true` makes the CLI establish named links; false still verifies
externally managed links.

```toml
version = 1
name = "live-rig"

[audio]
client = "SuperCollider"
input_channels = 2
output_channels = 2
manage_links = true

[[audio.input]]
channel = 0
name = "mic-left"
external_port = "alsa_input.*:capture_FL"

[[audio.output]]
channel = 0
name = "main-left"
external_port = "alsa_output.*:playback_FL"

[policy]
allow_degraded_start = false
readiness_timeout_ms = 2000
```

No other general `vibe` config file is parsed.

## `vibe-keys`

`vibe-keys [OPTIONS]` opens the terminal keyboard. Source:
[`main.rs`](../../crates/vibelang-keys/src/main.rs),
[`config.rs`](../../crates/vibelang-keys/src/config.rs), and
[`keyboard.rs`](../../crates/vibelang-keys/src/keyboard.rs).

| Command / option | Type/default | Behavior |
|---|---|---|
| `init` | subcommand | Write commented default config |
| `config-path` | subcommand | Print platform config path |
| `list-ports` | subcommand | Print JACK MIDI inputs, or non-error status without JACK/ports |
| `-c, --config PATH` | optional | Explicit config; read/TOML failure exits |
| `--us-layout` | bool | Override layout |
| `--client-name STRING` | `vibe-keys` | JACK client name |
| `-o, --octave U8` | optional | Runtime clamps 0..9; base note becomes `12 + octave*12` |
| `--channel U8` | 0 | Runtime clamps to 15 |
| `--velocity U8` | 100 | Runtime clamps 1..127 |

CLI values override config. Esc/Ctrl-C quits and releases notes. `<`/Left and
`>`/Right change octave and release current notes. JACK is probed with
`NO_START_SERVER`; unavailable JACK disables MIDI but leaves the TUI running.

### `vibe-keys` config

Platform path is `ProjectDirs("vibe-keys")/config.toml`. Every section uses
Serde defaults.

| Section | Exact fields/defaults |
|---|---|
| `[keyboard]` | `layout = "german"` (`german`, `us`, or `custom`), `base_note = 48`, `velocity = 100`, `channel = 0`, `note_release_ms = 400`, optional `custom_mappings` |
| custom mapping item | `key` character, optional `display` character, `offset` i8, `black` Bool |
| `[midi]` | `client_name = "vibe-keys"`, `port_name = "midi_out"`, optional String array `auto_connect` |
| `[theme]` | `white_key_color="white"`, `black_key_color="dark_gray"`, `pressed_key_color="cyan"`, `border_color="cyan"`, `show_note_names=true`, `show_help=true` |

`layout=custom` without mappings silently uses German. Colors accept known names
and `#RRGGBB`; invalid values render white. Loading the default path uses
`load_or_default`, so any missing/read/parse failure replaces the whole config
with defaults. `midi.auto_connect` currently does not connect: it only logs a
`jack_connect` instruction and returns success.
