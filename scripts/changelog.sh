#!/usr/bin/env bash
#
# changelog.sh — CHANGELOG.md utilities for the solgrid release process
#
# The --stamp subcommand is called automatically by version.sh --set.
# The --check and --extract subcommands are used by CI and the release workflow.
#
# Usage:
#   ./scripts/changelog.sh --stamp X.Y.Z [DATE]  # Stamp [Unreleased] as vX.Y.Z
#   ./scripts/changelog.sh --check X.Y.Z          # Verify changelog has entry for X.Y.Z
#   ./scripts/changelog.sh --extract X.Y.Z        # Print release notes body for X.Y.Z
#

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHANGELOG="$REPO_ROOT/CHANGELOG.md"
GITHUB_REPO="TateB/solgrid"

usage() {
  echo "Usage:"
  echo "  $0 --stamp X.Y.Z [DATE]   Stamp [Unreleased] as a new version"
  echo "  $0 --check X.Y.Z          Verify changelog has entry for version"
  echo "  $0 --extract X.Y.Z        Print release notes for version"
  exit 1
}

MODE="${1:-}"
VERSION="${2:-}"

if [ -z "$MODE" ] || [ -z "$VERSION" ]; then
  usage
fi

# Validate semver format
if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+'; then
  echo "Error: Version must be in semver format (X.Y.Z), got: $VERSION" >&2
  exit 1
fi

case "$MODE" in
  --stamp)
    DATE="${3:-$(date +%Y-%m-%d)}"

    if ! grep -q '^## \[Unreleased\]' "$CHANGELOG"; then
      echo "Error: No [Unreleased] section found in CHANGELOG.md" >&2
      exit 1
    fi

    if grep -q "^## \[$VERSION\]" "$CHANGELOG"; then
      echo "Error: Version $VERSION already exists in CHANGELOG.md" >&2
      exit 1
    fi

    # Find the previous version for comparison links
    PREV_VERSION=$(grep -oE '^## \[[0-9]+\.[0-9]+\.[0-9]+\]' "$CHANGELOG" | head -1 | sed 's/## \[\(.*\)\]/\1/')

    # Replace [Unreleased] header with versioned header, add fresh [Unreleased]
    sed -i '' "s/^## \[Unreleased\]/## [Unreleased]\n\n## [$VERSION] - $DATE/" "$CHANGELOG"

    # Update the [Unreleased] comparison link at the bottom
    sed -i '' "s|^\[Unreleased\]:.*|[Unreleased]: https://github.com/$GITHUB_REPO/compare/v$VERSION...HEAD|" "$CHANGELOG"

    # Add the new version comparison link
    if [ -n "$PREV_VERSION" ]; then
      sed -i '' "s|^\[$PREV_VERSION\]:.*|[$VERSION]: https://github.com/$GITHUB_REPO/compare/v$PREV_VERSION...v$VERSION\n[$PREV_VERSION]: https://github.com/$GITHUB_REPO/releases/tag/v$PREV_VERSION|" "$CHANGELOG"
    else
      echo "[$VERSION]: https://github.com/$GITHUB_REPO/releases/tag/v$VERSION" >> "$CHANGELOG"
    fi

    echo "  Updated CHANGELOG.md: [Unreleased] -> [$VERSION] - $DATE"
    ;;

  --check)
    if grep -q "^## \[$VERSION\]" "$CHANGELOG"; then
      echo "CHANGELOG.md has entry for $VERSION"
    else
      echo "ERROR: CHANGELOG.md is missing entry for $VERSION" >&2
      echo "This usually means ./scripts/version.sh --set was not used to bump the version." >&2
      exit 1
    fi
    ;;

  --extract)
    if ! grep -q "^## \[$VERSION\]" "$CHANGELOG"; then
      echo "ERROR: No entry for $VERSION in CHANGELOG.md" >&2
      exit 1
    fi

    # Extract everything between ## [VERSION] and the next ## heading
    sed -n "/^## \[$VERSION\]/,/^## \[/{/^## \[$VERSION\]/d;/^## \[/d;p;}" "$CHANGELOG"
    ;;

  *)
    usage
    ;;
esac
