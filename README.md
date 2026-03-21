# solgrid

A Rust-native Solidity linter and formatter.

90+ lint rules, built-in formatter, auto-fix, incremental caching, LSP server, VSCode extension, and Prettier plugin. Outputs text, JSON, GitHub Actions annotations, or SARIF.

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

The default `recommended` preset enables `security/*`, `best-practices/*`, and `naming/*`. The `docs/*`, `gas/*`, and `style/*` categories are opt-in unless you choose `preset = "all"` or enable individual rules explicitly.

## Documentation

| Resource | Description |
|----------|-------------|
| [Configuration Guide](docs/configuration.md) | Full `solgrid.toml` reference, presets, resolution order |
| [Rules Reference](docs/rules.md) | Current rule inventory with default severity and fix availability |
| [Editor Integration](docs/editor-integration.md) | VSCode, Cursor, and LSP setup for other editors |
| [Prettier Plugin](docs/prettier-plugin.md) | Using solgrid as a Prettier plugin |
| [Output Formats](docs/output-formats.md) | Text, JSON, GitHub Actions, and SARIF output |
| [WASM Bindings](docs/wasm.md) | Browser and web playground API |
| [Architecture](ARCHITECTURE.md) | Full technical design document |
| [Contributing](CONTRIBUTING.md) | Development setup, testing, and release process |
| [Changelog](CHANGELOG.md) | Release notes |

## License

MIT
