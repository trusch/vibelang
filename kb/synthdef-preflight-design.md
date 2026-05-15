# Synthdef Rejection Preflight Design

Design note for preventing the current scsynth rejection cascade:
`/d_recv` fails, vibelang still marks the synthdef loaded, then later
`/s_new` calls produce repeated and misleading `SynthDef not found`
server errors.

## Sources

- Local code: `crates/vibelang-core/src/backends/scsynth.rs`,
  `crates/vibelang-core/src/handlers/synthdefs.rs`,
  `crates/vibelang-core/src/handlers/voices.rs`, and
  `crates/vibelang-cli/src/main.rs`.
- SuperCollider Server Command Reference:
  <https://doc.sccode.org/Reference/Server-Command-Reference.html>.
  The docs say `/d_recv`, `/d_load`, and `/d_loadDir` are
  asynchronous synthdef commands that reply with `/done` when complete;
  `/sync` replies with `/synced` after earlier asynchronous commands
  complete; `/fail` replies contain the failing command name and an
  error message and are sent only to the sender.
- Public SC error examples show the concrete failure text shape for
  missing UGens, e.g. `exception in GraphDef_Recv: UGen 'VOSIM' not
  installed.` followed by `/s_new SynthDef not found` when the client
  tries to instantiate the rejected definition:
  <https://scsynth.org/t/problems-with-superclean-and-sc3-plugins-linux-ubuntu/12898>.

## Current Behavior

`ScsynthBackend` already decodes `/done` and `/fail`:

```rust
match msg.addr.as_str() {
    "/done" => OscResponse::Done { command },
    "/fail" => {
        tracing::warn!("Fail: {} - {}", command, reason);
        OscResponse::Fail { command, reason }
    }
}
```

Those responses are only broadcast to best-effort callbacks. There is
no pending request table for synthdef loads, no channel awaiting
`OscResponse::Done { command: "/d_recv" }`, and no path returning
`OscResponse::Fail { command: "/d_recv", reason }` to the caller.

The current load path is effectively:

```rust
async fn Backend::load_synthdef(name, data) -> Result<()> {
    send_msg("/d_recv", [Blob(data)])?;
    Ok(())
}

async fn SynthDefsHandler::load(name, data) -> Result<()> {
    backend.load_synthdef(name, data).await?;
    backend.sync().await?;

    state.synthdefs.insert(name);
    state.synthdef_outputs.insert(name, get_synthdef_outputs(name));
    state.synthdef_inputs.insert(name, get_synthdef_inputs(name));
    Ok(())
}
```

This prevents only the race where `/s_new` is sent before scsynth has
processed the preceding `/d_recv`. It does not prove the synthdef was
accepted. If scsynth rejects the graph because a UGen is missing, the
listener logs `/fail /d_recv <reason>`, `/sync` still completes, and
`SynthDefsHandler` still mutates local state.

The resulting state split is:

| Layer | Belief after rejected `/d_recv` |
|---|---|
| scsynth | Synthdef is absent. |
| `State::synthdefs` | Synthdef is present. |
| `State::synthdef_outputs` / `State::synthdef_inputs` | Port metadata is present because vibelang computes it locally from the name. |
| voice/effect/routing handlers | Local validation passes, then `/s_new` is sent. |
| scsynth logs | `/s_new SynthDef not found`, detached from the original missing UGen. |

Startup currently loads built-ins in `vibelang-cli` immediately after
backend connection and before script deploy callbacks. The primary path
uses `runtime.load_builtins().await` and wraps failures as `Failed to
load built-in synthdefs`, so a propagated rejection can become a
top-level startup error. A second CLI path logs `Failed to load built-in
synthdefs` and continues; that path should be migrated to the same
fail-fast policy or explicitly documented as best-effort.

## Protocol Design

Treat synthdef load as a real request/response operation:

