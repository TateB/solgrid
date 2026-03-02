# solgrid — Project TODO

## Completed

### Chunk 1: Architecture & Design
- [x] Research forge fmt, solhint, prettier-plugin-solidity
- [x] Write ARCHITECTURE.md with full feature specification
- [x] Define 90-rule set across 6 categories
- [x] Design 3-component architecture (linter core, prettier plugin, VSCode extension)
- [x] Define 4-phase roadmap

### Chunk 2: Workspace & Foundation Crates
- [x] Create Cargo workspace with 8 member crates
- [x] Implement `solgrid_diagnostics` — core types (Severity, Diagnostic, Fix, TextEdit, RuleMeta, FileResult, apply_fixes)
- [x] Implement `solgrid_parser` — Solar parser wrapper (with_parsed_ast, with_parsed_ast_sequential, check_syntax)
- [x] Implement `solgrid_ast` — AST helpers (span_to_range, span_text, is_pascal_case, is_camel_case, is_upper_snake_case, is_member_access, is_member_call)
- [x] Implement `solgrid_config` — config parsing (Config, LintConfig, FormatConfig, GlobalConfig, resolve_config, find_config_file)

### Chunk 3: Rule Engine & 12 Starter Rules
- [x] Implement `solgrid_linter` core (Rule trait, LintContext, RuleRegistry, LintEngine, suppression system)
- [x] Security rules (6): tx-origin, avoid-sha3, avoid-suicide, state-visibility, no-inline-assembly, low-level-calls
- [x] Best Practices rules (3): no-console, explicit-types, no-empty-blocks
- [x] Naming rules (3): contract-name-capwords, func-name-mixedcase, const-name-snakecase
- [x] Three-tier auto-fix system (safe/suggestion/dangerous)
- [x] Inline suppression comments (solgrid-disable-next-line, solgrid-disable-line, block disable/enable, solhint compat)

### Chunk 4: CLI & Testing
- [x] Implement CLI binary with clap (check, fix, fmt, list-rules, explain subcommands)
- [x] Text output with colored diagnostics
- [x] JSON output format
- [x] File discovery with gitignore support (ignore crate)
- [x] Parallel file processing (rayon)
- [x] Implement `solgrid_testing` — test utilities (lint_source, lint_source_for_rule, fix_source, assert_diagnostic_count)
- [x] Implement `solgrid_formatter` stub (Phase 1: validate syntax, return as-is)
- [x] 21 integration tests — all passing
- [x] End-to-end verification (check, fix, list-rules, explain, JSON output)
- [x] .gitignore for build artifacts

---

## Remaining Work

### Chunk 5: Security Rules Expansion (13 rules)
_Add remaining security rules from ARCHITECTURE.md._

- [x] `security/reentrancy` — detect state changes after external calls (CEI violation) — heuristic linear scan
- [x] `security/avoid-selfdestruct` — flag `selfdestruct` (deprecated in Solidity 0.8.18+)
- [x] `security/compiler-version` — require specific or minimum compiler version (+ auto-fix suggestion)
- [x] `security/not-rely-on-block-hash` — avoid `blockhash()` for randomness
- [x] `security/not-rely-on-time` — avoid `block.timestamp` / `now` for critical logic
- [x] `security/multiple-sends` — flag multiple `send()` calls in one function
- [x] `security/payable-fallback` — require payable on fallback/receive (+ auto-fix suggestion)
- [x] `security/no-delegatecall-in-loop` — flag `delegatecall` inside loops
- [x] `security/unchecked-transfer` — flag ERC20 transfer without return check
- [x] `security/msg-value-in-loop` — flag `msg.value` access inside loops
- [x] `security/arbitrary-send-eth` — flag send/transfer/call to user-controlled address
- [x] `security/uninitialized-storage` — detect uninitialized storage pointers (text heuristic)
- [x] `security/divide-before-multiply` — detect precision loss from division before multiplication

### Chunk 6: Best Practices Rules Expansion (19 rules)
_Add remaining best practices rules from ARCHITECTURE.md._

- [x] `best-practices/no-unused-vars` — detect unused local variables (text search heuristic + suggestion fix)
- [x] `best-practices/no-unused-imports` — detect unused imports (text search heuristic)
- [x] `best-practices/no-unused-state` — detect unused state variables (text search heuristic)
- [x] `best-practices/code-complexity` — flag functions exceeding cyclomatic complexity threshold
- [x] `best-practices/function-max-lines` — flag functions exceeding line count (default: 50)
- [x] `best-practices/max-states-count` — flag contracts with too many state variables (default: 15)
- [x] `best-practices/one-contract-per-file` — enforce one contract/interface/library per file
- [x] `best-practices/no-global-import` — disallow `import "file.sol"` (+ auto-fix suggestion)
- [x] `best-practices/constructor-syntax` — use `constructor` keyword (+ auto-fix safe)
- [x] `best-practices/use-natspec` — require NatSpec on public/external functions
- [x] `best-practices/natspec-params` — NatSpec @param for every function parameter
- [x] `best-practices/natspec-returns` — NatSpec @return for every return value
- [x] `best-practices/reason-string` — require reason strings in require/revert
- [x] `best-practices/custom-errors` — prefer custom errors over require with string (+ suggestion fix)
- [x] `best-practices/no-floating-pragma` — disallow floating pragma (+ safe fix)
- [x] `best-practices/imports-on-top` — all imports must be at the top
- [x] `best-practices/visibility-modifier-order` — enforce Solidity style guide order (+ safe fix)
- [x] `best-practices/no-unused-error` — detect declared but unused custom errors (+ suggestion fix)
- [x] `best-practices/no-unused-event` — detect declared but unused events (+ suggestion fix)

