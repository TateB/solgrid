# Editor Integration

## VSCode

The `editors/vscode/` directory contains a VSCode extension that provides:

- Real-time linting as you type
- Quick-fix code actions grouped by safety tier (safe, suggestion, dangerous)
- Document and range formatting
- Fix-on-save and format-on-save
- Rule documentation on hover
- Cross-file references, document symbols, workspace symbols, and import-path document links
- Call hierarchy for resolvable function and modifier declarations/call sites
- Reference-count and graph-entry code lenses
- Import, inheritance, linearized inheritance, and control-flow graph previews rendered as Markdown/Mermaid
- Parameter-name inlay hints for positional call arguments plus selector/interface-ID, inheritance-origin, inherited-member, contract-lineage, and detector-aware declaration hints
- A security overview tree with grouping, rerun, suppression, fix, and ignore-baseline flows
- LCOV/Cobertura coverage ingestion with a coverage summary tree, uncovered/partial line decorations, and provider-aware Foundry, Hardhat, and custom coverage run commands
- Conservative rename support for same-file and mechanically provable cross-file symbol graphs, including safe aliased and namespace-import rewrites
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
| `solgrid.coverage.enable` | `true` | Enable LCOV/Cobertura coverage discovery, summary views, and editor decorations |
| `solgrid.coverage.artifacts` | `["**/lcov.info", "**/*.lcov", "**/cobertura*.xml", "**/coverage.xml"]` | Glob patterns used to discover coverage artifacts in the workspace |
| `solgrid.coverage.autoRefreshAfterRun` | `true` | Refresh the coverage view after a coverage command completes successfully |
| `solgrid.coverage.customCommand` | `[]` | Custom coverage command as an argv-style string array, for example `["pnpm", "run", "coverage"]` |

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
| `textDocument/references` | Same-file lexical references plus cross-file references for imported top-level symbols |
| `textDocument/documentSymbol` / `workspace/symbol` | Hierarchical outline symbols and workspace symbol search |
| `textDocument/documentLink` | Clickable import-path links to resolved Solidity files |
| `textDocument/codeLens` | Reference counts and graph-entry code lenses |
| `textDocument/prepareCallHierarchy` / `callHierarchy/incomingCalls` / `callHierarchy/outgoingCalls` | Conservative call hierarchy for uniquely resolvable function and modifier declarations/call sites |
| `textDocument/prepareRename` / `textDocument/rename` | Conservative rename support for same-file and mechanically provable cross-file symbol graphs |
| `textDocument/formatting` | Full-document formatting |
| `textDocument/rangeFormatting` | Format selection |
| `textDocument/semanticTokens/full` / `textDocument/semanticTokens/full/delta` / `textDocument/semanticTokens/range` | Semantic tokens for Solidity declarations and high-signal references, including imports, contracts, functions, modifiers, events, variables, and parameters |
| `textDocument/inlayHint` | Parameter-name hints for positional call arguments plus ABI selector/interface-ID, inheritance-origin, inherited-member, contract-lineage, and detector-aware declaration hints |
| `textDocument/onSave` | Auto-fix safe fixes + format on save (configurable) |
| `textDocument/hover` | Rule documentation on hover over a diagnostic |
| `textDocument/completion` | Inline suppression comment completion (`// solgrid-disable...`) |
| `workspace/executeCommand` | Security reruns plus graph document generation for imports, inheritance, linearized inheritance, and control flow |
| `initialize` / `workspace/didChangeConfiguration` | Read fix-on-save, format-on-save, and optional `configPath` settings from the client |

### Graph Previews and Hints

The VS Code extension exposes a command-palette entry for the current file's imports graph and graph-entry code lenses for imports, inheritance, linearized inheritance, and control flow directly in Solidity editors.

Current graph coverage includes:

- import graphs
- inheritance graphs
- linearized inheritance graphs
- function-level control-flow graphs

Current control-flow graph coverage includes:

- graphs are emitted per implemented function, constructor, fallback, receive function, or modifier
- modifier expansion is inlined across the resolved inheritance chain when the modifier definition can be loaded from indexed or on-disk Solidity sources
- graph payloads and previews now carry semantic node and edge kinds for branches, loops, modifiers, calls, terminals, assembly, and structural flow
- inline assembly expands Yul declarations, calls, branches, switches, loops, and `leave` edges instead of collapsing to a single opaque node
- Yul function definitions build callable subgraphs with call edges from local call sites
- terminal Yul/EVM builtins such as `revert`, `return`, `stop`, `invalid`, and `selfdestruct` terminate the local flow path instead of rendering as ordinary calls

The inlay-hint surface currently includes:

- parameter-name hints only appear where solgrid can resolve an unambiguous call signature
- selector hints are currently limited to ABI-visible function declarations and interface IDs
- inheritance hints surface contract linearization on derived declarations, accessible inherited members, and nearest override or implementation sources for overriding members
- detector-aware hints summarize metadata-backed detector findings on the nearest stable declaration and include severity/confidence in the summary label

The semantic-token surface currently includes:

- contract, interface, library, struct, enum, UDVT, event, function, modifier, variable, property, parameter, and enum-member declarations
- high-signal resolved references for local symbols and imported namespace-qualified types or members
- readonly modifiers for enum members plus constant and immutable state variables at declaration, direct reference, and member-resolved reference sites
- named import aliases preserve the imported symbol kind for common cases such as contracts and custom errors instead of collapsing to a generic type token
- ambiguous plain-import collisions stay uncolored instead of guessing a semantic token kind from the first matching import
- full-document, full-delta, and visible-range token requests through standard LSP semantic-token providers
- conservative declaration-vs-reference coloring for namespace imports and common Solidity symbol sites without relying on extension-specific UI

### Coverage UI

The VS Code extension also ingests LCOV and Cobertura coverage artifacts directly from the workspace and exposes them without requiring a native runtime:

- coverage files are discovered from `solgrid.coverage.artifacts`
- the `solgrid Coverage` view summarizes per-file line and branch coverage
- uncovered lines are decorated with error-colored whole-line markers
- partially covered branch lines are decorated with warning-colored markers
- the tree defaults to actionable files but can also show all covered Solidity files
- command-palette and coverage-view actions can auto-pick the preferred coverage provider for the current workspace folder
- explicit commands can still run Foundry LCOV or Cobertura coverage, Hardhat coverage, or a configured custom coverage command directly in the current workspace folder
- a custom coverage command can be configured with `solgrid.coverage.customCommand`

Current coverage support is intentionally focused on imported artifact ingestion plus lightweight provider invocation rather than a native runtime. Native execution ownership is still out of scope, but the current viewer and command surface cover the common Foundry, Hardhat, and custom-command workflows.

### Call Hierarchy

Call hierarchy is intentionally conservative today:

- declaration preparation works on function and modifier names plus resolvable call sites
- incoming and outgoing calls are built from direct call expressions and modifier invocations
- ambiguous overloaded call sites are rejected instead of guessed
- broader interprocedural expansion and constructor-heavy rename-style graph edits remain future work

### Rename Support

Rename support is intentionally conservative today:

- `prepareRename` and `rename` are offered when every resolved occurrence can be updated mechanically without guessing alias semantics
- this covers lexical/local rename flows plus safe cross-file exported-symbol graphs through unaliased imports, declaration-site aliased imports, and namespace-member references
- ambiguous alias-driven usage-site renames and broader multi-file rewrites are still rejected instead of guessed

That tradeoff keeps rename semantics aligned with the current symbol-stability guarantees without forcing symbol-guessing into the rename path.