```text
vibelang -> scsynth: /d_recv <bytes>
scsynth  -> vibelang: /done "/d_recv"
        or
scsynth  -> vibelang: /fail "/d_recv" "<reason>"
```

`/sync` should remain useful as a general barrier, but it should not be
used as proof that `/d_recv` succeeded. The load call should return
after the first `/done /d_recv` or `/fail /d_recv` that corresponds to
the current load operation.

### Correlation

The SC protocol replies identify only the command, not the synthdef
name or a request ID. Sequential built-in loading already sends one
`/d_recv` at a time, so the minimal implementation can serialize
synthdef loads and keep one pending `/d_recv` waiter:

```rust
struct PendingSynthDefLoad {
    name: String,
    tx: oneshot::Sender<SynthDefLoadReply>,
}

enum SynthDefLoadReply {
    Done,
    Fail { reason: String },
}
```

If future code wants concurrent synthdef loads, use `/d_recv`'s optional
completion message to include a vibelang-generated token:

```text
/d_recv <bytes> ["/vibelang/d_recv_done", <load_id>, <name>]
```

The open question is whether scsynth also sends the ordinary
`/done /d_recv` when a completion message is supplied. If it does not,
tokened completion messages are good for success correlation but still
need `/fail /d_recv` serialization or a source-code-confirmed way to
include context on failure.

## Proposed API Surface