### Chunk 7: Naming Rules Expansion (13 rules)
_Add remaining naming rules from ARCHITECTURE.md._

- [x] `naming/interface-starts-with-i` — interface names must start with `I`
- [x] `naming/library-name-capwords` — libraries must use CapWords
- [x] `naming/struct-name-capwords` — structs must use CapWords
- [x] `naming/enum-name-capwords` — enums must use CapWords
- [x] `naming/event-name-capwords` — events must use CapWords
- [x] `naming/error-name-capwords` — custom errors must use CapWords
- [x] `naming/param-name-mixedcase` — function parameters must use mixedCase
- [x] `naming/var-name-mixedcase` — local variables must use mixedCase
- [x] `naming/immutable-name-snakecase` — immutable variables must use UPPER_SNAKE_CASE
- [x] `naming/private-vars-underscore` — private/internal state vars must start with `_`
- [x] `naming/modifier-name-mixedcase` — modifiers must use mixedCase
- [x] `naming/type-name-capwords` — user-defined value types must use CapWords
- [x] `naming/foundry-test-functions` — test functions must match test/testFuzz/testFail patterns

### Chunk 8: Gas Optimization Rules (15 rules)
_New rule category — all rules from ARCHITECTURE.md._

- [x] Create `rules/gas/` module
- [x] `gas/calldata-parameters` — use `calldata` instead of `memory` for read-only external params (+ safe fix)
- [x] `gas/custom-errors` — custom errors cheaper than require with string (+ suggestion fix)
- [x] `gas/increment-by-one` — use `++i` instead of `i += 1` (+ safe fix)
- [x] `gas/indexed-events` — index event parameters for cheaper filtering (+ suggestion fix)
- [x] `gas/named-return-values` — named return values avoid a stack variable (+ suggestion fix)
- [x] `gas/small-strings` — short strings in require/revert save gas
- [x] `gas/struct-packing` — reorder struct fields for optimal storage packing (+ suggestion fix)
- [x] `gas/cache-array-length` — cache `array.length` outside loops (+ safe fix)
- [x] `gas/use-immutable` — variables assigned only in constructor should be `immutable` (+ suggestion fix)
- [x] `gas/use-constant` — compile-time-known values should be `constant` (+ suggestion fix)
- [x] `gas/unchecked-increment` — loop counters can use `unchecked { ++i; }` (+ safe fix)
- [x] `gas/no-redundant-sload` — cache state variable reads used multiple times (+ suggestion fix)
- [x] `gas/bool-storage` — `bool` in storage costs more than `uint256`
- [x] `gas/tight-variable-packing` — pack adjacent storage variables (+ suggestion fix)
- [x] `gas/use-bytes32` — use `bytes32` instead of `string` for short fixed strings (+ suggestion fix)

### Chunk 9: Style & Documentation Rules (18 rules)
_Two new rule categories — all rules from ARCHITECTURE.md._

- [x] Create `rules/style/` module
- [x] `style/func-order` — enforce function ordering per Solidity style guide (+ suggestion fix)
- [x] `style/ordering` — enforce top-level declaration order (+ suggestion fix)
- [x] `style/imports-ordering` — sort imports alphabetically (+ safe fix)
- [x] `style/max-line-length` — maximum line length (default: 120)
- [x] `style/no-trailing-whitespace` — no trailing whitespace (+ safe fix)
- [x] `style/eol-last` — require newline at end of file (+ safe fix)
- [x] `style/no-multiple-empty-lines` — max consecutive empty lines (+ safe fix)
- [x] `style/contract-layout` — enforce ordering within contracts (+ suggestion fix)
- [x] `style/import-path-format` — enforce relative or absolute import paths (+ suggestion fix)
- [x] `style/file-name-format` — file names must match primary contract name
- [x] Create `rules/docs/` module
- [x] `docs/natspec-contract` — contracts must have @title and @author NatSpec
- [x] `docs/natspec-interface` — public interfaces must have NatSpec on all functions
- [x] `docs/natspec-function` — external/public functions must have @notice NatSpec
- [x] `docs/natspec-event` — events must have NatSpec
- [x] `docs/natspec-error` — custom errors must have NatSpec
- [x] `docs/natspec-modifier` — modifiers must have NatSpec
- [x] `docs/natspec-param-mismatch` — NatSpec @param names must match actual parameters (+ safe fix)
- [x] `docs/license-identifier` — file must contain SPDX license identifier

### Chunk 10: Built-in Formatter (Phase 2)
_Replace the Phase 1 stub with the full chunk-based IR formatter._

