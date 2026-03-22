# Editor Integration

## VSCode

The `editors/vscode/` directory contains a VSCode extension that provides:

- Real-time linting as you type
- Quick-fix code actions grouped by safety tier (safe, suggestion, dangerous)
- Document and range formatting
- Fix-on-save and format-on-save
- Rule documentation on hover
- Suppression comment completion (`// solgrid-disable-next-line ...`)

### Extension Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `solgrid.enable` | `true` | Enable solgrid |
| `solgrid.path` | `null` | Path to solgrid binary (auto-detected from PATH) |
| `solgrid.fixOnSave` | `true` | Auto-fix safe issues on save |
| `solgrid.fixOnSave.unsafeFixes` | `false` | Also apply suggestion-level fixes |
| `solgrid.formatOnSave` | `true` | Format on save |
| `solgrid.configPath` | `null` | Optional path to a specific `solgrid.toml`; otherwise the server auto-discovers the nearest config per document |

## Cursor

Cursor uses the same Extension Host and LSP protocol as VSCode. The solgrid extension works in Cursor without modification. The LSP integration tests verify protocol-level behavior that applies to both editors.

## Other Editors

Any editor with LSP support can use solgrid as a language server:

```bash
solgrid server
```

The server communicates via stdio and supports the standard LSP protocol.

### LSP Capabilities

| Feature | Description |
|---------|-------------|
| `textDocument/publishDiagnostics` | Real-time lint diagnostics as the user types (debounced) |
| `textDocument/codeAction` | Quick-fixes for all fixable rules, organized by safety tier |
| `textDocument/formatting` | Full-document formatting |
| `textDocument/rangeFormatting` | Format selection |
| `textDocument/onSave` | Auto-fix safe fixes + format on save (configurable) |
| `textDocument/hover` | Rule documentation on hover over a diagnostic |
| `textDocument/completion` | Inline suppression comment completion (`// solgrid-disable...`) |
| `initialize` / `workspace/didChangeConfiguration` | Read fix-on-save, format-on-save, and optional `configPath` settings from the client |
