# Roadmap

Planned improvements and future work for solgrid.

## Current Focus

- [ ] Execute the IDE and security expansion roadmap in [docs/editor-security-roadmap.md](./docs/editor-security-roadmap.md)

## IDE and Security Platform

- [ ] Build a shared workspace/project model for import graphs, symbol indexing, compilation units, and cache invalidation
- [ ] Add compiler-aware diagnostics with a Rust-native semantic backend (`solar` / `solar-sema` first), plus version/import/type validation and LSP/CLI reporting
- [ ] Add a detector platform with reusable metadata (severity, confidence, docs, suppression) and semantic/interprocedural analyses
- [ ] Ship a VS Code security overview with grouped/filterable findings, rerun controls, docs links, and suppression flows
- [ ] Close the navigation gap with find references, document links, document symbols, workspace symbols, and contract outline support
- [ ] Add code lens for references, selectors, and graph actions
- [ ] Add graph tooling for control flow, inheritance, linearized inheritance, and imports with CLI export plus VS Code previews
- [ ] Add inlay hints for high-signal Solidity context (parameter names, selectors, inheritance metadata, detector annotations)
- [ ] Add coverage ingestion and editor UI, starting from Foundry/Hardhat-produced artifacts before attempting a native runner
- [ ] Add editor polish items that improve parity or surpass it: semantic tokens, rename, call hierarchy, ignore baselines, and richer commands

## Performance

- [ ] Memory usage optimization (< 200MB for 1000 files target)

## Future Considerations

- Plugin system for custom lint rules
- Monorepo support (per-package configuration)
- Watch mode (`solgrid check --watch`)
- Web playground using WASM bindings

---

For completed development history, see [CHANGELOG.md](./CHANGELOG.md).
