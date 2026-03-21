# solgrid for VS Code

solgrid brings Rust-native Solidity linting and formatting to VS Code.

## Features

- Real-time diagnostics from the `solgrid` language server
- Format-on-save and fix-on-save support
- Configurable nearest-file `solgrid.toml` discovery, with optional explicit `solgrid.configPath`
- Bundled `solgrid` binary in the platform-specific VSIX builds for:
  - `darwin-arm64`
  - `linux-arm64`
  - `linux-x64`
  - `win32-x64`

## Installation

Install the extension from the Visual Studio Marketplace or Open VSX.

On the supported platform-specific VSIX targets above, the extension uses its bundled `solgrid` binary by default. On other platforms, install the `solgrid` CLI separately and make sure it is on your `PATH`, or point the extension at it with `solgrid.path`.

## Configuration

The extension contributes these settings:

- `solgrid.enable`
- `solgrid.path`
- `solgrid.fixOnSave`
- `solgrid.fixOnSave.unsafeFixes`
- `solgrid.formatOnSave`
- `solgrid.configPath` to pin the workspace to a specific config file instead of nearest-file discovery

## Project Links

- Repository: <https://github.com/TateB/solgrid>
- Issues: <https://github.com/TateB/solgrid/issues>
- Documentation: <https://github.com/TateB/solgrid/tree/main/docs>

## License

MIT
