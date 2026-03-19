# solgrid

Every PR must update CHANGELOG.md under `## [Unreleased]` (CI enforces this). Use [Keep a Changelog](https://keepachangelog.com/) categories: Added, Changed, Fixed, Removed, Deprecated, Security. Skip with `skip-changelog` label.

Bump versions with `just version set X.Y.Z` — this updates all packages and stamps the changelog.

Run `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --all --check` before committing.
