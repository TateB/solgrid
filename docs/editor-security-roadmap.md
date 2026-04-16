# IDE and Security Expansion Roadmap

This document captures the planning work for bringing `solgrid` up to, and eventually beyond, the Wake-powered `solidity-for-vscode` experience in editor and security tooling.

The scope here is intentionally focused on static analysis and editor UX. It does not cover deploy/interact flows, local chain management, or network forking.

## Goals

- Add missing compiler-aware diagnostics
- Add a detector platform for vulnerability and code-quality findings
- Add a security overview in the VS Code extension
- Close the navigation gap with references, document links, outline, symbols, and code lens
- Add graph tooling for control flow, inheritance, linearized inheritance, and imports
- Add inlay hints and coverage UI
- Add additional high-value editor features where `solgrid` can reasonably surpass Wake

## Current Baseline

`solgrid` already has a strong foundation that this roadmap can build on:

- CLI lint/fix/fmt workflows with JSON, GitHub annotations, and SARIF outputs
- LSP diagnostics, code actions, formatting, range formatting, fix-on-save, and format-on-save
- Hover, completion, go-to-definition, signature help, and auto-import completion
- A VS Code extension that is currently a thin client over the Rust language server

This means the roadmap is not about starting from zero. It is about broadening the semantic model and product surface.

## Design Constraints

### 1. Keep the Rust-native architecture

The goal is feature parity, not architectural parity. We should not reproduce Wake's Python stack or extension-host-heavy design if the same outcomes can live in reusable Rust crates.

The default path should be `solar` / `solar-sema` first, extended upstream or locally as needed.

### 2. Separate concerns cleanly

`solgrid` should distinguish:

- parser diagnostics
- compiler diagnostics
- lint and style findings
- semantic or interprocedural detectors
- UI-only projections such as graph views, code lens, and tree views

That separation keeps CLI output, LSP behavior, suppression, and testing coherent.

### 3. Reuse features outside VS Code when practical

If we build a graph generator, detector engine, or compiler diagnostics pipeline, it should ideally have:

- a reusable Rust API
- LSP integration
- optional CLI entry points or machine-readable export

This keeps `solgrid` differentiated as a platform, not just an extension.

### 4. Start with ecosystems we already understand

The shortest path is strong support for Foundry-style workspaces first:

- nearest-config discovery
- `foundry.toml`
- `remappings.txt`
- standard Solidity import layouts

Hardhat and mixed monorepos can follow once the project model is stable.

### 5. Favor artifact ingestion over runtime ownership where possible

Coverage is the clearest example. `solgrid` does not need to become a full smart-contract runtime to offer a strong coverage UI. The first milestone should consume existing coverage outputs from Foundry or Hardhat.

### 6. Keep `solc` out of the product hot path

`solc` is valuable as a compatibility oracle and test corpus, but not as the default runtime dependency for `solgrid`.

That means:

- no `solc` shell-out in the LSP hot path
- no architecture that requires `solc` to make the editor experience usable
- use `solc`, Wake fixtures, and other external suites primarily for conformance and differential testing

## Recommended Workstreams

### 1. Shared Project Model and Semantic Backend

This is the critical foundation. The current LSP has enough symbol logic for hover, completion, and definition lookup, but not enough project semantics for compiler parity, references, outline, or graph-heavy features.

#### Deliverables

- A shared workspace/project model crate for:
  - discovered Solidity files
  - import graph and remapping resolution
  - symbol tables and declaration index
  - compilation units
  - nearest-config and language/version settings resolution
  - incremental invalidation and caching
- A semantic backend abstraction for:
  - integrating `solar` / `solar-sema` first
  - mapping semantic diagnostics back to source locations
  - caching semantic analysis outputs
  - supporting local extensions or upstream Solar contributions without rewriting the LSP surface

#### Notes

- The likely shape is a new crate such as `solgrid_project` or `solgrid_semantic` rather than packing more into `solgrid_server`.
- Semantic integration should be reusable by CLI and LSP. The language server should not own the entire semantic model.
- The preferred implementation order is:
  - phase 1: use `solar` / `solar-sema` directly
  - phase 2: extend Solar upstream or in local `solgrid_*` crates where needed
  - phase 3: only own larger frontend or semantic subsystems locally if Solar cannot cover them in a reasonable time frame
- External toolchains such as `solc` and Wake should be used as compatibility or differential-test oracles, not as required runtime dependencies.

#### Risks

- compilation-unit correctness across remappings, mixed pragmas, and monorepos
- performance regressions from whole-workspace recompilation
- complicated invalidation when config, remappings, or imported files change

### 2. Compiler-Aware Diagnostics

Wake surfaces compiler errors and warnings. `solgrid` currently does not provide equivalent compiler-backed editor diagnostics.

#### Deliverables

- Rust-native semantic or compiler-style diagnostics surfaced in LSP alongside `solgrid` diagnostics
- clear source labeling and severity mapping between:
  - parser failures
  - compiler diagnostics
  - lints
  - detectors
