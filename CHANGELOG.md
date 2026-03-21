# Changelog

All notable changes to solgrid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- Make lint presets behaviorally meaningful, resolve `solgrid.toml` per file, and honor global discovery controls such as `include`, `exclude`, `respect_gitignore`, and `threads`
- Support documented `[lint.settings]` knobs at runtime, including compiler version comparators, rule thresholds, regex-based Foundry test naming, and line-length limits
- Make VSCode `solgrid.configPath` load an explicit config on initialize and config changes, and align the Prettier plugin with `solidityContractBodySpacing` / `solidityInheritanceBraceNewLine`
- Generate `docs/rules.md` from `solgrid list-rules` and verify it in CI to keep the rules reference aligned with the live registry

### Deprecated
- Treat `best-practices/use-natspec` as an alias for `docs/natspec-function` and keep `solidityContractNewLines` as a deprecated Prettier alias for `solidityContractBodySpacing = "single"`

### Fixed
- Remove duplicate NatSpec and custom-error diagnostics by making `docs/*` the canonical NatSpec home and only running `gas/custom-errors` when the best-practices rule is disabled
- Make runtime rule severity fallback match each rule's declared default severity instead of category-level defaults
- Fix compiler-version allowed-range checks for wide pragma ranges, make config hashing deterministic for cache invalidation, and cache LSP/CLI config resolution instead of reloading configs on every file or request


## [0.0.4] - 2026-03-19

### Added
- Cross-file hover support: imported symbols (errors, functions, contracts, etc.) now show signature and NatSpec documentation
- Transitive import resolution: hover and go-to-definition now follow re-exported symbols through intermediate files
- Implement autofix for `style/imports-ordering` rule (sorts import groups alphabetically)
- Implement autofix for `style/contract-layout` rule (reorders contract members by type)
- Implement autofix for `best-practices/visibility-modifier-order` rule (reorders function modifiers)
- Implement autofix for `best-practices/no-unused-imports` rule (removes unused import aliases)
- Implement autofix for `gas/use-constant` rule (adds `constant` modifier)
- Implement autofix for `gas/use-immutable` rule (adds `immutable` modifier)
- Implement autofix for `style/func-order` rule (reorders functions by visibility)
- Implement autofix for `style/ordering` rule (reorders top-level declarations)
- Implement autofix for `style/import-path-format` rule (converts import paths to consistent format)

### Fixed
- Fix `security/state-visibility` diagnostic span covering initializer values instead of just the declaration
- Fix `gas/bool-storage` diagnostic span highlighting leading whitespace instead of the `bool` keyword
- Fix autofix regressions in modifier ordering, unused import cleanup, function ordering, and import path normalization
- Fix reorder autofixes stripping NatSpec comments from reordered functions and top-level declarations
- Fix formatter duplicating inline assembly comments on repeated save/format
- Fix formatter moving struct-field comments, empty-block comments, wrapped initializers, and ternary indentation
- Fix formatter removing intentional single blank lines inside functions and around comment blocks
- Fix formatter emitting invalid `catch()` syntax for bare catch clauses and allow underscore-prefixed internal/private function names
- Fix formatter wrapping for long initializers, tuple assignments, modifier arguments, and multiline condition comments
- Fix import autofixes on save so multiline imports still reorder and overlapping import fixes no longer cancel each other
- Fix `style/contract-layout` code action not appearing in VSCode when cursor is on non-first violation
- Fix `style/contract-layout` autofix producing awkward member spacing and detached trailing comments
- Fix `style/ordering` and `style/func-order` code actions only appearing on the first violation in a reordered group
- Fix `style/imports-ordering` collapsing blank-line grouping and only exposing the sort fix on the first violation
- Fix `best-practices/no-unused-imports` leaving attached import comments behind when deleting whole import statements
- Avoid double formatting on save in VSCode when `editor.formatOnSave` already uses solgrid
- Deduplicate identical fixes in the fix engine to prevent overlapping-edit aborts

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

[Unreleased]: https://github.com/TateB/solgrid/compare/v0.0.4...HEAD
[0.0.4]: https://github.com/TateB/solgrid/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/TateB/solgrid/releases/tag/v0.0.3
[0.0.2]: https://github.com/TateB/solgrid/releases/tag/v0.0.2
[0.0.1]: https://github.com/TateB/solgrid/releases/tag/v0.0.1
