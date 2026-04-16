# Semantic Detectors

This page documents the server-native semantic detectors that are currently emitted by `solgrid`.

These findings are not simple lint wrappers. They are produced from the Rust-native project model and AST-backed semantic analysis in the language server.

The current compiler-style semantic surface around them also covers unresolved imports, types, base contracts, overrides, using symbols, modifiers, emitted events, and reverted custom errors.

## Current Detectors

### `security-unchecked-low-level-call`

Rule ID: `security/unchecked-low-level-call`

Flags low-level `.call()`, `.delegatecall()`, and `.staticcall()` expressions whose success return value is ignored.

Current behavior:

- Fires on bare expression statements such as `target.call(data);`
- Does not fire when the return value is captured and checked
- Suppresses the broader `security/low-level-calls` heuristic when both overlap on the same site

### `security-user-controlled-delegatecall`

Rule ID: `security/user-controlled-delegatecall`

Flags `delegatecall` targets that resolve to a function parameter.

Current behavior:

- Fires when the target expression resolves to a parameter such as `implementation.delegatecall(data);`
- Also fires at same-file helper call sites when a caller argument is propagated into a helper parameter that reaches `delegatecall`
- Does not fire for state variables or other non-parameter targets
- Coexists with `security/unchecked-low-level-call` when both findings are true
- Suppresses the broader `security/low-level-calls` heuristic when both overlap on the same site

### `security-user-controlled-eth-transfer`

Rule ID: `security/user-controlled-eth-transfer`

Flags ETH-sending calls whose recipient resolves to a function parameter.

Current behavior:

- Covers `.send(...)`
- Covers one-argument ETH `.transfer(...)`
- Covers `.call{value: ...}(...)`
- Also fires at same-file helper call sites when a caller argument is propagated into a helper parameter that reaches an ETH transfer sink
- Does not fire for non-parameter targets such as state variables
- Suppresses the broader `security/arbitrary-send-eth` heuristic when both overlap on the same site

## Current Limitations

- Interprocedural propagation currently stays within uniquely resolved same-file helper calls. Cross-file, inheritance-heavy, and overload-ambiguous flows remain future work.
- Confidence is currently authored per detector rather than computed from deeper semantic evidence.
- Native semantic detectors currently expose docs and suppression metadata, but not autofixes.
- Ignored baselines in the VS Code security overview key off the current finding fingerprint, so large line shifts can require restoring and re-ignoring a finding.

## Deferred For Now

The remaining Phase 2 item intentionally deferred for now is:

- Security overview fix actions for server-native semantic detectors. The current detectors do not yet have a semantics-preserving default rewrite, so the overview supports documentation, rerun, suppression, ignore baselines, and any regular lint/code-action fixes, but not native detector autofixes.
