# Contributing to solgrid

## Prerequisites

- Rust (stable toolchain, pinned via `rust-toolchain.toml`)
- Node.js 22+ and pnpm 10+ (for VSCode extension and Prettier plugin)
- [just](https://github.com/casey/just) task runner (`cargo install just`)
- wasm-pack (for WASM builds only)

## Quick Start

```bash
just --list        # Show all available commands
just check         # Verify workspace compiles
just test          # Run all Rust tests
just vscode-test   # Run VSCode extension tests
just prettier-test # Run Prettier plugin tests
just ci            # Run all CI checks locally
```

## Building from Source

```bash
just dev           # Build debug binary
just build         # Build release binary

# Or directly:
cargo build -p solgrid
cargo build --release -p solgrid
```

## Testing

### Rust tests

```bash
just test          # Run all tests
just test-doc      # Run doc tests

# Or directly:
cargo test --workspace
```

### VSCode extension — unit tests

```bash
just vscode-test

# Or directly:
pnpm install
pnpm --filter solgrid-vscode run compile
pnpm --filter solgrid-vscode test
```

### VSCode extension — LSP integration tests

These tests spawn the `solgrid server` binary and verify all LSP features (diagnostics, code actions, formatting, hover, completion, configuration, fix-on-save) via the protocol. They apply to any LSP-compatible editor, including both VSCode and Cursor.

```bash
just vscode-integration

# Or directly:
cargo build -p solgrid
SOLGRID_BIN=target/debug/solgrid pnpm --filter solgrid-vscode run test:integration
```

### VSCode extension — e2e tests

These tests launch a real VSCode instance with the extension installed and verify activation, diagnostics, and editor commands.

```bash
just vscode-e2e

# Or directly:
cargo build -p solgrid
pnpm --filter solgrid-vscode run compile:tests
SOLGRID_BIN=target/debug/solgrid node editors/vscode/out/test/e2e/run.js
```

### Prettier plugin tests

```bash
just prettier-test

# Or directly:
pnpm install
pnpm --filter prettier-plugin-solgrid run build:napi
pnpm --filter prettier-plugin-solgrid test
```

### Benchmarks

```bash
just bench                        # Run all benchmarks
just bench solgrid_formatter      # Run formatter benchmarks only
just bench solgrid_linter         # Run linter benchmarks only

# Or directly:
cargo bench --workspace
cargo bench -p solgrid_formatter
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

Run all CI checks locally with:

```bash
just ci
```

## Versioning

solgrid uses a single source of truth for version management:

1. **`Cargo.toml`** `[workspace.package] version` is the canonical version
2. All Rust crates inherit the workspace version
3. `editors/vscode/package.json` and `packages/prettier-plugin-solgrid/package.json` must match

### Version management

```bash
just version              # Check all versions are in sync
just version write        # Update all package.json files to match Cargo.toml
just version set 0.2.0    # Set a new version everywhere

# Or directly:
./scripts/version.sh
./scripts/version.sh --write
./scripts/version.sh --set 0.2.0
```

## Release Process

1. Bump version: `just version set X.Y.Z`
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
