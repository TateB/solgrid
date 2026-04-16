# Changelog

All notable changes to solgrid will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- Fixed trailing-operator formatter output so wrapped binary and logical expressions align flat instead of adding an extra continuation indent

## [0.0.10] - 2026-04-15

### Added
- Added a formatter `operator_line_break` option, plus Prettier `solidityOperatorLineBreak`, so multiline binary and logical chains converge to either canonical leading-operator or trailing-operator layouts

### Fixed
- Fixed formatter header comment attachment so inline comments after constructor base calls stay on the header line instead of moving into the constructor body

## [0.0.9] - 2026-04-15

## [0.0.8] - 2026-04-14

### Fixed
- Fixed `fix` retaining imports referenced by NatSpec `@inheritdoc` tags so autofixes no longer leave compiler-broken documentation references behind
- Fixed `naming/func-name-mixedcase` to allow public ABI-style uppercase function names by default while still supporting exact-name and regex exceptions
- Fixed formatter output for long `for` headers, infinite `for (;;)` loops, single-statement control flow, and top-level file-scope declaration spacing to use stable canonical layouts
- Fixed formatter comment preservation so standalone comments stay in their original gap by default and comments after the last child in a block or declaration stay inside the enclosing scope instead of drifting onto neighboring items or outside the closing brace

## [0.0.7] - 2026-04-14

### Added
- Added exact-name and regex-based exceptions for `naming/func-name-mixedcase` so legacy public ABI names can be allowed without repeated inline suppressions

### Fixed
- Fixed VS Code save-time import ordering so `style/imports-ordering` fixes no longer get reverted by a follow-up `solgrid` formatting request
- Fixed formatter import spacing to canonicalize blank lines between configured import groups instead of collapsing them on save
- Fixed `fix` output to run through the formatter before the final lint pass so `fix` and `fmt` converge on one canonical steady state
- Fixed formatter comment attachment for ignored parameters and call arguments so inline annotations stay attached to the parameter or argument they describe
- Fixed formatter output for multiline bitwise chains, long `return` expressions, multiline enums, and typed `catch (...)` clauses to use stable readable canonical layouts

## [0.0.6] - 2026-04-13

### Fixed
- Fixed VS Code save-time import ordering so `style/imports-ordering` fixes no longer get reverted by a follow-up `solgrid` formatting request
- Fixed formatter import spacing to canonicalize blank lines between configured import groups instead of collapsing them on save

## [0.0.5] - 2026-04-01

### Added
- Intelligent Solidity autocomplete with in-scope symbols, member completions (`msg.`, `MyEnum.`, `MyLib.`), builtins, imported symbols, and auto-import suggestions that can insert missing `import` statements
- Workspace-wide `.sol` symbol indexing with incremental updates to keep LSP autocomplete current as files change
- Type-aware member autocomplete and signature help for user-defined functions, constructors, and builtins
- `style/prefer-remappings` rule that suggests using project remappings instead of relative imports

### Changed
- Config resolution now honors per-file `solgrid.toml` discovery together with global `include`, `exclude`, `respect_gitignore`, and `threads` controls
- Runtime now applies documented `[lint.settings]` options, including compiler version ranges, rule thresholds, Foundry test naming patterns, and max line length
- VS Code `solgrid.configPath` now reloads explicit configs on initialize and config changes, and the Prettier plugin aligns `solidityContractBodySpacing` / `solidityInheritanceBraceNewLine`
- `docs/rules.md` is now generated from `solgrid list-rules` and verified in CI so the published rule reference stays in sync

### Deprecated
- Treat legacy NatSpec rule IDs such as `best-practices/use-natspec` as deprecated config aliases for `docs/natspec` and keep `solidityContractNewLines` as a deprecated Prettier alias for `solidityContractBodySpacing = "single"`

### Fixed
- Fixed duplicate NatSpec and custom-error diagnostics by making `docs/*` the canonical NatSpec home and only running `gas/custom-errors` when the best-practices rule is disabled
- Fixed runtime rule-severity fallback to use each rule's declared default severity instead of category-level defaults
- Fixed compiler-version range checks for wide pragma constraints, made config hashing deterministic for cache invalidation, and avoided repeated LSP/CLI config reloads
- Fixed namespace-import autocomplete (`import "./Foo.sol" as Foo; Foo.Bar`) and stale auto-import index entries when files close
- Fixed `check` / `fix` remapping resolution to use each linted file's workspace instead of only the current working directory
- Fixed LSP remapping resolution to use each file's nearest workspace instead of one workspace-wide remapping set
- Fixed file-based remapping discovery to avoid inheriting unrelated current-working-directory remappings
- Fixed `style/prefer-remappings` path matching by canonicalizing remapping targets before prefix comparison
- Fixed `style/prefer-remappings` producing mangled import paths when remapping prefixes omit a trailing slash
- Fixed `style/imports-ordering` safe autofixes to avoid rewriting across separated import blocks or comment-bearing import gaps
- Fixed selector-tag autofixes to skip unresolved custom types, and aligned initialization classification between `style/category-headers` and `style/ordering`
### Removed
- `style/import-path-format` rule (replaced by `style/prefer-remappings`)

