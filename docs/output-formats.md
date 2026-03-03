# Output Formats

solgrid supports multiple output formats for different use cases.

## Formats

| Format | Flag | Description |
|--------|------|-------------|
| Text | `--output-format text` | Colored terminal output (default) |
| JSON | `--output-format json` | Machine-readable JSON |
| GitHub | `--output-format github` | GitHub Actions `::error`/`::warning` annotations |
| SARIF | `--output-format sarif` | OASIS SARIF 2.1.0 for CodeQL and security tools |

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
```

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No errors or warnings |
| `1` | Diagnostics were reported |
| `2` | CLI usage error or invalid config |
| `3` | Internal error (parser crash, bug) |
