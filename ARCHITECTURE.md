# solgrid Architecture

> A blazing-fast, Rust-native linter and formatter for Solidity smart contracts.

---

## Table of Contents

1. [Project Vision](#1-project-vision)
2. [Architecture Overview](#2-architecture-overview)
3. [Linter Core](#3-linter-core)
4. [Rule Inventory](#4-rule-inventory)
5. [Formatter](#5-formatter)
6. [Prettier Plugin](#6-prettier-plugin)
7. [VSCode Extension](#7-vscode-extension)
8. [Configuration](#8-configuration)
9. [CLI Interface](#9-cli-interface)
10. [Performance Goals](#10-performance-goals)
11. [Project Roadmap](#11-project-roadmap)

---

## 1. Project Vision

**solgrid** is a unified Solidity linter, formatter, and language server written in Rust.

### Why solgrid?

- **Speed:** Rust + Solar parser keeps linting and formatting fast enough for editor and CI workflows.
- **Coverage:** 89 active rules, a built-in formatter, and three-tier auto-fix cover the common Solidity quality checks in one tool.
- **Consistency:** One binary and one config file drive CLI, formatter, cache, and editor behavior.
- **Editor integration:** A native LSP server provides diagnostics, code actions, formatting, and save-time actions.
- **Extensibility:** The rule engine is modular and designed for future native extension points.

### Design Principles

1. **Correctness first.** Every rule must have a clear specification, comprehensive test suite, and zero false positives on well-formed Solidity.
2. **Speed is a feature.** Linting an entire Foundry project should complete in under one second. The tool should never be the bottleneck in a developer's workflow.
3. **Batteries included.** Ship with every rule a Solidity developer needs. No plugin installation for standard use cases.
4. **Consistent by default.** Formatting is deterministic and configuration is shared across CLI, formatter, and editor workflows.
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

The workspace is split into focused crates with a single responsibility and a well-defined public API.

```
crates/
  solgrid/              # Binary crate — CLI entry point
  solgrid_ast/          # AST utilities, semantic helpers, symbol table
  solgrid_config/       # Config file parsing (solgrid.toml)
  solgrid_diagnostics/  # Diagnostic types, severity, reporting
  solgrid_formatter/    # Built-in Solidity formatter
  solgrid_linter/       # Rule engine, rule registry, violation types
  solgrid_napi/         # NAPI-RS bindings for Node.js (prettier plugin)
  solgrid_parser/       # Thin wrapper around Solar parser
  solgrid_server/       # LSP server implementation
  solgrid_wasm/         # WASM build target (playground, browser use)
```

### 3.2 Parser: Solar

solgrid uses [**Solar**](https://github.com/paradigmxyz/solar) (`paradigmxyz/solar`) as its parser. Solar is:

- Written in Rust, derived from rustc's parser architecture
- 41x faster than solc at parsing
- Produces a typed AST with `Visit` and `VisitMut` traits for traversal
- Uses arena allocation (`bumpalo`) for zero-copy, cache-friendly AST nodes
- Actively maintained by Paradigm (Foundry ecosystem)

The `solgrid_parser` crate exposes a small callback-based API around Solar:

```rust
pub fn with_parsed_ast<T, F>(source: &str, filename: &str, callback: F) -> Result<T, ParseError>
where
    T: Send,
    F: FnOnce(&solar_ast::SourceUnit<'_>) -> T + Send;

pub fn with_parsed_ast_sequential<T, F>(
    source: &str,
    filename: &str,
    callback: F,
) -> Result<T, ParseError>
where
    F: FnOnce(&solar_ast::SourceUnit<'_>) -> T;

pub fn check_syntax(source: &str, filename: &str) -> Result<(), ParseError>;
```

The wrapper exists to insulate the rest of the codebase from Solar API changes and provide a unified error type.

### 3.3 Rule Engine

Rules are the heart of solgrid. The current engine owns a registry of built-in rules, creates a `LintContext` per file, filters the registry by the active config, runs each enabled rule, applies suppression comments, then applies configured severity overrides before sorting diagnostics.

**Rule trait:**

```rust
pub trait Rule: Send + Sync {
    /// Rule metadata: name, category, default severity, docs.
    fn meta(&self) -> &RuleMeta;

    /// Run the rule against a source file.
    /// Produces zero or more diagnostics, each optionally carrying a Fix.
    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic>;
}
```

**Lint context:**

The `LintContext` provides rules with everything they need without requiring them to manage their own AST traversal:

```rust
pub struct LintContext<'a> {
    /// Source text being linted.
    pub source: &'a str,
    /// The file path.
    pub path: &'a Path,
    /// Active configuration for this file.
    pub config: &'a Config,
}
```

Rules that need AST access parse the source inside their `check()` implementation, typically via `with_parsed_ast_sequential()`. The engine itself does not currently maintain a shared semantic model or a global multi-pass analysis pipeline.

### 3.4 Three-Tier Auto-Fix System

Every fixable rule categorizes its fix into one of three safety tiers:

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

---

## 4. Rule Inventory

solgrid currently exposes 89 active rules across six categories. The canonical inventory lives in [docs/rules.md](docs/rules.md), which is a better fit than this architecture document for the full catalog.

### Rule Counts Summary

| Category | Count | Fixable |
|---|---|---|
| Security | 19 | 2 |
| Best Practices | 21 | 5 |
| Naming | 16 | 0 |
| Gas Optimization | 15 | 5 |
| Style | 10 | 8 |
| Documentation | 8 | 1 |
| **Total** | **89** | **21** |

Notes:

- `best-practices/use-natspec` is a deprecated config alias for `docs/natspec-function`, not an active registered rule.
- Default preset behavior is documented in [docs/configuration.md](docs/configuration.md).

---

## 5. Formatter

solgrid includes a built-in formatter (`solgrid fmt`) as a first-class component.

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

The intermediate representation is chunk-based rather than direct string concatenation. Formatting builds an IR in `crates/solgrid_formatter/src/ir.rs` and prints it with line-fitting in `crates/solgrid_formatter/src/printer.rs`.

### 5.2 Formatting Options

These options live in `[format]` in `solgrid.toml`:

| Option | Type | Default |
|---|---|---|
| `line_length` | integer | 120 |
| `tab_width` | integer | 4 |
| `use_tabs` | bool | false |
| `single_quote` | bool | false |
| `bracket_spacing` | bool | false |
| `number_underscore` | `"thousands"` / `"remove"` / `"preserve"` | `"preserve"` |
| `uint_type` | `"uint256"` / `"uint"` / `"preserve"` | `"uint256"` |
| `override_spacing` | bool | true |
| `wrap_comments` | bool | false |
| `sort_imports` | bool | false |
| `multiline_func_header` | `"attributes_first"` / `"params_first"` / `"all"` | `"attributes_first"` |
| `contract_body_spacing` | `"preserve"` / `"single"` / `"compact"` | `"preserve"` |
| `inheritance_brace_new_line` | bool | true |

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

- **Parser:** Calls into `solgrid_napi` to validate Solidity syntax and returns a lightweight wrapper containing the source text.
- **Printer:** Calls `solgrid_napi.format()` with Prettier's resolved options mapped to solgrid's native formatting options.

### 6.2 Option Mapping

| Prettier Option | solgrid Option |
|---|---|
| `printWidth` | `line_length` |
| `tabWidth` | `tab_width` |
| `useTabs` | `use_tabs` |
| `singleQuote` | `single_quote` |
| `bracketSpacing` | `bracket_spacing` |

### 6.3 Formatter Validation

The formatter is validated with snapshot, idempotency, and conformance tests over a Solidity corpus so formatting behavior stays deterministic and stable.

---

## 7. VSCode Extension

### 7.1 Architecture

The VSCode extension (`solgrid-vscode`) is a thin TypeScript client that speaks LSP to the Rust server:

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
| `initialize` / `workspace/didChangeConfiguration` | Read fix-on-save, format-on-save, and optional `configPath` settings from the client |

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
- Rule ID in the diagnostic payload (e.g., `security/tx-origin`)
- Source marked as `solgrid`
- Quick-fix code actions grouped by safety tier
- Hover documentation for rules and symbols

### 7.5 Extension Technology

- **Client:** TypeScript, using `vscode-languageclient`
- **Bundler:** esbuild
- **Distribution:** VS Marketplace + Open VSX Registry

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

Configuration is resolved per file:

1. `--config <path>` when provided
2. The nearest `solgrid.toml` discovered by walking upward from the file or traversal root
3. Built-in defaults

### 8.3 Foundry.toml Compatibility

If no `solgrid.toml` is found, solgrid reads formatting options from `foundry.toml` under `[fmt]`. This provides zero-config adoption for Foundry projects.

### 8.4 Config Migration

```bash
solgrid migrate --from solhint
```

Reads `.solhint.json` or `.solhintrc`, maps supported rules to solgrid equivalents, and writes a `solgrid.toml`.

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
    migrate     Migrate configuration from another tool
    explain     Show detailed documentation for a rule
    list-rules  List all available rules with status

OPTIONS:
    --config <PATH>         Path to solgrid.toml
    --fix                   Apply safe auto-fixes
    --unsafe-fixes          Also apply suggestion-level fixes (requires --fix)
    --diff                  Show diff instead of writing files
    --stdin                 Read from stdin, write to stdout
    --output-format <FMT>   Output format: text (default), json, github, sarif
    --no-cache              Disable incremental cache
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
| **Format** (full project) | < 300ms for 500 files | Fast enough for whole-project formatting in CI and editor workflows |
| **LSP diagnostics** (single file) | < 20ms | Must feel instant in the editor |
| **Memory** | < 200MB for 1000-file project | Arena allocation keeps AST memory bounded |
| **Binary size** | < 30MB (stripped) | Single static binary, no runtime dependencies |
| **Startup time** | < 10ms | No JIT, no VM, no plugin loading |

### Performance Strategies

- **Parallelism:** CLI file processing uses `rayon`, and `check` / `fmt` can honor configured thread counts.
- **Arena allocation:** Solar's `bumpalo`-based AST means zero `malloc`/`free` overhead during AST construction. The entire AST for a file is freed in one operation.
- **Incremental caching:** Content-hash-based cache. If a file's hash matches the cache entry and the config hasn't changed, skip linting. Cache is invalidated on config change or solgrid version upgrade.
- **Per-file config caching:** Nearest-config resolution is cached by directory during CLI runs so repeated lookups do not re-read disk.
- **Chunk-based formatting:** The formatter builds an intermediate representation and performs deterministic line fitting rather than ad hoc string rewriting.

---

## 11. Project Roadmap

### Phase 1: Foundation

- Workspace setup with all crates
- Solar parser integration (`solgrid_parser`)
- Rule engine and built-in rule registry
- First 30 rules: all security rules, core naming rules, core best practices
- Three-tier fix system
- CLI (`solgrid check`, `solgrid fix`)
- `solgrid.toml` config parsing
- Snapshot test infrastructure
- Text and JSON output formats

### Phase 2: Formatter + Full Rules

- Built-in formatter (`solgrid fmt`)
- Full rule catalog across gas, style, and docs
- Inline suppression system
- Incremental caching
- GitHub and SARIF output formats
- Config migration command
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
- Formatter conformance and regression test suite
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
  ├── solgrid_diagnostics
  └── solgrid_server

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
| `tower-lsp-server` | LSP server framework | Latest |
| `napi` / `napi-derive` | NAPI-RS Node.js bindings | Latest |
| `bumpalo` | Arena allocator (via Solar) | Matches Solar's version |
| `ariadne` | Diagnostic rendering | Stable; pinned in workspace |
| `ignore` | Gitignore-aware file walking | Stable; latest |
| `insta` | Snapshot testing | Dev dependency; latest |