## [0.0.4] - 2026-03-19

### Added
- Cross-file hover support: imported symbols (errors, functions, contracts, etc.) now show signature and NatSpec documentation
- Transitive import resolution: hover and go-to-definition now follow re-exported symbols through intermediate files
- Add typed `[lint.settings]` decoding helpers so rules can safely read structured configuration with default fallback
- Add shared AST-side import resolution, symbol table, and NatSpec attachment helpers reused by the linter and language server
- Add `docs/natspec` rule to consolidate NatSpec presence, tag validation, formatting, and triple-slash enforcement
- Add `docs/selector-tags` rule to compute and enforce canonical interface IDs and custom error selectors
- Add `style/category-headers` rule to rebuild contract bodies into canonical declaration sections with standardized headers
- Implement autofix for `style/imports-ordering` rule (sorts import groups alphabetically)
- Implement autofix for `style/contract-layout` rule (reorders contract members by type)
- Implement autofix for `best-practices/visibility-modifier-order` rule (reorders function modifiers)
- Implement autofix for `best-practices/no-unused-imports` rule (removes unused import aliases)
- Implement autofix for `gas/use-constant` rule (adds `constant` modifier)
- Implement autofix for `gas/use-immutable` rule (adds `immutable` modifier)
- Implement autofix for `style/func-order` rule (reorders functions by visibility)
- Implement autofix for `style/ordering` rule (reorders top-level declarations)
- Implement autofix for `style/import-path-format` rule (converts import paths to consistent format)

### Changed
- Expand `style/imports-ordering` to support grouped ordering, regex-configured import groups, spacing-only fixes, and quote normalization on full rewrites
- Rewrite `style/ordering` as the single declaration-order rule for file-level and contract-level scopes, including initialization and mutability ordering
- Replace the fragmented NatSpec and layout/order rule registrations with consolidated `docs/natspec`, `docs/selector-tags`, `style/category-headers`, `style/ordering`, and `style/imports-ordering`
- Remove overlapping NatSpec rules (`best-practices/use-natspec`, `best-practices/natspec-params`, `best-practices/natspec-returns`, `docs/natspec-contract`, `docs/natspec-interface`, `docs/natspec-function`, `docs/natspec-event`, `docs/natspec-error`, `docs/natspec-param-mismatch`) from the active registry
- Remove overlapping style rules (`style/func-order`, `style/contract-layout`) from the active registry

### Fixed
- Fix `security/state-visibility` diagnostic span covering initializer values instead of just the declaration
- Fix `gas/bool-storage` diagnostic span highlighting leading whitespace instead of the `bool` keyword
- Fix autofix regressions in modifier ordering, unused import cleanup, function ordering, and import path normalization
- Add regression coverage for consolidated NatSpec, selector, ordering, import grouping, and rule-settings behaviors
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

[Unreleased]: https://github.com/TateB/solgrid/compare/v0.0.10...HEAD
[0.0.10]: https://github.com/TateB/solgrid/compare/v0.0.9...v0.0.10
[0.0.9]: https://github.com/TateB/solgrid/releases/tag/v0.0.9
[0.0.8]: https://github.com/TateB/solgrid/releases/tag/v0.0.8
[0.0.7]: https://github.com/TateB/solgrid/releases/tag/v0.0.7
[0.0.6]: https://github.com/TateB/solgrid/releases/tag/v0.0.6
[0.0.5]: https://github.com/TateB/solgrid/releases/tag/v0.0.5
[0.0.4]: https://github.com/TateB/solgrid/releases/tag/v0.0.4
[0.0.3]: https://github.com/TateB/solgrid/releases/tag/v0.0.3
[0.0.2]: https://github.com/TateB/solgrid/releases/tag/v0.0.2
[0.0.1]: https://github.com/TateB/solgrid/releases/tag/v0.0.1
