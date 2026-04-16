# solgrid

A Rust-native Solidity linter and formatter.

90+ lint rules, built-in formatter, auto-fix, incremental caching, LSP server, VSCode extension, and Prettier plugin. Outputs text, JSON, GitHub Actions annotations, or SARIF.

The editor stack now also includes cross-file navigation, call hierarchy, a security overview, import, inheritance, linearized inheritance, and control-flow graph previews with inherited modifier expansion, Yul function/call expansion, terminal assembly semantics, and semantic node/edge rendering, full, delta, and visible-range semantic tokens for Solidity declarations and high-signal references, parameter-name, selector/interface-ID, inheritance-origin, inherited-member, contract-lineage, and detector-aware declaration inlay hints, LCOV/Cobertura-backed coverage decorations, summary views, and provider-aware Foundry, Hardhat, and custom run commands, stronger same-file, inherited-helper, and contract-typed helper-wrapper interprocedural security detectors, including uniquely resolved member and indexed helper bases, overloaded helper-returning call expressions when their return targets collapse to the same helper contract, and imported wrapper overloads and wrapper chains when semantic filtering leaves one propagated sink result, and conservative rename support for same-file plus safe cross-file symbol graphs on top of the Rust-native language server.

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

# Export a control-flow graph as Mermaid
solgrid graph --kind control-flow --symbol Vault.run --format mermaid src/Vault.sol

# Export an inheritance graph as Graphviz DOT
solgrid graph --kind inheritance --symbol Vault --format dot src/Vault.sol
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
| [IDE and Security Roadmap](docs/editor-security-roadmap.md) | Detailed plan for compiler diagnostics, detectors, graphs, and richer editor UX |
| [Semantic Detectors](docs/semantic-detectors.md) | Native semantic security findings, compiler-style semantic diagnostics, and current intentional limits |
| [Prettier Plugin](docs/prettier-plugin.md) | Using solgrid as a Prettier plugin |
| [Output Formats](docs/output-formats.md) | Text, JSON, GitHub Actions, and SARIF output |
| [WASM Bindings](docs/wasm.md) | Browser and web playground API |
| [Architecture](ARCHITECTURE.md) | Full technical design document |
| [Contributing](CONTRIBUTING.md) | Development setup, testing, and release process |
| [Changelog](CHANGELOG.md) | Release notes |

## License

MIT