- config support for:
  - ignored compiler warnings
  - language and version targeting
  - include/exclude/import resolution behavior
- optional CLI reporting path so compiler diagnostics can also show up outside the editor

#### Acceptance Criteria

- syntax, type, import, and pragma/version errors appear in-editor with stable spans
- diagnostics stay incremental enough for real editing workflows
- semantic backend limitations or unavailable features degrade gracefully and explain why diagnostics are missing

#### Suggested Phasing

- Phase A: diagnostics only in VS Code/LSP
- Phase B: compiler diagnostics available in CLI checks and JSON/SARIF output

### 3. Detector Platform

Wake's strongest non-runtime feature is its detector surface and detector metadata. `solgrid` already has many security and best-practice rules, but they are not packaged as an analyst-facing detector system.

#### Deliverables

- A normalized detector model with:
  - id
  - title
  - category
  - severity
  - confidence
  - help/docs URL
  - suppressibility / ignore behavior
  - optional fix metadata
- A split between:
  - rule-based detectors that can reuse current lint rules
  - semantic detectors that require symbol graphs or compiler output
  - interprocedural detectors that require call graph / control-flow data
- Detector-specific output for UI grouping and filtering

#### Suggested Sequence

- Stage 1: wrap existing `security/*`, selected `best-practices/*`, and selected `docs/*` rules as detector-friendly findings
- Stage 2: add semantic detectors that become possible once the project model and semantic backend land
- Stage 3: define a native extension point for custom detectors and printers

#### High-Value Detector Targets

- stronger reentrancy analysis
- external-call ordering and state-write sequencing
- taint-like checks around `delegatecall`, low-level calls, and unchecked return values
- compiler/version and import hygiene issues
- inheritance/layout footguns that need cross-file awareness

### 4. Security Overview

Wake's security sidebar is not just about more findings; it is about triage UX. `solgrid` should treat this as a first-class extension feature.

#### Deliverables

- VS Code tree view for workspace findings
- grouping modes:
  - by file
  - by severity
  - by confidence
  - by detector or rule
- filter controls
- commands to:
  - jump to code
  - open rule or detector docs
  - rerun analysis
  - toggle ignored findings
  - apply available fixes

#### Nice-to-Have Follow-Ups

- workspace summary panel with counts and trends
- ignore baselines committed in-repo
- stale/introduced finding comparison for CI and review flows

### 5. Navigation and Symbol UX

This is the most obvious gap between `solgrid` and the Wake extension.

#### Required Features

- find references
- document links for imports and relevant URIs
- document symbols
- workspace symbols
- contract outline in VS Code via symbol providers
- richer symbol indexing beyond completion-only use

#### Additional Features Worth Pulling Forward

- rename support where it can be made safe
- call hierarchy
- semantic tokens for clearer Solidity code navigation and readability

#### Dependencies

- reliable declaration/reference index
- cross-file import and alias resolution
- a project model that can answer workspace-wide queries incrementally

### 6. Code Lens

Wake uses code lens to make analysis and navigation feel immediate. `solgrid` should adopt the same pattern once reference counts and graph generation exist.

#### Suggested Code Lens Types

- reference counts
- function selectors and interface ids
- inheritance graph entry points
- linearized inheritance graph entry points
- control-flow graph entry points
- detector summaries on especially interesting declarations

#### Recommendation

Start with reference counts and selector-oriented code lens. They are high-signal and do not require webview plumbing on day one.

### 7. Graph and Visualization Tooling

This should not be implemented as VS Code-only logic. The better architecture is a reusable graph intermediate representation plus multiple render targets.

#### Core Deliverables

- graph IR shared across CLI and extension
- generators for:
  - imports graph
  - inheritance graph
  - linearized inheritance graph
  - control-flow graph
- export formats:
  - JSON
  - DOT
  - SVG or HTML preview assets

#### VS Code Deliverables

- graph preview commands
- clickable graph nodes that return to source
- code lens entry points for graph generation

#### Order of Work

- imports graph
- inheritance graph
- linearized inheritance graph
- control-flow graph

That order tracks implementation risk and user value.

### 8. Inlay Hints

Wake exposes hints broadly; `solgrid` should be selective and aim for high-signal hints that make Solidity easier to read.

#### Recommended Hint Types

- parameter-name hints at call sites
- selector or interface-id hints where relevant
- inheritance-origin hints for overridden or inherited members
- detector annotations where findings are attached to declarations
- optional mutability/storage-location context if it proves useful

#### Recommendation

Add settings to enable hint categories independently. Inlay hints are easy to overdo.

### 9. Coverage UI

Coverage is useful, but this is the one area where copying Wake's shape directly would likely drag `solgrid` into runtime ownership too early.

#### Recommended Scope

