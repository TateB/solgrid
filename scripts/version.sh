#!/usr/bin/env bash
#
# version.sh — Synchronize version across the solgrid monorepo
#
# Single source of truth: Cargo.toml [workspace.package] version
#
# Usage:
#   ./scripts/version.sh           # Check mode — report versions, fail if out of sync
#   ./scripts/version.sh --write   # Write mode — update all package.json files to match Cargo.toml
#   ./scripts/version.sh --set X.Y.Z  # Set a new version everywhere (Cargo.toml + all package.json)
#

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CARGO_TOML="$REPO_ROOT/Cargo.toml"
VSCODE_PKG="$REPO_ROOT/editors/vscode/package.json"
PRETTIER_PKG="$REPO_ROOT/packages/prettier-plugin-solgrid/package.json"

# Extract version from Cargo.toml [workspace.package] section
get_cargo_version() {
  grep -A5 '^\[workspace\.package\]' "$CARGO_TOML" | grep '^version' | head -1 | sed 's/.*"\(.*\)".*/\1/'
}

# Extract version from a package.json
get_json_version() {
  grep '"version"' "$1" | head -1 | sed 's/.*"\([0-9][0-9.]*[0-9a-zA-Z.-]*\)".*/\1/'
}

# Update version in a package.json
set_json_version() {
  local file="$1"
  local version="$2"
  local old_version
  old_version=$(get_json_version "$file")
  sed -i "s/\"version\": \"$old_version\"/\"version\": \"$version\"/" "$file"
}

# Update version in Cargo.toml workspace
set_cargo_version() {
  local version="$1"
  local old_version
  old_version=$(get_cargo_version)
  sed -i "s/^version = \"$old_version\"/version = \"$version\"/" "$CARGO_TOML"
}

MODE="${1:-check}"
CARGO_VERSION=$(get_cargo_version)

case "$MODE" in
  --write)
    echo "Syncing all package versions to Cargo.toml version: $CARGO_VERSION"
    set_json_version "$VSCODE_PKG" "$CARGO_VERSION"
    set_json_version "$PRETTIER_PKG" "$CARGO_VERSION"
    echo "  Updated editors/vscode/package.json -> $CARGO_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/package.json -> $CARGO_VERSION"
    echo "Done."
    ;;

  --set)
    NEW_VERSION="${2:?Usage: $0 --set X.Y.Z}"
    # Validate semver-ish format
    if ! echo "$NEW_VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+'; then
      echo "Error: Version must be in semver format (X.Y.Z), got: $NEW_VERSION" >&2
      exit 1
    fi
    echo "Setting version to $NEW_VERSION across all packages"
    set_cargo_version "$NEW_VERSION"
    set_json_version "$VSCODE_PKG" "$NEW_VERSION"
    set_json_version "$PRETTIER_PKG" "$NEW_VERSION"
    echo "  Updated Cargo.toml -> $NEW_VERSION"
    echo "  Updated editors/vscode/package.json -> $NEW_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/package.json -> $NEW_VERSION"
    echo ""
    echo "Next steps:"
    echo "  1. git add -A && git commit -m 'chore: bump version to $NEW_VERSION'"
    echo "  2. git tag v$NEW_VERSION"
    echo "  3. git push origin main --tags"
    ;;

  check|*)
    VSCODE_VERSION=$(get_json_version "$VSCODE_PKG")
    PRETTIER_VERSION=$(get_json_version "$PRETTIER_PKG")

    echo "Version check:"
    echo "  Cargo.toml (source of truth):              $CARGO_VERSION"
    echo "  editors/vscode/package.json:                $VSCODE_VERSION"
    echo "  packages/prettier-plugin-solgrid/package.json: $PRETTIER_VERSION"

    MISMATCH=0
    if [ "$VSCODE_VERSION" != "$CARGO_VERSION" ]; then
      echo ""
      echo "ERROR: VSCode extension version ($VSCODE_VERSION) does not match Cargo.toml ($CARGO_VERSION)" >&2
      MISMATCH=1
    fi
    if [ "$PRETTIER_VERSION" != "$CARGO_VERSION" ]; then
      echo ""
      echo "ERROR: Prettier plugin version ($PRETTIER_VERSION) does not match Cargo.toml ($CARGO_VERSION)" >&2
      MISMATCH=1
    fi

    if [ "$MISMATCH" -eq 1 ]; then
      echo ""
      echo "Fix with: ./scripts/version.sh --write" >&2
      exit 1
    fi

    echo ""
    echo "All versions are in sync."
    ;;
esac
