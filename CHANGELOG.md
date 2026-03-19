# Changelog

All notable changes to solgrid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.3] - 2026-03-19

### Fixed
- Fix EACCES error when spawning native binary from npm CLI and VSCode extension (npm tarballs don't preserve execute bits)

## [0.0.2] - 2026-03-18

### Fixed
- Prevent extra blank line when leading comments follow imports in formatter
- Temporarily disable VS Code Marketplace publishing in release workflow

### Added
- Robust versioning system with single source of truth (`Cargo.toml`)
- Version sync script (`scripts/version.sh`) for monorepo consistency
- CI version validation (Cargo.toml ↔ package.json sync check)
- Release tag validation (tag version ↔ Cargo.toml match)
- Git commit hash and build date in `--version` output
- `rust-toolchain.toml` for reproducible builds
- `.cargo/config.toml` for cross-compilation linker configuration
- `solgrid_wasm` crate for browser/web playground use
- npm publish workflow for `prettier-plugin-solgrid`
- Conformance test suite for Prettier plugin
- Performance benchmarks (cold lint corpus, startup time)

## [0.0.1] - 2026-03-18

Initial development release.

### Added
- **90 lint rules** across 6 categories:
  - Security (19 rules): reentrancy, tx-origin, selfdestruct, compiler-version, unchecked-transfer, and more
  - Best Practices (22 rules): no-unused-vars, no-floating-pragma, custom-errors, code-complexity, and more
  - Naming (16 rules): contract-name-capwords, func-name-mixedcase, foundry-test-functions, and more
  - Gas Optimization (15 rules): calldata-parameters, cache-array-length, struct-packing, and more
  - Style (10 rules): func-order, imports-ordering, max-line-length, and more
  - Documentation (8 rules): natspec-contract, natspec-function, license-identifier, and more
- **Three-tier auto-fix system**: safe fixes (applied with `--fix`), suggestion fixes (`--fix --unsafe-fixes`), dangerous fixes (editor code actions only)
- **Inline suppression comments**: `solgrid-disable-next-line`, `solgrid-disable-line`, block `solgrid-disable`/`solgrid-enable`, and solhint compatibility
- **Built-in formatter** with Wadler-Lindig line-fitting algorithm, comment preservation, and formatter directives (`solgrid-fmt: off/on`)
- **CLI** with subcommands: `check`, `fix`, `fmt`, `list-rules`, `explain`, `migrate`, `server`
- **Output formats**: text (colored), JSON, GitHub Actions annotations, SARIF 2.1.0
- **Incremental caching**: content-hash-based file cache with config/version invalidation
- **Configuration**: `solgrid.toml` with `[lint]`, `[format]`, `[global]` sections; Foundry.toml fallback
- **Migration**: `solgrid migrate --from solhint` converts `.solhint.json` to `solgrid.toml`
- **Stdin/stdout support** for editor integrations and piping
- **LSP server** (`solgrid server`): real-time diagnostics, code actions, formatting, range formatting, hover docs, suppression completion, workspace configuration
- **VSCode extension**: language client with fix-on-save, format-on-save, configurable settings
- **Prettier plugin** (`prettier-plugin-solgrid`): NAPI-RS bindings with full Prettier option mapping
- **Benchmark infrastructure**: criterion benchmarks for lint and format operations
- **Release workflow**: GitHub Actions with multi-platform builds (Linux, macOS Intel/ARM, Windows), VSIX packaging, GitHub Release with checksums
- **Binary optimization**: strip, LTO, codegen-units=1
- 309+ tests across Rust workspace, VSCode extension (unit, integration, e2e), and Prettier plugin

[Unreleased]: https://github.com/TateB/solgrid/compare/v0.0.3...HEAD
[0.0.3]: https://github.com/TateB/solgrid/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/TateB/solgrid/releases/tag/v0.0.2
[0.0.1]: https://github.com/TateB/solgrid/releases/tag/v0.0.1