- Phase 1: ingest coverage artifacts from existing tools
  - Foundry LCOV or JSON
  - Hardhat-compatible LCOV where possible
- Phase 2: VS Code decorations and summary tree
- Phase 3: optional convenience commands to invoke known coverage providers

#### Explicit Recommendation

Do not start by building a native EVM execution or test runner. A coverage viewer is enough to unlock most of the user value.

### 10. Extension Shell and Product Polish

Wake also wins on shell-level UX around commands and workflow discoverability. `solgrid` should add the useful parts without dragging in unnecessary complexity.

#### Candidates

- richer command palette entries
- walkthrough or onboarding flow
- output channel for compiler and detector logs
- binary and semantic-backend health diagnostics
- extension status bar item
- remapping refresh commands
- better error surfaces when bundled binaries or semantic backend dependencies fail

## Proposed Delivery Plan

### Milestone 1: Semantic Foundation

- shared project model
- incremental workspace index redesign
- semantic backend abstraction
- document symbols and workspace symbols

This unlocks nearly every later milestone.

### Milestone 2: Navigation Parity

- find references
- document links
- contract outline
- first wave of code lens
- semantic tokens if the symbol model is mature enough

This gives users obvious day-to-day editor value quickly.

### Milestone 3: Compiler and Detector Surface

- compiler-aware diagnostics
- detector metadata normalization
- security overview tree
- rerun/filter/group UX

This is the point where `solgrid` starts to feel meaningfully closer to Wake in security workflows.

### Milestone 4: Graphs and Hints

- complete: imports, inheritance, linearized inheritance, and control-flow graphs, graph-entry code lens, inline-assembly Yul expansion with local function subgraphs and terminal builtin semantics, CLI graph export surfaces in JSON, Mermaid, and DOT, and parameter-name plus selector/interface-ID, inheritance-origin, inherited-member, contract-lineage, and detector-aware declaration inlay hints

### Milestone 5: Coverage and Advanced Analysis

- complete: LCOV/Cobertura coverage artifact ingestion, VS Code coverage decorations and summary view, provider-aware Foundry, Hardhat, and custom coverage run commands, stronger same-file interprocedural semantic detectors for helper-mediated delegatecall and ETH-transfer flows, conservative rename for same-file plus safe cross-file symbol graphs, and conservative call hierarchy for resolvable function and modifier declarations/call sites
- future depth work: cross-file or inheritance-aware interprocedural detectors, deeper semantic-token coverage beyond the shipped full/delta/range declaration, readonly, high-signal reference, and conservative ambiguity-handling slice, broader rename/call-hierarchy support under ambiguous overload or alias graphs, and any provider-specific coverage polish beyond the shipped command and artifact surface

## Testing and Verification Plan

Every milestone should add tests at multiple layers:

- Rust unit tests for symbol resolution, compiler mapping, detector logic, and graph generation
- LSP integration tests for capabilities and payloads
- VS Code extension tests for tree views, code lens, links, hints, and coverage decorations
- fixture workspaces that model:
  - simple single-package Foundry projects
  - remapping-heavy projects
  - cross-file inheritance and interface usage
  - semantic and compiler-style error scenarios
  - large workspaces for incremental-performance checks

Validation should also include differential and corpus-based testing against external references:

- Solidity ecosystem fixture corpora
- `solc` behavior where it is still the best compatibility oracle
- Wake fixtures or similar real-world detector scenarios

Performance budgets should be explicit before milestone 3 ships:

- references and symbols must remain interactive on medium projects
- semantic diagnostics must be debounced and cached
- graph generation must avoid full-workspace reparsing when possible

## Recommended Future CLI Surface

To keep the platform story strong, the following commands are worth reserving space for:

- `solgrid graph imports`
- `solgrid graph inheritance`
- `solgrid graph linearized-inheritance`
- `solgrid graph cfg`
- `solgrid detect`
- `solgrid references`
- `solgrid coverage show` or artifact import helpers

These do not need to land immediately, but designing the internals with CLI reuse in mind will avoid repainting the architecture later.

## Useful Extras Beyond Wake Parity

If the goal is not just parity but a better product, these are the strongest extras to consider:

- semantic tokens tuned for Solidity constructs
- safe rename support for locally provable symbols
- call hierarchy
- baseline files for suppressing known findings in large repos
- diff-aware security summaries for pull requests
- graph export usable in CI and docs, not only inside VS Code
- machine-readable detector metadata shared by CLI, SARIF, and extension UIs

## Open Questions

- Should compiler diagnostics be available in `solgrid check` by default, or only behind an opt-in mode until performance is proven?
- Which missing semantic capabilities should be pushed upstream into Solar versus maintained locally in `solgrid_*` crates?
- Should detector confidence be hand-authored metadata, computed heuristically, or omitted until enough semantic signal exists?
- How aggressively should the extension pursue Wake-style UI surface area versus keeping the client intentionally thin?

Those decisions should be resolved before implementation starts on milestone 1.