- [x] Design chunk-based format IR (FormatChunk: Text, Line, HardLine, Group, Indent, Comment)
- [x] Implement line-fitting algorithm (Wadler-Lindig style)
- [x] Implement comment extraction and reattachment
- [x] Format pragma declarations
- [x] Format import statements (+ sort_imports option)
- [x] Format contract declarations (+ contract_new_lines option)
- [x] Format function declarations (+ multiline_func_header option)
- [x] Format variable declarations (+ uint_type, number_underscore options)
- [x] Format expressions and statements
- [x] Format string literals (+ single_quote option)
- [x] Format spacing (+ bracket_spacing, override_spacing options)
- [x] Implement formatter directives (solgrid-fmt: off/on, forgefmt: disable-next-line)
- [x] Idempotency verification (format(format(x)) == format(x))
- [x] Integration tests against corpus of Solidity files

### Chunk 11: Incremental Caching & Extra Output Formats
_Performance and CI integration features._

- [x] Create `solgrid_cache` crate
- [x] Implement content-hash-based file cache
- [x] Cache invalidation on config change or solgrid version upgrade
- [x] Implement `--no-cache` flag
- [x] GitHub Actions output format (`::error file=...`)
- [x] SARIF output format (OASIS SARIF 2.1 for CodeQL etc.)
- [x] `solgrid migrate --from solhint` command (read .solhint.json, write solgrid.toml)
- [x] Foundry.toml fallback (read `[fmt]` section when no solgrid.toml found)
- [x] `--stdin` support (read from stdin, write to stdout)

### Chunk 12: LSP Server & VSCode Extension (Phase 3)
_Editor integration._

- [ ] Create `solgrid_server` crate with `tower-lsp` dependency
- [ ] Implement textDocument/publishDiagnostics (real-time lint as user types)
- [ ] Implement textDocument/codeAction (quick-fixes grouped by safety tier)
- [ ] Implement textDocument/formatting (full-document formatting)
- [ ] Implement textDocument/rangeFormatting (format selection)
- [ ] Implement fix-on-save + format-on-save
- [ ] Implement textDocument/hover (rule documentation on hover)
- [ ] Implement suppression comment completion
- [ ] Implement workspace/configuration (read solgrid.toml)
- [ ] Create VSCode extension TypeScript client (vscode-languageclient)
- [ ] Extension settings UI (fixOnSave, formatOnSave, solgrid.path)
- [ ] Diagnostic presentation (severity icons, clickable rule IDs, disable actions)
- [ ] Publish to VS Marketplace + Open VSX Registry

### Chunk 13: Prettier Plugin (Phase 4)
_Prettier compatibility._

- [ ] Create `solgrid_napi` crate with NAPI-RS bindings
- [ ] Implement parse() binding — return opaque AST handle
- [ ] Implement format() binding — map Prettier options to solgrid options
- [ ] Create `prettier-plugin-solgrid` npm package
- [ ] Implement Prettier parsers + printers API
- [ ] Prettier option mapping (printWidth, tabWidth, useTabs, singleQuote, bracketSpacing)
- [ ] Conformance test suite (solgrid vs prettier-plugin-solidity output comparison)
- [ ] npm publish workflow

### Chunk 14: WASM, Performance & v1.0
_Final polish._

- [ ] Create `solgrid_wasm` crate (web playground, browser use)
- [ ] Performance optimization pass (benchmarks, profiling)
- [ ] Binary size optimization (strip, LTO)
- [ ] Startup time optimization (< 10ms target)
- [ ] Memory usage optimization (< 200MB for 1000 files target)
- [ ] Cold lint benchmark (< 500ms for 500 files target)
- [ ] Comprehensive documentation
- [ ] CI/CD setup (GitHub Actions: build, test, release)
- [ ] v1.0 release

---

## Summary

| Chunk | Status | Rules Added | Description |
|-------|--------|-------------|-------------|
| 1. Architecture & Design | Done | — | ARCHITECTURE.md |
| 2. Workspace & Foundation | Done | — | 8 crates scaffolded |
| 3. Rule Engine & 12 Rules | Done | 12 | Core engine + starter rules |
| 4. CLI & Testing | Done | — | Full CLI + 21 tests |
| 5. Security Expansion | **Done** | +13 | All 13 security rules |
| 6. Best Practices Expansion | **Done** | +19 | All 19 best practices rules |
| 7. Naming Expansion | **Done** | +13 | All 13 naming rules |
| 8. Gas Rules | **Done** | +15 | New category |
| 9. Style & Docs Rules | **Done** | +18 | Two new categories |
| 10. Formatter | **Done** | — | Full chunk-based formatter |
| 11. Caching & CI Formats | **Done** | — | Cache, SARIF, GitHub output |
| 12. LSP & VSCode | TODO | — | Editor integration |
| 13. Prettier Plugin | TODO | — | npm plugin |
| 14. WASM & v1.0 | TODO | — | Final polish |

**Current state:** 90/90 rules implemented, full formatter, caching, SARIF/GitHub output, working CLI, 258 tests passing.
