# Reproducible public artifacts

The repository-owned entry point for every checked-in public artifact is:

```bash
scripts/public-artifacts.sh generate
```

Its non-mutating CI form is:

```bash
scripts/public-artifacts.sh check
```

The command serializes Cargo with `CARGO_BUILD_JOBS=1`, renders the default
Clap command/subcommand help from the just-built `vibe` binary, then regenerates
or checks the manifest-backed core/UGen/stdlib reference, WASM TypeScript
surface, HTTP route/schema snapshot, VS Code core/stdlib tables, and the
canonical UGen subsets bundled by the LSP and VS Code extension. A dirty CLI
option therefore changes the help snapshot and fails `check`; it cannot be
published as a clean-revision artifact by accident.

The editor UGen bundles intentionally remain the existing 24-category package
subset. The entry point refreshes every bundled file byte-for-byte from
`crates/vibelang-dsp/ugen_manifests` and rejects a bundled filename without a
canonical source; adding previously unbundled plugin/experimental categories is
an editor availability decision, not an artifact-regeneration side effect.

`crates/vibelang-wasm/types/index.d.ts` is generated from the Rust
`#[wasm_bindgen]` export annotations and serialized result structs. Private
`InitOutput` ABI fields, generated JavaScript, and the `.wasm` binary are
intentionally excluded because they are tool-version-specific build outputs.
Functions marked `#[wasm_bindgen(start)]` are also excluded because they run as
module lifecycle hooks rather than callable JavaScript exports. The declarations
retain only a stable opaque module-initializer compatibility shim; the
source-backed callable class/function surface remains drift-gated.

## Public API manifest

`public-api-manifest-v1.json` is the deterministic, versioned inventory of the
VibeLang API exposed to `.vibe` programs. It is generated from the effective
Rhai engine metadata, Rust registration declarations and callable ASTs,
canonical DSP UGen JSON manifests, and parsed stdlib `.vibe` sources. It
contains no timestamp, host path, build directory, or Git state.

Each callable overload records accepted types plus classified coercion, cast,
clamp, range, fallback, structured-error, and panic-exposure facets. Generated
`CustomType` properties are tied back to their source fields and derive-template
semantics, including fallible `Option<T>` setters; declared callables retain
their definition anchors and transitive in-project boundary evidence. A facet
is `present`, `none`, or `not_applicable`; mechanically unresolved evidence is
`unknown` and fails generation. Stdlib functions require an adjacent
`@vibelang-api export=... support=...` source annotation. The manifest preserves
all 707 declarations, import paths, exact signatures, source anchors, and
duplicate-name behavior.

Regenerate only the registration manifest and its stdlib reference adapter with:

```bash
CARGO_BUILD_JOBS=1 cargo run -p xtask -- public-api generate
```

Check that the committed artifact is current without modifying it:

```bash
CARGO_BUILD_JOBS=1 cargo run -p xtask -- public-api check
```

The same command owns the manifest-backed
`docs/reference/generated/stdlib.md` adapter. The generator records current v1
behavior; it does not normalize or repair permissive runtime semantics. The 25
demand-rate identities remain explicit quarantined, non-callable records.
