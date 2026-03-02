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

- Multi-crate Rust workspace structure (9 crates)
- Rule engine design with two-pass analysis (syntactic + semantic)
- Complete rule set with 90 rules across 6 categories
- Formatter with chunk-based intermediate representation
- LSP server and VSCode extension design
- Configuration system (`solgrid.toml`)
- CLI interface and output formats
- Performance goals and strategies
- Phased development roadmap

## Status

**In development** — 90/90 lint rules implemented across 6 categories (security, best practices, naming, gas optimization, style, documentation). Full chunk-based formatter with comment preservation and idempotency verification. Incremental caching, GitHub Actions/SARIF output, foundry.toml fallback, and solhint migration support. 258 tests passing. See [TODO.md](./TODO.md) for detailed progress.
