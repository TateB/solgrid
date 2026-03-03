# Contributing to solgrid

## Prerequisites

- Rust (stable toolchain, pinned via `rust-toolchain.toml`)
- Node.js 22+ and pnpm 10+ (for VSCode extension and Prettier plugin)
- wasm-pack (for WASM builds only)

## Building from Source

```bash
# Build the CLI binary
cargo build -p solgrid

# Build in release mode
cargo build --release -p solgrid
```

## Testing

### Rust tests

```bash
cargo test --workspace
```

### VSCode extension — unit tests

```bash
cd editors/vscode
pnpm install
pnpm test:unit
```

### VSCode extension — LSP integration tests

These tests spawn the `solgrid server` binary and verify all LSP features (diagnostics, code actions, formatting, hover, completion, configuration, fix-on-save) via the protocol. They apply to any LSP-compatible editor, including both VSCode and Cursor.

```bash
# Build the solgrid binary first
cargo build -p solgrid

# Run integration tests
cd editors/vscode
SOLGRID_BIN=../../target/debug/solgrid pnpm test:integration
```

### VSCode extension — e2e tests

These tests launch a real VSCode instance with the extension installed and verify activation, diagnostics, and editor commands.

```bash
cargo build -p solgrid
cd editors/vscode
SOLGRID_BIN=../../target/debug/solgrid pnpm test:e2e
```

### Prettier plugin tests

```bash
# Build the NAPI native addon first
cd packages/prettier-plugin-solgrid
pnpm install
pnpm build:napi

# Run tests
pnpm test
```

### Benchmarks

```bash
# Run all benchmarks
cargo bench --workspace

# Run formatter benchmarks only
cargo bench -p solgrid_formatter

# Run linter benchmarks only (includes cold lint corpus)
cargo bench -p solgrid_linter

# Run startup/initialization benchmarks
cargo bench -p solgrid
```

## CI

The GitHub Actions workflow (`.github/workflows/ci.yml`) runs the full test suite on every push and on pull requests:

- `cargo check --workspace --all-targets`
- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- VSCode extension unit tests
- VSCode LSP integration tests
- VSCode e2e tests
- Prettier plugin build + tests
- Version sync validation (`scripts/version.sh`)

## Versioning

solgrid uses a single source of truth for version management:

1. **`Cargo.toml`** `[workspace.package] version` is the canonical version
2. All Rust crates inherit the workspace version
3. `editors/vscode/package.json` and `packages/prettier-plugin-solgrid/package.json` must match

### Version management

```bash
# Check all versions are in sync
./scripts/version.sh

# Update all package.json files to match Cargo.toml
./scripts/version.sh --write

# Set a new version everywhere
./scripts/version.sh --set 0.2.0
```

## Release Process

1. Bump version: `./scripts/version.sh --set X.Y.Z`
2. Commit: `git commit -am "chore: bump version to X.Y.Z"`
3. Tag: `git tag vX.Y.Z`
4. Push: `git push origin main --tags`
5. CI builds all platforms, publishes VSIX to VS Marketplace, and publishes npm package

The `--version` flag includes the git commit hash and build date:

```
$ solgrid --version
solgrid 0.1.0 (abc1234 2026-03-03)
```

## Project Structure

```
crates/
  solgrid/              # Binary crate — CLI entry point
  solgrid_ast/          # AST utilities
  solgrid_cache/        # Incremental analysis cache
  solgrid_config/       # Config file parsing
  solgrid_diagnostics/  # Diagnostic types and reporting
  solgrid_formatter/    # Built-in Solidity formatter
  solgrid_linter/       # Rule engine and registry
  solgrid_napi/         # NAPI-RS bindings (Prettier plugin)
  solgrid_parser/       # Solar parser wrapper
  solgrid_server/       # LSP server
  solgrid_testing/      # Test utilities
  solgrid_wasm/         # WASM bindings
editors/vscode/         # VSCode extension (TypeScript)
packages/prettier-plugin-solgrid/  # Prettier plugin (npm)
scripts/                # Version management scripts
```
