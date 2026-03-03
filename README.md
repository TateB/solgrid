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

[global]
exclude = ["lib/**", "node_modules/**"]
```

See the [Configuration Guide](docs/configuration.md) for the full reference including rule-specific settings, presets, and config resolution order.

## Documentation

| Resource | Description |
|----------|-------------|
| [Configuration Guide](docs/configuration.md) | Full `solgrid.toml` reference, presets, resolution order |
| [Editor Integration](docs/editor-integration.md) | VSCode, Cursor, and LSP setup for other editors |
| [Prettier Plugin](docs/prettier-plugin.md) | Using solgrid as a Prettier plugin |
| [Output Formats](docs/output-formats.md) | Text, JSON, GitHub Actions, and SARIF output |
| [WASM Bindings](docs/wasm.md) | Browser and web playground API |
| [Architecture](ARCHITECTURE.md) | Full technical design document |
| [Contributing](CONTRIBUTING.md) | Development setup, testing, and release process |
| [Changelog](CHANGELOG.md) | Release notes |

## License

MIT OR Apache-2.0
