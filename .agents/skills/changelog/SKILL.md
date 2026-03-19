---
name: changelog
description: Add or update CHANGELOG.md entries. Use this skill whenever making code changes that need a changelog entry, when CI fails the "Changelog" check, when bumping versions, or when the user mentions changelog, release notes, or versioning.
---

Analyze the changes on the current branch (compared to main) and add an appropriate entry to CHANGELOG.md under `## [Unreleased]`.

1. Run `git diff main...HEAD` and `git log main..HEAD --oneline` to understand what changed
2. Determine the category: Added, Changed, Deprecated, Removed, Fixed, or Security
3. Write a concise entry — one line per logical change, starting with `- `
4. If the category heading (e.g. `### Added`) doesn't exist under `[Unreleased]`, create it
5. Add the entry under the correct heading

Format:
```markdown
## [Unreleased]

### Added
- New feature description

### Fixed
- Bug fix description
```

If the PR has no consumer-facing changes (CI config, internal tooling, docs-only), apply the `skip-changelog` label instead of adding an entry.

For version bumps, use `just version set X.Y.Z` which updates all packages and stamps the changelog automatically.
