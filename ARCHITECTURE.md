# solgrid Architecture

> The Ruff for Solidity — a blazing-fast, Rust-native linter and formatter for Solidity smart contracts.

---

## Table of Contents

1. [Project Vision](#1-project-vision)
2. [Architecture Overview](#2-architecture-overview)
3. [Linter Core](#3-linter-core)
4. [Complete Rule Set](#4-complete-rule-set)
5. [Formatter](#5-formatter)
6. [Prettier Plugin](#6-prettier-plugin)
7. [VSCode Extension](#7-vscode-extension)
8. [Configuration](#8-configuration)
9. [CLI Interface](#9-cli-interface)
10. [Performance Goals](#10-performance-goals)
11. [Project Roadmap](#11-project-roadmap)

---

## 1. Project Vision

**solgrid** is a unified Solidity linter, formatter, and language server written in Rust. It replaces the fragmented toolchain of solhint (slow, JavaScript, limited auto-fix), forge fmt (formatting only, no lint rules), and prettier-plugin-solidity (formatting only, Node.js runtime) with a single, fast, comprehensive tool.

### Why solgrid?

| Problem | Current State | solgrid Solution |
|---|---|---|
| **Speed** | solhint is JavaScript — startup and parse time dominate on large repos | Rust + Solar parser: sub-second linting on entire projects |
| **Coverage** | solhint has ~40 rules, limited auto-fix (~9 rules); forge fmt is formatting only | 90+ lint rules, built-in formatter, three-tier auto-fix on the majority of rules |
| **Fragmentation** | Developers run solhint + forge fmt + prettier separately with conflicting configs | One tool, one config file, one pass |
| **Editor integration** | solhint's VSCode extension is slow and limited | Native LSP server with real-time diagnostics, code actions, and fix-on-save |
| **Extensibility** | solhint plugins are npm packages requiring Node.js | Designed for future native plugin system; rule engine is modular from day one |

### Design Principles

1. **Correctness first.** Every rule must have a clear specification, comprehensive test suite, and zero false positives on well-formed Solidity.
2. **Speed is a feature.** Linting an entire Foundry project should complete in under one second. The tool should never be the bottleneck in a developer's workflow.
3. **Batteries included.** Ship with every rule a Solidity developer needs. No plugin installation for standard use cases.
4. **Compatible by default.** Output matches prettier-plugin-solidity formatting. Config is a superset of foundry.toml formatting options. Migration from solhint is a single command.
5. **Fix, don't just warn.** Every rule that can be auto-fixed should be. Fixes are categorized by safety level so developers stay in control.

---

## 2. Architecture Overview

solgrid is composed of three deployable components built from a shared Rust workspace:

```
┌─────────────────────────────────────────────────────┐
│                  solgrid Workspace                  │
│                                                     │
│  ┌────────────────┐  ┌────────────────┐             │
│  │ solgrid_parser │  │  solgrid_ast   │             │
│  │ (Solar bridge) │  │ (AST utilities)│             │
│  └───────┬────────┘  └───────┬────────┘             │
│          │                   │                      │
│          └────────┬──────────┘                      │
│                   │                                 │
│          ┌────────▼────────┐                        │
│          │ solgrid_linter  │                        │
│          │  (Rule engine)  │                        │
│          └────────┬────────┘                        │
│                   │                                 │
│     ┌─────────────┼─────────────┐                   │
│     │             │             │                   │
│  ┌──▼───┐   ┌────▼────┐  ┌────▼────────┐           │
│  │format│   │   cli   │  │   server    │           │
│  │      │   │         │  │   (LSP)     │           │
│  └──┬───┘   └────┬────┘  └────┬────────┘           │
│     │            │             │                    │
└─────┼────────────┼─────────────┼────────────────────┘
      │            │             │
      ▼            ▼             ▼
  prettier      solgrid       VSCode
  plugin        binary       Extension
  (npm)                    (TypeScript)
```

### Component Summary

| Component | Language | Deployment | Purpose |
|---|---|---|---|
| **Linter Core** | Rust (multi-crate workspace) | Library + CLI binary | Parse, lint, format, auto-fix Solidity files |
| **Prettier Plugin** | TypeScript + NAPI-RS bindings | npm package | Run solgrid's formatter as a Prettier plugin |
| **VSCode Extension** | TypeScript client + Rust LSP server | VS Marketplace | Real-time diagnostics, code actions, format-on-save |

---

## 3. Linter Core

### 3.1 Crate Structure

The workspace follows the multi-crate pattern established by Ruff and Oxc. Each crate has a single responsibility and a well-defined public API.

```
crates/
  solgrid/              # Binary crate — CLI entry point
  solgrid_ast/          # AST utilities, semantic helpers, symbol table
  solgrid_cache/        # Incremental analysis cache
  solgrid_config/       # Config file parsing (solgrid.toml)
  solgrid_diagnostics/  # Diagnostic types, severity, reporting
  solgrid_formatter/    # Built-in Solidity formatter
  solgrid_linter/       # Rule engine, rule registry, violation types
  solgrid_napi/         # NAPI-RS bindings for Node.js (prettier plugin)
  solgrid_parser/       # Thin wrapper around Solar parser
  solgrid_server/       # LSP server implementation
  solgrid_testing/      # Snapshot test infrastructure, test utilities
  solgrid_wasm/         # WASM build target (playground, browser use)
```

### 3.2 Parser: Solar

solgrid uses [**Solar**](https://github.com/paradigmxyz/solar) (`paradigmxyz/solar`) as its parser. Solar is:

- Written in Rust, derived from rustc's parser architecture
- 41x faster than solc at parsing
- Produces a typed AST with `Visit` and `VisitMut` traits for traversal
- Uses arena allocation (`bumpalo`) for zero-copy, cache-friendly AST nodes
- Actively maintained by Paradigm (Foundry ecosystem)

The `solgrid_parser` crate provides a thin abstraction over Solar:

```rust
pub use solar_parse::Parser;
pub use solar_ast::*;

/// Parse a Solidity source file into an AST.
/// Returns the SourceUnit and any recovered diagnostics.
pub fn parse_source(source: &str, path: &Path) -> ParseResult<SourceUnit> {
    // ...
}
```

The wrapper exists to (a) insulate the rest of the codebase from Solar API changes, (b) add solgrid-specific comment extraction, and (c) provide a unified error type.

### 3.3 Rule Engine

Rules are the heart of solgrid. The engine is modeled after Clippy and Ruff.

**Rule declaration macro:**

```rust
declare_solgrid_rule!(
    /// Detects functions that use `tx.origin` for authorization.
    ///
    /// ## Why is this bad?
    /// `tx.origin` returns the original sender of the transaction, which can
    /// be exploited in phishing attacks where a malicious contract calls
    /// the victim contract on behalf of the user.
    ///
    /// ## Example
    /// ```solidity
    /// // Bad
    /// require(tx.origin == owner);
    ///
    /// // Good
    /// require(msg.sender == owner);
    /// ```
    pub TxOrigin,
    security,
    "use of `tx.origin` for authorization",
    Fix::Safe
);
```

**Rule trait:**

```rust
pub trait Rule: Send + Sync {
    /// Rule metadata: name, category, default severity, docs.
    fn meta(&self) -> &RuleMeta;

    /// Run the rule against a parsed source file.
    /// Produces zero or more diagnostics, each optionally carrying a Fix.
    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic>;
}
```

**Lint context:**

The `LintContext` provides rules with everything they need without requiring them to manage their own AST traversal:

```rust
pub struct LintContext<'a> {
    /// The parsed AST.
    pub ast: &'a SourceUnit<'a>,
    /// Source text (for span-to-text lookups).
    pub source: &'a str,
    /// Extracted comments with positions.
    pub comments: &'a [Comment],
    /// Semantic information: symbol table, type info, scopes.
    pub semantic: &'a SemanticModel<'a>,
    /// Active configuration for this file.
    pub config: &'a ResolvedConfig,
    /// The file path.
    pub path: &'a Path,
}
```

**Two-pass architecture:**

1. **Early pass (syntactic):** Operates on the raw AST. Catches naming violations, style issues, simple pattern matches. Most rules live here. Runs in parallel across files.
2. **Late pass (semantic):** Operates after semantic analysis (symbol resolution, type inference, scope analysis). Required for rules like `no-unused-vars`, `reentrancy`, `state-visibility` inference. Runs after the early pass.

### 3.4 Three-Tier Auto-Fix System

Every fixable rule categorizes its fix into one of three safety tiers, following the model established by oxlint:

| Tier | Name | Behavior | Example |
|---|---|---|---|
| 1 | **Safe** | Applied with `--fix`. Guaranteed to preserve semantics. | Rename `uint` → `uint256` |
| 2 | **Suggestion** | Applied with `--fix --unsafe-fixes`. Likely correct but may change semantics in edge cases. | Reorder imports, add `indexed` to event params |
| 3 | **Dangerous** | Shown as editor code actions only. Requires manual confirmation. | Remove unused state variable, replace `tx.origin` |

Fix representation:

```rust
pub struct Fix {
    /// Safety tier.
    pub safety: FixSafety,
    /// The text edits to apply.
    pub edits: Vec<TextEdit>,
    /// Human-readable description of what the fix does.
    pub message: String,
}

pub struct TextEdit {
    /// Byte range in the original source to replace.
    pub range: Range<usize>,
    /// Replacement text (empty string = deletion).
    pub replacement: String,
}
```

Fixes are applied in a single pass after all rules have run, with conflict detection: if two fixes overlap the same span, neither is applied, and both diagnostics are reported without fixes.

### 3.5 Inline Suppression

solgrid supports inline suppression comments, compatible with existing conventions and introducing its own:

```solidity
// solgrid-disable-next-line security/tx-origin
require(tx.origin == owner);

require(tx.origin == owner); // solgrid-disable-line security/tx-origin

// solgrid-disable security/tx-origin
// ... block of code ...
// solgrid-enable security/tx-origin

// Also supports blanket disable:
// solgrid-disable-next-line
```

For migration convenience, solgrid also recognizes `solhint-disable` and `forgefmt: disable` comments (controlled by a config flag).

---

## 4. Complete Rule Set

solgrid ships with 90+ rules organized into six categories. Each rule has a unique identifier in the form `category/rule-name`.

### 4.1 Security Rules

Rules that detect vulnerabilities, unsafe patterns, and potential exploits. Default severity: **error**.

| Rule ID | Description | Auto-fix | Origin |
|---|---|---|---|
| `security/tx-origin` | Disallow `tx.origin` for authorization | Dangerous | solhint |
| `security/reentrancy` | Detect state changes after external calls (CEI violation) | — | solhint |
| `security/avoid-sha3` | Use `keccak256()` instead of deprecated `sha3()` | Safe | solhint |
| `security/avoid-suicide` | Use `selfdestruct()` instead of deprecated `suicide()` | Safe | solhint |
| `security/avoid-selfdestruct` | Flag `selfdestruct` (deprecated in Solidity 0.8.18+) | — | new |
| `security/compiler-version` | Require specific or minimum compiler version | Suggestion | solhint |
| `security/no-inline-assembly` | Flag inline assembly usage | — | solhint |
| `security/not-rely-on-block-hash` | Avoid `blockhash()` for randomness | — | solhint |
| `security/not-rely-on-time` | Avoid `block.timestamp` / `now` for critical logic | — | solhint |
| `security/state-visibility` | Require explicit visibility on all state variables | Safe | solhint |
| `security/multiple-sends` | Flag multiple `send()` calls in one function | — | solhint |
| `security/low-level-calls` | Flag `call`, `delegatecall`, `staticcall` usage | — | solhint |
| `security/payable-fallback` | Require payable on fallback/receive or flag unintended | Suggestion | solhint |
| `security/no-delegatecall-in-loop` | Flag `delegatecall` inside loops | — | new |
| `security/unchecked-transfer` | Flag ERC20 `transfer`/`transferFrom` without return check | Suggestion | new |
| `security/msg-value-in-loop` | Flag `msg.value` access inside loops | — | new |
| `security/arbitrary-send-eth` | Flag `send`/`transfer`/`call` to user-controlled address | — | new |
| `security/uninitialized-storage` | Detect uninitialized storage pointers | — | new |
| `security/divide-before-multiply` | Detect precision loss from division before multiplication | — | new |

### 4.2 Best Practices Rules

Rules that enforce community-accepted patterns and defensive coding. Default severity: **warning**.

| Rule ID | Description | Auto-fix | Origin |
|---|---|---|---|
| `best-practices/explicit-types` | Use `uint256` not `uint`, `int256` not `int` | Safe | solhint |
| `best-practices/no-empty-blocks` | Disallow empty blocks (excluding receive/fallback) | — | solhint |
| `best-practices/no-unused-vars` | Detect unused local variables (semantic) | Suggestion | solhint |
| `best-practices/no-unused-imports` | Detect unused imports (semantic) | Safe | solhint |
| `best-practices/no-unused-state` | Detect unused state variables (semantic) | Dangerous | new |
| `best-practices/no-console` | Remove `console.log` / `console2.log` statements | Safe | solhint |
| `best-practices/code-complexity` | Flag functions exceeding cyclomatic complexity threshold | — | solhint |
| `best-practices/function-max-lines` | Flag functions exceeding line count (default: 50) | — | solhint |
| `best-practices/max-states-count` | Flag contracts with too many state variables (default: 15) | — | solhint |
| `best-practices/one-contract-per-file` | Enforce one contract/interface/library per file | — | solhint |
| `best-practices/no-global-import` | Disallow `import "file.sol"` — require named imports | Suggestion | solhint |
| `best-practices/constructor-syntax` | Use `constructor` keyword, not old-style named constructors | Safe | solhint |
| `best-practices/use-natspec` | Require NatSpec on public/external functions and contracts | — | new |
| `best-practices/natspec-params` | NatSpec `@param` must exist for every function parameter | — | new |
| `best-practices/natspec-returns` | NatSpec `@return` must exist for every return value | — | new |
| `best-practices/reason-string` | Require reason strings in `require` / `revert` | — | solhint |
| `best-practices/custom-errors` | Prefer custom errors over `require` with string | Suggestion | solhint |
| `best-practices/no-floating-pragma` | Disallow floating pragma (`^`, `>=`) in non-library code | Safe | new |
| `best-practices/imports-on-top` | All imports must be at the top of the file | — | solhint |
| `best-practices/visibility-modifier-order` | Enforce Solidity style guide order for function modifiers | Safe | solhint |
| `best-practices/no-unused-error` | Detect declared but unused custom errors | Suggestion | new |
| `best-practices/no-unused-event` | Detect declared but unused events | Suggestion | new |

### 4.3 Naming Convention Rules

Rules that enforce naming conventions per the Solidity style guide. Default severity: **warning**.

| Rule ID | Description | Auto-fix | Origin |
|---|---|---|---|
| `naming/contract-name-capwords` | Contracts must use CapWords (PascalCase) | Suggestion | solhint |
| `naming/interface-starts-with-i` | Interface names must start with `I` | Suggestion | solhint |
| `naming/library-name-capwords` | Libraries must use CapWords | Suggestion | solhint |
| `naming/struct-name-capwords` | Structs must use CapWords | Suggestion | solhint |
| `naming/enum-name-capwords` | Enums must use CapWords | Suggestion | solhint |
| `naming/event-name-capwords` | Events must use CapWords | Suggestion | solhint |
| `naming/error-name-capwords` | Custom errors must use CapWords | Suggestion | new |
| `naming/func-name-mixedcase` | Functions must use mixedCase (camelCase) | Suggestion | solhint |
| `naming/param-name-mixedcase` | Function parameters must use mixedCase | Suggestion | solhint |
| `naming/var-name-mixedcase` | Local variables must use mixedCase | Suggestion | solhint |
| `naming/const-name-snakecase` | Constants must use UPPER_SNAKE_CASE | Suggestion | solhint |
| `naming/immutable-name-snakecase` | Immutable variables must use UPPER_SNAKE_CASE | Suggestion | solhint |
| `naming/private-vars-underscore` | Private/internal state variables must start with `_` | Suggestion | solhint |
| `naming/modifier-name-mixedcase` | Modifiers must use mixedCase | Suggestion | new |
| `naming/type-name-capwords` | User-defined value types must use CapWords | Suggestion | new |
| `naming/foundry-test-functions` | Test functions must match `test`, `testFuzz`, `testFail` patterns | Suggestion | solhint |

### 4.4 Gas Optimization Rules

Rules that flag patterns with unnecessary gas cost. Default severity: **info**.

| Rule ID | Description | Auto-fix | Origin |
|---|---|---|---|
| `gas/calldata-parameters` | Use `calldata` instead of `memory` for read-only external params | Safe | solhint |
| `gas/custom-errors` | Custom errors are cheaper than `require` with string | Suggestion | solhint |
| `gas/increment-by-one` | Use `++i` instead of `i += 1` or `i = i + 1` | Safe | solhint |
| `gas/indexed-events` | Index event parameters (up to 3) for cheaper filtering | Suggestion | solhint |
| `gas/named-return-values` | Named return values avoid a stack variable | Suggestion | solhint |
| `gas/small-strings` | Short strings (< 32 bytes) in require/revert save gas | Suggestion | solhint |
| `gas/struct-packing` | Reorder struct fields for optimal storage packing | Suggestion | solhint |
| `gas/cache-array-length` | Cache `array.length` outside of loops | Safe | new |
| `gas/use-immutable` | Variables assigned only in constructor should be `immutable` | Suggestion | new |
| `gas/use-constant` | Variables with compile-time-known values should be `constant` | Suggestion | new |
| `gas/unchecked-increment` | Loop counters can use `unchecked { ++i; }` (0.8+) | Safe | new |
| `gas/no-redundant-sload` | Cache state variable reads used multiple times in a function | Suggestion | new |
| `gas/bool-storage` | `bool` in storage costs more than `uint256` | — | new |
| `gas/tight-variable-packing` | Pack adjacent storage variables to fit in 32-byte slots | Suggestion | new |
| `gas/use-bytes32` | Use `bytes32` instead of `string` for short fixed strings | Suggestion | new |

### 4.5 Style Rules

Rules enforcing code layout and consistency. These complement (but do not duplicate) the built-in formatter. Default severity: **info**.

| Rule ID | Description | Auto-fix | Origin |
|---|---|---|---|
| `style/func-order` | Enforce function ordering per Solidity style guide | Suggestion | solhint |
| `style/ordering` | Enforce top-level declaration order (pragma, imports, interfaces, libraries, contracts) | Suggestion | solhint |
| `style/imports-ordering` | Sort imports alphabetically | Safe | solhint |
| `style/max-line-length` | Maximum line length (default: 120) | — | solhint |
| `style/no-trailing-whitespace` | No trailing whitespace | Safe | new |
| `style/eol-last` | Require newline at end of file | Safe | new |
| `style/no-multiple-empty-lines` | Maximum consecutive empty lines (default: 2) | Safe | new |
| `style/contract-layout` | Enforce ordering within contracts (types, state vars, events, errors, modifiers, functions) | Suggestion | new |
| `style/prefer-remappings` | Suggest remapped import paths over relative imports | Suggestion | — |
| `style/file-name-format` | File names must match primary contract name (PascalCase.sol) | — | solhint |

### 4.6 Documentation Rules

Rules enforcing NatSpec and documentation completeness. Default severity: **info**.

| Rule ID | Description | Auto-fix | Origin |
|---|---|---|---|
| `docs/natspec-contract` | Contracts must have `@title` and `@author` NatSpec | — | new |
| `docs/natspec-interface` | Public interfaces must have NatSpec on all functions | — | new |
| `docs/natspec-function` | External/public functions must have `@notice` NatSpec | — | new |
| `docs/natspec-event` | Events must have NatSpec | — | new |
| `docs/natspec-error` | Custom errors must have NatSpec | — | new |
| `docs/natspec-modifier` | Modifiers must have NatSpec | — | new |
| `docs/natspec-param-mismatch` | NatSpec `@param` names must match actual parameter names | Safe | new |
| `docs/license-identifier` | File must contain SPDX license identifier | — | new |

### Rule Counts Summary

| Category | Count | New | Fixable |
|---|---|---|---|
| Security | 19 | 7 | 6 |
| Best Practices | 22 | 8 | 12 |
| Naming | 16 | 4 | 16 |
| Gas Optimization | 15 | 8 | 11 |
| Style | 10 | 4 | 7 |
| Documentation | 8 | 8 | 1 |
| **Total** | **90** | **39** | **53** |

The formatter contributes an additional 15+ formatting options, bringing the total addressable style surface well above 100 checks.

---

## 5. Formatter

solgrid includes a built-in formatter (`solgrid fmt`) that is a first-class component. It draws on the best ideas from forge fmt while producing output compatible with prettier-plugin-solidity.

### 5.1 Formatter Architecture

```
Source Text
    │
    ▼
  Parse (Solar) ──▶ AST + Comments
    │
    ▼
  Format IR (Chunks)
    │
    ▼
  Print (line fitting, indentation) ──▶ Formatted Output
    │
    ▼
  Verify idempotency (debug mode)
```

The intermediate representation uses a **chunk-based model** (inspired by Wadler-Lindig and Prettier's IR) rather than direct string manipulation. This allows the formatter to make line-breaking decisions based on the available width:

```rust
pub enum FormatChunk {
    /// Literal text, printed as-is.
    Text(String),
    /// Soft line break: space if the group fits, newline + indent if not.
    Line,
    /// Hard line break: always a newline.
    HardLine,
    /// A group of chunks that the printer tries to fit on one line.
    Group(Vec<FormatChunk>),
    /// Increase indent level for nested chunks.
    Indent(Vec<FormatChunk>),
    /// A comment, preserved in its original form.
    Comment(CommentKind, String),
}
```

### 5.2 Formatting Options

These options live in `[format]` in `solgrid.toml` and are a superset of both forge fmt and prettier-plugin-solidity options:

| Option | Type | Default | forge fmt | prettier |
|---|---|---|---|---|
| `line_length` | integer | 120 | `line_length` | `printWidth` |
| `tab_width` | integer | 4 | `tab_width` | `tabWidth` |
| `use_tabs` | bool | false | — | `useTabs` |
| `single_quote` | bool | false | `quote_style` | `singleQuote` |
| `bracket_spacing` | bool | false | `bracket_spacing` | `bracketSpacing` |
| `number_underscore` | `"thousands"` / `"remove"` / `"preserve"` | `"preserve"` | `number_underscore` | — |
| `uint_type` | `"uint256"` / `"uint"` / `"preserve"` | `"uint256"` | `int_types` | — |
| `override_spacing` | bool | true | `override_spacing` | — |
| `wrap_comments` | bool | false | `wrap_comments` | — |
| `sort_imports` | bool | false | `sort_imports` | — |
| `imports_granularity` | `"preserve"` / `"item"` / `"file"` | `"preserve"` | — | — |
| `multiline_func_header` | `"attributes_first"` / `"params_first"` / `"all"` | `"attributes_first"` | `multiline_func_header` | — |
| `contract_body_spacing` | `"preserve"` / `"single"` / `"compact"` | `"preserve"` | `contract_new_lines`* | — |
| `inheritance_brace_new_line` | bool | true | — | — |
| `trailing_comma` | bool | false | — | — |
| `blank_lines_between_contracts` | integer | 2 | — | (style guide) |
| `blank_lines_between_functions` | integer | 1 | — | (style guide) |

### 5.3 Comment Handling

Comments are extracted from the token stream during parsing and attached to AST nodes by proximity. The formatter preserves:

- Leading comments (above a node)
- Trailing comments (same line, after a node)
- Inline disable directives (`// solgrid-disable-line`)
- Block comments (`/* ... */`)
- NatSpec comments (`/// ...` and `/** ... */`)

### 5.4 Inline Formatter Directives

```solidity
// solgrid-fmt: off
// ... this code is not formatted ...
// solgrid-fmt: on

// Also supports forge fmt compatibility:
// forgefmt: disable-next-line
```

---

## 6. Prettier Plugin

The prettier plugin (`prettier-plugin-solgrid`) allows teams already using Prettier to adopt solgrid's formatter without changing their workflow.

### 6.1 Architecture

```
prettier CLI / IDE plugin
        │
        ▼
  prettier-plugin-solgrid (npm package)
        │
        ▼
  NAPI-RS bindings (solgrid_napi crate)
        │
        ▼
  solgrid_formatter (Rust)
        │
        ▼
  Formatted output returned to Prettier
```

The plugin implements Prettier's `parsers` and `printers` API:

- **Parser:** Calls into `solgrid_napi` to parse the source. Returns a Prettier-compatible AST wrapper (the actual AST lives in Rust memory; the JS side holds an opaque handle).
- **Printer:** Calls `solgrid_napi.format()` with Prettier's resolved options mapped to solgrid's native formatting options.

### 6.2 Option Mapping

| Prettier Option | solgrid Option |
|---|---|
| `printWidth` | `line_length` |
| `tabWidth` | `tab_width` |
| `useTabs` | `use_tabs` |
| `singleQuote` | `single_quote` |
| `bracketSpacing` | `bracket_spacing` |

### 6.3 Compatibility Guarantee

solgrid's formatter output, when given equivalent options, must match prettier-plugin-solidity's output. This is enforced by a conformance test suite that runs both formatters on a corpus of Solidity files and diffs the results. Deviations are tracked and justified.

---

## 7. VSCode Extension

### 7.1 Architecture

The VSCode extension (`solgrid-vscode`) follows the same architecture as `ruff-vscode` and `oxc-vscode`:

```
VSCode
  │
  ▼
TypeScript Extension Client
  │  (LSP over stdio)
  ▼
solgrid server (Rust binary)
  │
  ▼
solgrid_linter + solgrid_formatter
```

The TypeScript client is thin — it manages extension lifecycle, settings UI, and status bar. All intelligence lives in the Rust LSP server.

### 7.2 LSP Server Capabilities

| LSP Feature | solgrid Behavior |
|---|---|
| `textDocument/publishDiagnostics` | Real-time lint diagnostics as the user types (debounced) |
| `textDocument/codeAction` | Quick-fixes for all fixable rules, organized by safety tier |
| `textDocument/formatting` | Full-document formatting via solgrid's formatter |
| `textDocument/rangeFormatting` | Format selection |
| `textDocument/onSave` | Auto-fix safe fixes + format on save (configurable) |
| `textDocument/hover` | Rule documentation on hover over a diagnostic |
| `textDocument/completion` | Inline suppression comment completion (`// solgrid-disable...`) |
| `workspace/configuration` | Read `solgrid.toml` from workspace root |

### 7.3 Fix-on-Save Behavior

Configurable via VSCode settings:

```jsonc
{
  // Apply safe auto-fixes on save
  "solgrid.fixOnSave": true,
  // Also apply suggestion-level fixes on save
  "solgrid.fixOnSave.unsafeFixes": false,
  // Format on save (uses solgrid's formatter)
  "solgrid.formatOnSave": true,
  // Path to solgrid binary (auto-detected if on PATH)
  "solgrid.path": null
}
```

### 7.4 Diagnostic Presentation

Diagnostics include:

- Severity icon (error / warning / info / hint)
- Rule ID as a clickable link to documentation (e.g., `security/tx-origin`)
- Source marked as `solgrid`
- Quick-fix code actions grouped by safety tier
- "Disable rule for this line" and "Disable rule for this file" actions

### 7.5 Extension Technology

- **Client:** TypeScript, using `vscode-languageclient`
- **Bundler:** esbuild
- **Distribution:** VS Marketplace + Open VSX Registry
- **Activation:** On `.sol` file open or workspace containing `solgrid.toml`

---

## 8. Configuration

### 8.1 Config File: `solgrid.toml`

solgrid uses TOML for configuration, following the Foundry ecosystem convention. Config is discovered by walking up from the file being linted to the filesystem root.

```toml
# solgrid.toml

[lint]
# Rule selection preset: "all", "recommended" (default), or "security-only"
preset = "recommended"

# Enable/disable specific rules
# Values: "error", "warn", "info", "off"
[lint.rules]
"security/tx-origin" = "error"
"gas/cache-array-length" = "off"
"naming/private-vars-underscore" = "warn"

# Rule-specific configuration
[lint.settings]
"best-practices/code-complexity".threshold = 10
"best-practices/function-max-lines".max_lines = 60
"best-practices/max-states-count".max_count = 20
"security/compiler-version".allowed = [">=0.8.19", "<0.9.0"]
"naming/foundry-test-functions".pattern = "test(Fork)?(Fuzz)?(Fail)?_"
"style/max-line-length".limit = 120

[format]
line_length = 120
tab_width = 4
use_tabs = false
single_quote = false
bracket_spacing = false
number_underscore = "preserve"
uint_type = "uint256"
sort_imports = false
multiline_func_header = "attributes_first"

[global]
# Solidity version (auto-detected from pragma if omitted)
solidity_version = "0.8.24"
# File patterns to include
include = ["src/**/*.sol", "test/**/*.sol", "script/**/*.sol"]
# File patterns to exclude
exclude = ["lib/**", "node_modules/**", "out/**"]
# Respect .gitignore
respect_gitignore = true
# Number of threads (0 = auto)
threads = 0
# Cache directory
cache_dir = ".solgrid_cache"
```

### 8.2 Config Resolution Order

Configuration is resolved in this priority (highest first):

1. CLI flags (`--rule`, `--fix`, etc.)
2. Inline comments (`// solgrid-disable-next-line`)
3. `solgrid.toml` in the closest parent directory
4. `solgrid.toml` in the project root
5. `~/.config/solgrid/solgrid.toml` (global user config)
6. Built-in defaults

### 8.3 Foundry.toml Compatibility

If no `solgrid.toml` is found, solgrid reads formatting options from `foundry.toml` under `[fmt]`. This provides zero-config adoption for Foundry projects.

### 8.4 Solhint Migration

```bash
solgrid migrate --from solhint
```

Reads `.solhint.json` (or `.solhintrc`), maps rule names to solgrid equivalents, and writes a `solgrid.toml`.

---

## 9. CLI Interface

```
solgrid — The Solidity linter and formatter

USAGE:
    solgrid <COMMAND> [OPTIONS] [FILES...]

COMMANDS:
    check       Lint files and report diagnostics (default)
    fix         Lint files and apply safe auto-fixes
    fmt         Format files
    server      Start the LSP server (used by editor extensions)
    migrate     Convert config from solhint or forge fmt
    explain     Show detailed documentation for a rule
    list-rules  List all available rules with status

OPTIONS:
    --config <PATH>         Path to solgrid.toml
    --rule <RULE=LEVEL>     Override a rule's severity
    --fix                   Apply safe auto-fixes
    --unsafe-fixes          Also apply suggestion-level fixes (requires --fix)
    --format                Also format files (combines check + fmt)
    --diff                  Show diff instead of writing files
    --stdin                 Read from stdin, write to stdout
    --output-format <FMT>   Output format: text (default), json, github, sarif
    -j, --threads <N>       Number of parallel threads (default: auto)
    --no-cache              Disable incremental cache
    --verbose               Show which rules matched
    --quiet                 Only show errors
    --version               Print version
    --help                  Print help

EXAMPLES:
    solgrid check src/                      # Lint all .sol files in src/
    solgrid fix src/ --unsafe-fixes         # Apply safe + suggestion fixes
    solgrid fmt src/ --diff                 # Preview formatting changes
    solgrid check --output-format sarif     # SARIF output for CI
    solgrid explain security/reentrancy     # Show rule docs
```

### 9.1 Output Formats

| Format | Use Case |
|---|---|
| `text` | Human-readable terminal output with colors and context |
| `json` | Machine-readable, one JSON object per diagnostic |
| `github` | GitHub Actions annotation format (`::error file=...`) |
| `sarif` | OASIS SARIF 2.1 for CI integrations (CodeQL, etc.) |

### 9.2 Exit Codes

| Code | Meaning |
|---|---|
| `0` | No errors or warnings |
| `1` | Diagnostics were reported |
| `2` | CLI usage error or invalid config |
| `3` | Internal error (parser crash, bug) |

---

## 10. Performance Goals

| Metric | Target | Rationale |
|---|---|---|
| **Cold lint** (full project, no cache) | < 500ms for 500 files | Comparable to Ruff; Solidity ASTs are smaller but rules are heavier |
| **Warm lint** (incremental, cached) | < 50ms for changed files | Only re-lint files whose content hash changed |
| **Format** (full project) | < 300ms for 500 files | Match or beat forge fmt |
| **LSP diagnostics** (single file) | < 20ms | Must feel instant in the editor |
| **Memory** | < 200MB for 1000-file project | Arena allocation keeps AST memory bounded |
| **Binary size** | < 30MB (stripped) | Single static binary, no runtime dependencies |
| **Startup time** | < 10ms | No JIT, no VM, no plugin loading |

### Performance Strategies

- **Parallelism:** Files are linted in parallel using `rayon`. The early (syntactic) pass is embarrassingly parallel. The late (semantic) pass uses a shared read-only symbol table built in a prior phase.
- **Arena allocation:** Solar's `bumpalo`-based AST means zero `malloc`/`free` overhead during AST construction. The entire AST for a file is freed in one operation.
- **Incremental caching:** Content-hash-based cache. If a file's hash matches the cache entry and the config hasn't changed, skip linting. Cache is invalidated on config change or solgrid version upgrade.
- **Zero-copy parsing:** Source text is memory-mapped; spans reference the original buffer rather than copying strings.
- **Rule short-circuiting:** Rules declare which AST node types they visit. The traversal engine skips nodes that no active rule cares about.

---

## 11. Project Roadmap

### Phase 1: Foundation

- Workspace setup with all crates
- Solar parser integration (`solgrid_parser`)
- Rule engine with `declare_solgrid_rule!` macro
- First 30 rules: all security rules, core naming rules, core best practices
- Three-tier fix system
- CLI (`solgrid check`, `solgrid fix`)
- `solgrid.toml` config parsing
- Snapshot test infrastructure
- Text and JSON output formats

### Phase 2: Formatter + Full Rules

- Built-in formatter (`solgrid fmt`)
- Remaining 60+ lint rules (gas, style, docs)
- Inline suppression system
- Incremental caching
- GitHub and SARIF output formats
- `solgrid migrate --from solhint`
- Foundry.toml compatibility
- CI integration documentation

### Phase 3: Editor Integration

- LSP server (`solgrid_server`)
- VSCode extension (TypeScript client)
- Fix-on-save, format-on-save
- Code action presentation (tiered fixes)
- Hover docs for rules
- Extension published to VS Marketplace

### Phase 4: Prettier Plugin + Polish

- NAPI-RS bindings (`solgrid_napi`)
- Prettier plugin (`prettier-plugin-solgrid`)
- Prettier output compatibility test suite
- WASM build (`solgrid_wasm`) for web playground
- Performance optimization pass
- v1.0 release

---

## Appendix A: Dependency Graph

```
solgrid (binary)
  ├── solgrid_config
  ├── solgrid_linter
  ├── solgrid_formatter
  └── solgrid_diagnostics

solgrid_linter
  ├── solgrid_parser
  ├── solgrid_ast
  ├── solgrid_diagnostics
  └── solgrid_config

solgrid_formatter
  ├── solgrid_parser
  ├── solgrid_ast
  └── solgrid_config

solgrid_server
  ├── solgrid_linter
  ├── solgrid_formatter
  ├── solgrid_config
  └── solgrid_diagnostics

solgrid_napi
  ├── solgrid_formatter
  └── solgrid_config

solgrid_parser
  ├── solar-parse (external)
  └── solar-ast (external)

solgrid_ast
  ├── solar-ast (external)
  └── solgrid_parser
```

## Appendix B: External Dependencies

| Crate | Purpose | Version Policy |
|---|---|---|
| `solar-parse` / `solar-ast` | Solidity parser and AST | Pin to minor; track upstream closely |
| `rayon` | Data parallelism | Stable; latest |
| `toml` | Config file parsing | Stable; latest |
| `serde` | Serialization | Stable; latest |
| `clap` | CLI argument parsing | v4+ |
| `tower-lsp` | LSP server framework | Latest |
| `napi` / `napi-derive` | NAPI-RS Node.js bindings | Latest |
| `bumpalo` | Arena allocator (via Solar) | Matches Solar's version |
| `miette` or `ariadne` | Diagnostic rendering | Evaluate both; choose one |
| `ignore` | Gitignore-aware file walking | Stable; latest |
| `insta` | Snapshot testing | Dev dependency; latest |
