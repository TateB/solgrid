# Roadmap

Planned improvements and future work for solgrid.

## Current Focus

- [x] Execute the IDE and security expansion roadmap in [docs/editor-security-roadmap.md](./docs/editor-security-roadmap.md)

## IDE and Security Platform

- [ ] Build a shared workspace/project model for import graphs, symbol indexing, compilation units, and cache invalidation
- [ ] Add compiler-aware diagnostics with a Rust-native semantic backend (`solar` / `solar-sema` first), plus version/import/type validation and LSP/CLI reporting
- [ ] Add a detector platform with reusable metadata (severity, confidence, docs, suppression) and semantic/interprocedural analyses
- [ ] Ship a VS Code security overview with grouped/filterable findings, rerun controls, docs links, and suppression flows
- [ ] Decide whether any server-native semantic detector should expose an editor autofix, or keep native-detector remediation as documentation/suppression-only until a semantics-preserving rewrite is defensible
- [ ] Extend the current navigation surface beyond the shipped references, document links, document symbols, workspace symbols, outline support, and first code lens
- [ ] Extend code lens beyond the shipped reference counts and graph entry points
- [x] Add coverage ingestion and editor UI, starting from imported workspace coverage artifacts before attempting a native runner
- [ ] Extend semantic/interprocedural detector depth beyond the shipped same-file and inherited-helper propagation, especially across arbitrary cross-file call edges and overload-heavy flows
- [ ] Decide whether coverage should grow beyond the shipped Foundry/Hardhat/custom command surface into provider-specific artifact management or stay intentionally thin
- [ ] Deepen the shipped semantic-token surface beyond the current full/delta/range declarations, readonly markings, high-signal references, multi-segment path coverage, conservative ambiguity handling, and same-semantics duplicate-resolution support, especially for harder cross-file and member-heavy cases that still need stronger kind/metadata preservation
- [ ] Add editor polish items that improve parity or surpass it: broader rename/call-hierarchy support under ambiguous graphs and richer commands

## Performance

- [ ] Memory usage optimization (< 200MB for 1000 files target)

## Future Considerations

- Plugin system for custom lint rules
- Monorepo support (per-package configuration)
- Watch mode (`solgrid check --watch`)
- Web playground using WASM bindings

---

For completed development history, see [CHANGELOG.md](./CHANGELOG.md).
