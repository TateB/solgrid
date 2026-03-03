# solgrid

A blazing-fast, Rust-native Solidity linter and formatter. One tool to replace solhint, forge fmt, and prettier-plugin-solidity.

## Features

- **90+ lint rules** across security, best practices, naming, gas optimization, style, and documentation
- **Built-in formatter** with Wadler-Lindig line-fitting algorithm, comment preservation, and formatter directives
- **Three-tier auto-fix** — safe, suggestion, and dangerous fixes with full developer control
- **Incremental caching** — content-hash-based file cache for near-instant re-runs
- **Multiple output formats** — text (colored), JSON, GitHub Actions annotations, SARIF 2.1.0
- **Foundry.toml fallback** — reads `[fmt]` section when no `solgrid.toml` is found
- **Migration support** — `solgrid migrate --from solhint` converts `.solhint.json` to `solgrid.toml`
- **Stdin/stdout support** — pipe Solidity through solgrid for editor integrations
- **LSP server** — real-time linting, code actions, formatting, hover docs, and suppression completions
- **VSCode extension** — first-class editor integration with fix-on-save and format-on-save
- **Prettier plugin** — drop-in integration for teams using Prettier (`prettier-plugin-solgrid`)
- **Sub-second performance** on entire projects, powered by the Solar parser

## Quick Start

```bash
# Lint current directory
solgrid check

# Lint with auto-fix
solgrid fix

# Format Solidity files
solgrid fmt

# Format check (dry run)
solgrid fmt --diff

# Lint from stdin
echo 'pragma solidity ^0.8.0; contract T {}' | solgrid check --stdin

# Format from stdin
echo 'pragma solidity ^0.8.0; contract T {}' | solgrid fmt --stdin

# GitHub Actions output
solgrid check --output-format github

# SARIF output (for CodeQL, etc.)
solgrid check --output-format sarif

# Migrate from solhint
solgrid migrate --from solhint

# Start the LSP server (for editor integration)
solgrid server
```

## Configuration

solgrid uses `solgrid.toml` for configuration. If no `solgrid.toml` is found, it falls back to the `[fmt]` section of `foundry.toml`.

```toml
[lint]
preset = "recommended"

[lint.rules]
"security/tx-origin" = "error"
"gas/custom-errors" = "warn"
"naming/const-name-snakecase" = "off"

[format]
line_length = 120
tab_width = 4
use_tabs = false
single_quote = false
bracket_spacing = false
number_underscore = "preserve"
uint_type = "long"
sort_imports = false

[global]
exclude = ["lib/**", "node_modules/**"]
```

## Output Formats

| Format | Flag | Description |
|--------|------|-------------|
| Text | `--output-format text` | Colored terminal output (default) |
| JSON | `--output-format json` | Machine-readable JSON |
| GitHub | `--output-format github` | GitHub Actions `::error`/`::warning` annotations |
| SARIF | `--output-format sarif` | OASIS SARIF 2.1.0 for CodeQL and security tools |

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full design document covering:

- Multi-crate Rust workspace structure (11 crates)
- Rule engine design with two-pass analysis (syntactic + semantic)
- Complete rule set with 90 rules across 6 categories
- Formatter with chunk-based intermediate representation
- LSP server and VSCode extension design
- Configuration system (`solgrid.toml`)
- CLI interface and output formats
- Performance goals and strategies
- Phased development roadmap

## Status

**In development** — 90/90 lint rules implemented across 6 categories (security, best practices, naming, gas optimization, style, documentation). Full chunk-based formatter with comment preservation and idempotency verification. Incremental caching, GitHub Actions/SARIF output, foundry.toml fallback, and solhint migration support. LSP server with real-time linting, code actions, formatting, hover docs, and suppression completions. VSCode extension with fix-on-save and format-on-save. Prettier plugin with NAPI-RS bindings. Release workflow with platform-specific binaries and VSIX packages. 309+ tests passing. See [TODO.md](./TODO.md) for detailed progress.

## Editor Integration

### VSCode

The `editors/vscode/` directory contains a VSCode extension that provides:

- Real-time linting as you type
- Quick-fix code actions grouped by safety tier (safe, suggestion, dangerous)
- Document and range formatting
- Fix-on-save and format-on-save
- Rule documentation on hover
- Suppression comment completion (`// solgrid-disable-next-line ...`)

**Extension settings:**

| Setting | Default | Description |
|---------|---------|-------------|
| `solgrid.enable` | `true` | Enable solgrid |
| `solgrid.path` | `null` | Path to solgrid binary (auto-detected from PATH) |
| `solgrid.fixOnSave` | `true` | Auto-fix safe issues on save |
| `solgrid.fixOnSave.unsafeFixes` | `false` | Also apply suggestion-level fixes |
| `solgrid.formatOnSave` | `true` | Format on save |
| `solgrid.configPath` | `null` | Path to solgrid.toml (auto-discovered) |

### Cursor

Cursor uses the same Extension Host and LSP protocol as VSCode. The solgrid extension works in Cursor without modification. The LSP integration tests verify protocol-level behavior that applies to both editors.

### Other Editors

Any editor with LSP support can use solgrid as a language server:

```bash
solgrid server
```

The server communicates via stdio and supports the standard LSP protocol.

## Prettier Plugin

The `prettier-plugin-solgrid` package lets teams already using Prettier adopt solgrid's formatter without changing their workflow. The plugin delegates all formatting to solgrid's Rust formatter via NAPI-RS bindings.

```bash
# Install
npm install --save-dev prettier prettier-plugin-solgrid

# Format with Prettier
npx prettier --write "**/*.sol"
```

Standard Prettier options (`printWidth`, `tabWidth`, `useTabs`, `singleQuote`, `bracketSpacing`) are automatically mapped to solgrid equivalents. Additional solgrid-specific options are available:

| Option | Default | Description |
|--------|---------|-------------|
| `solidityNumberUnderscore` | `"preserve"` | Number literal underscores: `"preserve"`, `"thousands"`, `"remove"` |
| `solidityUintType` | `"long"` | Uint representation: `"long"` (uint256), `"short"` (uint), `"preserve"` |
| `soliditySortImports` | `false` | Sort import statements alphabetically |
| `solidityMultilineFuncHeader` | `"attributes_first"` | Multiline function header style |
| `solidityOverrideSpacing` | `true` | Space in override specifiers |
| `solidityWrapComments` | `false` | Wrap comments to fit within printWidth |
| `solidityContractNewLines` | `false` | Newlines at start/end of contract body |

## Development & Testing

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

# Run linter benchmarks only
cargo bench -p solgrid_linter
```

### CI

The GitHub Actions workflow runs the full test suite: Rust checks (check, test, fmt, clippy), VSCode extension unit tests, LSP integration tests, VSCode e2e tests, and Prettier plugin tests.
