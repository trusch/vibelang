# Releasing VibeLang

## Publish Order (dependency graph)

Crates must be published in this order — each level depends only on previously published crates.

| Order | Crate | Depends on |
|-------|-------|------------|
| 1 | `vibelang-dsp` | (none) |
| 1 | `vibelang-keys` | (none) |
| 1 | `vibelang-std` | (none) |
| 2 | `vibelang-sfz` | dsp |
| 3 | `vibelang-core` | dsp, sfz |
| 4 | `vibelang-rhai` | core, dsp, std |
| 4 | `vibelang-http` | core |
| 5 | `vibelang-lsp` | core, rhai, dsp, std |
| 6 | `vibelang-cli` | core, rhai, dsp, std, lsp, http |
| — | `vibelang-wasm` | core, rhai, dsp (publish manually if needed) |

## Release Checklist

### 1. Pre-flight

```bash
# Clean working tree
git status  # must be clean

# All tests pass
bash -c "cargo test --workspace"

# Clippy clean
bash -c "cargo clippy --workspace"

# Check for uncommitted Cargo.lock changes
git diff Cargo.lock
```

### 2. Bump versions

Update `version = "X.Y.Z"` in all crate Cargo.toml files **and** their cross-references:

```bash
# Files to edit (all in crates/*/Cargo.toml):
OLD="0.3.0"
NEW="0.4.0"

# Bump package versions
sed -i "s/^version = \"$OLD\"/version = \"$NEW\"/" crates/*/Cargo.toml

# Bump internal dependency versions
sed -i "s/vibelang-core = { version = \"$OLD\"/vibelang-core = { version = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-core = \"$OLD\"/vibelang-core = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-dsp = { version = \"$OLD\"/vibelang-dsp = { version = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-dsp = \"$OLD\"/vibelang-dsp = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-sfz = { version = \"$OLD\"/vibelang-sfz = { version = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-sfz = \"$OLD\"/vibelang-sfz = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-std = { version = \"$OLD\"/vibelang-std = { version = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-std = \"$OLD\"/vibelang-std = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-rhai = { version = \"$OLD\"/vibelang-rhai = { version = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-rhai = \"$OLD\"/vibelang-rhai = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-lsp = { version = \"$OLD\"/vibelang-lsp = { version = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-lsp = \"$OLD\"/vibelang-lsp = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-http = { version = \"$OLD\"/vibelang-http = { version = \"$NEW\"/" crates/*/Cargo.toml
sed -i "s/vibelang-http = \"$OLD\"/vibelang-http = \"$NEW\"/" crates/*/Cargo.toml
```

### 3. Verify after bump

```bash
# Must compile
bash -c "cargo check --workspace"

# Tests still pass
bash -c "cargo test --workspace"

# Dry-run publish for each crate (in order)
for crate in vibelang-dsp vibelang-keys vibelang-std vibelang-sfz vibelang-core vibelang-rhai vibelang-http vibelang-lsp vibelang-cli; do
  echo "=== $crate ==="
  bash -c "cargo publish --dry-run -p $crate --registry crates-io 2>&1" | tail -3
done
```

### 4. Commit and tag

```bash
git add -A
git commit -m "chore: bump version to X.Y.Z"
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin main --tags
```

### 5. Publish to crates.io (in order, wait ~30s between each)

```bash
for crate in vibelang-dsp vibelang-keys vibelang-std vibelang-sfz vibelang-core vibelang-rhai vibelang-http vibelang-lsp vibelang-cli; do
  echo "Publishing $crate..."
  bash -c "cargo publish -p $crate --registry crates-io"
  echo "Waiting for index update..."
  sleep 30
done
```

### 6. Create GitHub release

```bash
gh release create vX.Y.Z --title "VibeLang vX.Y.Z" --notes-file RELEASE_NOTES.md
```

### 7. Post-release

- Post to Reddit (r/rust, r/musicprogramming, r/synthesizers)
- Update landing page version references if any
- Verify `cargo install vibelang-cli` works from a clean environment
