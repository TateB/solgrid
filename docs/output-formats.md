# Output Formats

solgrid supports multiple output formats for different use cases.

## Formats

| Format | Flag | Description |
|--------|------|-------------|
| Text | `--output-format text` | Colored terminal output (default) |
| JSON | `--output-format json` | Machine-readable JSON |
| GitHub | `--output-format github` | GitHub Actions `::error`/`::warning` annotations |
| SARIF | `--output-format sarif` | OASIS SARIF 2.1.0 for CodeQL and security tools |

## Graph Exports

The `solgrid graph` subcommand uses its own export flag instead of `--output-format`.

| Format | Flag | Description |
|--------|------|-------------|
| JSON | `--format json` | Serialized shared `GraphDocument` payload for tools and scripts |
| Mermaid | `--format mermaid` | Mermaid flowchart text suitable for Markdown previews or docs |
| DOT | `--format dot` | Graphviz DOT output for downstream renderers and CLI pipelines |

## Usage Examples

```bash
# Default colored terminal output
solgrid check

# Machine-readable JSON
solgrid check --output-format json

# GitHub Actions annotations
solgrid check --output-format github

# SARIF for CodeQL and security tools
solgrid check --output-format sarif

# Export an imports graph as JSON
solgrid graph --kind imports --format json src/Vault.sol

# Export a control-flow graph as Mermaid
solgrid graph --kind control-flow --symbol Vault.run --format mermaid src/Vault.sol

# Export an inheritance graph as Graphviz DOT
solgrid graph --kind inheritance --symbol Vault --format dot src/Vault.sol
```

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No errors or warnings |
| `1` | Diagnostics were reported |
| `2` | CLI usage error or invalid config |
| `3` | Internal error (parser crash, bug) |
