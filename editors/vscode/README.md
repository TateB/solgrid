# solgrid for VS Code

solgrid brings Rust-native Solidity linting and formatting to VS Code.

## Features

- Real-time diagnostics from the `solgrid` language server
- Format-on-save and fix-on-save support
- Configurable nearest-file `solgrid.toml` discovery, with optional explicit `solgrid.configPath`
- Cross-file navigation, references, rename, call hierarchy, semantic tokens, inlay hints, and CodeLens actions
- Security overview with filtering, grouping, baselines, targeted fixes, and inline suppressions
- Import, inheritance, linearized-inheritance, and control-flow graph previews
- LCOV and Cobertura artifact ingestion with coverage summaries, source decorations, and actionable uncovered lines
- Built-in Foundry LCOV and locally installed Hardhat coverage runners, plus an argv-safe custom runner
- Bundled `solgrid` binary in the platform-specific VSIX builds for:
  - `darwin-arm64`
  - `linux-arm64`
  - `linux-x64`
  - `win32-x64`

## Installation

Install the extension from the Visual Studio Marketplace or Open VSX. VS Code 1.82 or newer is required.

On the supported platform-specific VSIX targets above, the extension uses its bundled `solgrid` binary by default. On other platforms, install the `solgrid` CLI separately and make sure it is on your `PATH`, or point the extension at it with `solgrid.path`.

## Configuration

The extension contributes these settings:

- `solgrid.enable`
- `solgrid.path`
- `solgrid.fixOnSave`
- `solgrid.unsafeFixesOnSave` (the extension still reads the legacy `solgrid.fixOnSave.unsafeFixes` value when the replacement is not explicitly configured)
- `solgrid.formatOnSave`
- `solgrid.configPath` to pin the workspace to a specific config file instead of nearest-file discovery
- `solgrid.coverage.enable`
- `solgrid.coverage.artifacts` for LCOV and Cobertura discovery globs
- `solgrid.coverage.autoRefreshAfterRun`
- `solgrid.coverage.customCommand` as an argv array, for example `["pnpm", "run", "coverage"]`

Coverage artifact browsing and coverage commands are independent of the language server. They remain available when `solgrid.enable` is false as long as `solgrid.coverage.enable` is true. The Hardhat runner uses `npx --no-install`, so it will not download packages implicitly.

Changing `solgrid.enable` requires a VS Code window reload because it starts or stops the language-server process and its editor integrations. The extension offers **Reload Window** when this setting changes. Coverage remains independent and does not require the language server.

Foundry currently emits LCOV through `forge coverage --report lcov`; Cobertura XML remains supported for ingestion when another coverage tool produces it.
When both formats describe the same source line, line-hit totals use the larger per-format aggregate to avoid double-counting duplicate reports. LCOV branch data takes precedence only on lines where it is present because it retains stable branch identities; Cobertura-only branch lines are still included.

## Project Links

- Repository: <https://github.com/TateB/solgrid>
- Issues: <https://github.com/TateB/solgrid/issues>
- Documentation: <https://github.com/TateB/solgrid/tree/main/docs>

## License

MIT
