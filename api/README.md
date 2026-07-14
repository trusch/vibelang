# Public API manifest

`public-api-manifest-v1.json` is the deterministic, versioned inventory of the
VibeLang API exposed to `.vibe` programs. It is generated from the effective
Rhai engine metadata, the Rust registration declarations, canonical DSP UGen
JSON manifests, and parsed stdlib `.vibe` sources. It contains no timestamp,
host path, build directory, or Git state.

Regenerate it from the repository root with:

```bash
CARGO_BUILD_JOBS=1 cargo run -p xtask -- public-api generate
```

Check that the committed artifact is current without modifying it:

```bash
CARGO_BUILD_JOBS=1 cargo run -p xtask -- public-api check
```

P0.1 records existing registrations, including the current demand-rate fallback
as `runtime_rate: "audio"`. Lifecycle, terminal, export, and support fields are
deliberately marked `pending-p0.4`; this manifest does not change or classify
runtime semantics.
