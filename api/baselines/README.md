# Frozen v1 behavior and public-artifact baseline

This directory freezes the behavior observed at accepted M00 base
`9aff9b40db1597364279f9aacf47d436718a031e`, except that
`failure-injection-seams-v1.json` is a source-backed inventory of the repaired
M04 candidate recorded in that artifact's provenance. A golden documents
compatibility; it does not endorse an ignored field, stale success, no-op,
warning, fallback, or misleading consumer declaration.

`public-artifacts-v1.json` records deterministic byte lengths, SHA-256 digests,
tree digests, and logical counts for the v1 manifest, HTTP, Rhai/editor, WASM,
CLI, documentation, package, authoring-fixture, negative-fixture, and
failure-seam surfaces. `authoring-families-v1.json` maps every current authoring
family to an executable `.vibe` golden and state snapshot.
`known-defects-v1.json` maps every M00-named defect to a source-backed negative
fixture. `failure-injection-seams-v1.json` indexes the current phase boundaries,
structured outcomes, receipt/fence behavior, carrier projections, direct tests,
and the complete executable reload component table without changing runtime
behavior.

## Regenerate

From a clean checkout of the intended compatibility base, with no other Cargo,
Rust, Node, or package-manager process running:

```bash
CARGO_BUILD_JOBS=1 scripts/public-artifacts.sh generate
VIBELANG_UPDATE_V1_GOLDENS=1 CARGO_BUILD_JOBS=1 \
  cargo test --locked -p vibelang-rhai --test v1_api_unification_baselines -- --nocapture
```

Regeneration is an explicit compatibility decision. A later task may update a
baseline only with a classified compatibility record identifying the changed
surface, observable behavior, consumers, and migration/release action. Do not
regenerate merely to make drift disappear.

## Check and exercise drift detection

```bash
CARGO_BUILD_JOBS=1 scripts/public-artifacts.sh check
CARGO_BUILD_JOBS=1 \
  cargo test --locked -p vibelang-rhai --test v1_api_unification_baselines
python3 scripts/v1-baselines.py test-drift
```

`check` rebuilds the existing public projections first, validates every
source assertion and required catalog denominator, compares the reload component
catalog to `RELOAD_PHASE_COMPONENTS`, verifies every catalogued direct test, and
then byte-compares the fresh inventory. `test-drift` mutates in-memory byte,
semantic-claim, source-catalog, and direct-test fixtures and proves every drift
is rejected; it never edits the worktree.
