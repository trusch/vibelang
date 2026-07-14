# Public API manifest

`public-api-manifest-v1.json` is the deterministic, versioned inventory of the
VibeLang API exposed to `.vibe` programs. It is generated from the effective
Rhai engine metadata, Rust registration declarations and callable ASTs,
canonical DSP UGen JSON manifests, and parsed stdlib `.vibe` sources. It
contains no timestamp, host path, build directory, or Git state.

Each callable overload records accepted types plus classified coercion, cast,
clamp, range, fallback, structured-error, and panic-exposure facets. A facet is
`present`, `none`, or `not_applicable`; `unknown` is serializable for staged
schema migrations but fails generation. Stdlib functions require an adjacent
`@vibelang-api export=... support=...` source annotation. The manifest preserves
all 707 declarations, import paths, exact signatures, source anchors, and
duplicate-name behavior.

Regenerate it from the repository root with:

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