Add first-class synthdef rejection information at the core error layer:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthDefRejection {
    pub name: String,
    pub reason: String,
    pub missing_ugen: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SynthDefPreflightError {
    pub rejected: Vec<SynthDefRejection>,
}

pub enum Error {
    SynthDefRejected {
        name: String,
        reason: String,
        missing_ugen: Option<String>,
    },
    SynthDefPreflightFailed {
        rejected: Vec<SynthDefRejection>,
    },
    // existing variants...
}
```

At the scsynth backend layer:

```rust
pub enum ScsynthError {
    SynthDefRejected {
        name: String,
        reason: String,
        missing_ugen: Option<String>,
    },
    UnexpectedSynthDefLoadReply {
        name: String,
        command: String,
    },
    // existing variants...
}

impl ScsynthBackend {
    async fn load_synthdef_checked(
        &self,
        name: &str,
        data: &[u8],
    ) -> Result<(), ScsynthError>;
}
```

The existing `Backend::load_synthdef` can either be strengthened to have
checked semantics for every backend, or a narrower native-only extension
trait can be introduced first:

```rust
#[async_trait]
pub trait CheckedSynthDefBackend: Backend {
    async fn load_synthdef_checked(
        &self,
        name: &str,
        data: &[u8],
    ) -> Result<(), Self::Error>;
}
```

The cleaner long-term design is to strengthen `Backend::load_synthdef`:
callers already assume `Ok(())` means "the synthdef is available." Mock
and WASM backends can keep immediate success, while scsynth performs the
real wait.

### Missing UGen Parsing

Parse the common SC reason shape without making the entire design depend
on it:

```rust
fn parse_missing_ugen(reason: &str) -> Option<String> {
    // Covers:
    // exception in GraphDef_Recv: UGen 'Changed' not installed.
    // exception in GraphDef_Load: UGen 'JPverbRaw' not installed.
}
```

If parsing fails, still surface the raw `reason`. The display layer can
show `missing UGen: unknown` only when `missing_ugen` is `None`; it
should never discard scsynth's exact message.

## Loader Pseudocode

```rust
async fn load_synthdef_checked(name, data) -> Result<(), ScsynthError> {
    let (tx, rx) = oneshot::channel();

    {
        let mut pending = pending_synthdef_load.lock()?;
        if pending.is_some() {
            return Err(ScsynthError::ConnectionFailed(
                "concurrent /d_recv loads are not supported yet".into(),
            ));
        }
        *pending = Some(PendingSynthDefLoad {
            name: name.to_owned(),
            tx,
        });
    }

    if let Err(err) = send_msg("/d_recv", [Blob(data.to_vec())]) {
        clear_pending_synthdef_load(name);
        return Err(err);
    }

    match timeout(SYNTHDEF_LOAD_TIMEOUT, rx).await {
        Ok(Ok(SynthDefLoadReply::Done)) => Ok(()),
        Ok(Ok(SynthDefLoadReply::Fail { reason })) => {
            Err(ScsynthError::SynthDefRejected {
                name: name.to_owned(),
                missing_ugen: parse_missing_ugen(&reason),
                reason,
            })
        }
        Ok(Err(_)) => Err(ScsynthError::ConnectionFailed(
            "synthdef load response channel closed".into(),
        )),
        Err(_) => {
            clear_pending_synthdef_load(name);
            Err(ScsynthError::Timeout)
        }
    }
}
```

Listener branch:

```rust
"/done" if command == "/d_recv" => {
    if let Some(pending) = pending_synthdef_load.take() {
        let _ = pending.tx.send(SynthDefLoadReply::Done);
    }
    Some(OscResponse::Done { command })
}

"/fail" if command == "/d_recv" => {
    if let Some(pending) = pending_synthdef_load.take() {
        let _ = pending.tx.send(SynthDefLoadReply::Fail {
            reason: reason.clone(),
        });
    }
    Some(OscResponse::Fail { command, reason })
}
```

`SynthDefsHandler::load` should register in `State` only after checked
load returns `Ok(())`. Its later `backend.sync().await` can either be
removed for scsynth loads, because `/done /d_recv` is already the async
completion signal, or kept as a conservative barrier for now. Keeping it
is lower-risk during migration.

## Startup Preflight

Add an explicit preflight API that loads a collection and returns all
rejections instead of stopping at the first one:

```rust
pub struct SynthDefBlob {
    pub name: String,
    pub bytes: Vec<u8>,
    pub source: SynthDefSource,
}

pub enum SynthDefSource {
    Builtin,
    ScriptDeploy,
    UserPath(PathBuf),
}

pub struct SynthDefPreflightReport {
    pub loaded: Vec<String>,
    pub rejected: Vec<SynthDefRejection>,
}

impl<B: Backend> SynthDefsHandler<B> {
    pub async fn preflight_load(
        &self,
        synthdefs: impl IntoIterator<Item = SynthDefBlob>,
    ) -> Result<SynthDefPreflightReport>;
}
```

Startup orchestration:

```rust
let builtins = generate_builtins()
    .into_iter()
    .map(|(name, bytes)| SynthDefBlob {
        name,
        bytes,
        source: SynthDefSource::Builtin,
    });

let report = runtime.preflight_synthdefs(builtins).await?;

if !report.rejected.is_empty() {
    return Err(Error::SynthDefPreflightFailed {
        rejected: report.rejected,
    });
}

// Only after this point:
// - install custom deploy callback
// - apply initial script state
// - create groups, voices, routes, effects
```

`preflight_load` should continue past individual
`SynthDefRejected` errors, collecting all of them, but it should still
abort immediately on transport errors such as socket failure, listener
shutdown, or timeout if those mean the report cannot be trusted.

### User-Facing Error Block

Surface one top-of-startup block before the first `/s_new`:

```text
Failed to load 3 SynthDefs into scsynth.

The SuperCollider server rejected these definitions:

  - spectraphon_dual
    missing UGen: Changed
    scsynth: exception in GraphDef_Recv: UGen 'Changed' not installed.

  - reverb_jpverb
    missing UGen: JPverbRaw
    scsynth: exception in GraphDef_Recv: UGen 'JPverbRaw' not installed.

Install the missing SuperCollider UGen plugins or remove voices/effects
that depend on these SynthDefs. Startup stopped before creating synths.
```

This must be an actual returned startup error, not only a tracing event.
The CLI can add user guidance around plugin installation, but
`vibelang-core` should carry structured rejection data so HTTP, LSP, or
future UI surfaces can render their own block.

## Optional Trial Preflight

A separate "trial" mode can validate the runtime's synthdef set without
creating groups, voices, routes, or audio-producing synths:

```rust
pub async fn preflight_synthdefs_only(
    backend: ScsynthBackend,
    synthdefs: Vec<SynthDefBlob>,
) -> Result<SynthDefPreflightReport>;
```

Behavior:

1. Connect to scsynth and perform the existing cleanup
   (`/g_freeAll 0`, `/clearSched`, `/sync`).
2. Load every built-in and script-deployed synthdef with checked
   `/d_recv` semantics.
3. Return a report and exit before the first `/s_new`.

This is useful for `vibelang doctor`, CI against known SC installations,
and user-facing diagnostics after manifest expansion. It should be
implemented after the checked loader because it depends on the same
request/response mechanism.

## Interaction With UGen Manifests

The UGen manifest audit can prevent many bad definitions before scsynth
sees them, but it should not replace `/d_recv` preflight:

- The manifest says what vibelang can emit, not what the user's current
  scsynth process actually loaded.
- Third-party plugin directories, API-version mismatches, and platform
  packaging differences are only known at runtime.
- `/u_cmd` is not a general UGen inventory query; it sends commands to
  an existing unit generator instance, so it is not a direct solution for
  "is this UGen installed?"

Recommended layering:

1. Keep compile-time/author-time manifest validation for obvious gaps.
2. Use checked `/d_recv` as the authoritative server acceptance test.
3. Optionally add tiny generated probe synthdefs per required UGen only
   if a future feature needs a server UGen inventory before compiling the
   real synthdefs.

## Migration Impact

Expected code changes for implementation:

- `ScsynthBackend` gains a pending synthdef-load waiter, plus tests for
  `/done /d_recv`, `/fail /d_recv`, and timeout cleanup.
- `Backend::load_synthdef` semantics become "loaded or rejected," not
  "UDP send succeeded." Mock backends used in handler tests should still
  return `Ok(())` unless a test explicitly injects rejection.
- `SynthDefsHandler::load` must register state only after checked load
  succeeds. Existing tests that assert successful state insertion should
  keep passing with the mock backend.
- Add a handler test where `load_synthdef` returns a rejection and assert
  that `State::synthdefs`, `synthdef_outputs`, and `synthdef_inputs` do
  not get updated.
- Add a startup/preflight test that loads multiple built-ins through a
  fake backend, collects two rejections, and formats one aggregate error.
- Review tests that expected `runtime.load_builtins()` to continue after
  backend errors. The desired behavior is fail-fast for transport errors
  and aggregate reporting for known synthdef rejections.
- Update CLI startup tests or snapshots if they match
  `Failed to load built-in synthdefs`; the error should now include the
  rejected synthdef names and raw scsynth reasons.

No existing test should intentionally rely on the old silent behavior. If
one does, it is testing the bug.

## Open Questions

- Does scsynth send `/done /d_recv` in addition to executing the optional
  `/d_recv` completion message, or is the completion message a
  replacement? This affects whether tokened success correlation is
  viable without serializing loads.
- Can `GraphDef_Recv` failure reasons ever omit `/fail` and print only
  to stderr? The SC command reference says `/fail` is the command-error
  reply, but implementation should be tested against a real missing UGen.
- Should custom deploy callbacks keep running after startup and surface
  `SynthDefRejected` as a script/reload error, or should any rejection
  after startup disable only voices/effects that depend on that synthdef?
- Should the second CLI startup path that currently logs built-in load
  failure and continues be removed, or is it intentionally best-effort
  for a mode that should survive missing synthdefs?
- Should successful preflight remove definitions afterward with `/d_free`
  in trial mode, or is process cleanup/restart enough for the intended
  diagnostic command?
