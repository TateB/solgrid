# solgrid

A blazing-fast, Rust-native Solidity linter and formatter. One tool to replace solhint, forge fmt, and prettier-plugin-solidity.

## Features

- **90+ lint rules** across security, best practices, naming, gas optimization, style, and documentation
- **Built-in formatter** with output compatible with prettier-plugin-solidity and forge fmt
- **Three-tier auto-fix** — safe, suggestion, and dangerous fixes with full developer control
- **Native LSP server** for real-time editor diagnostics and code actions
- **VSCode extension** with fix-on-save and format-on-save
- **Prettier plugin** via NAPI-RS bindings for seamless integration
- **Sub-second performance** on entire projects, powered by the Solar parser

## Architecture

See [ARCHITECTURE.md](./ARCHITECTURE.md) for the full design document covering:

- Multi-crate Rust workspace structure
- Rule engine design with two-pass analysis (syntactic + semantic)
- Complete rule set with 90 rules across 6 categories
- Formatter with chunk-based intermediate representation
- LSP server and VSCode extension design
- Configuration system (`solgrid.toml`)
- CLI interface and output formats
- Performance goals and strategies
- Phased development roadmap

## Status

**Pre-development** — Architecture phase. See the [roadmap](./ARCHITECTURE.md#11-project-roadmap) for planned milestones.
