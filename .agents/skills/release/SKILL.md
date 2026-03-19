---
name: release
description: Create a versioned release. Use when the user asks to release, bump the version, cut a release, or publish a new version. Must be run from the main branch.
---

Create a release for solgrid by running the Release PR workflow, which bumps versions across all packages, stamps the changelog, and opens a PR to main.

1. Confirm you are on the `main` branch — abort if not
2. Ask the user for the version number if not provided (semver `X.Y.Z`)
3. Trigger the Release PR workflow:
   ```bash
   gh workflow run release-pr.yml -f version=X.Y.Z
   ```
4. Confirm the workflow started successfully:
   ```bash
   gh run list --workflow=release-pr.yml --limit=1
   ```

The workflow will:
- Create a `release/vX.Y.Z` branch
- Run `scripts/version.sh --set X.Y.Z` (updates Cargo.toml, all package.json files, and stamps CHANGELOG.md)
- Open a PR to main with the `skip-changelog` label

After the PR is reviewed and merged, the **Release Tag** workflow automatically creates the `vX.Y.Z` tag, which triggers the full release pipeline (multi-platform builds, GitHub Release, npm publish, Open VSX publish).

If the `gh workflow run` command is unavailable or fails, fall back to the manual flow:
1. `git checkout -b release/vX.Y.Z`
2. `bash scripts/version.sh --set X.Y.Z`
3. Review CHANGELOG.md — edit the new `[X.Y.Z]` section if needed
4. `git add -A && git commit -m 'chore: bump version to X.Y.Z'`
5. `git push origin release/vX.Y.Z`
6. Open a PR to main with the `skip-changelog` label
