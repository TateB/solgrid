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
SOLGRID_PKG="$REPO_ROOT/packages/solgrid/package.json"
CLI_DARWIN_ARM64_PKG="$REPO_ROOT/packages/cli-darwin-arm64/package.json"
CLI_DARWIN_X64_PKG="$REPO_ROOT/packages/cli-darwin-x64/package.json"
CLI_LINUX_X64_PKG="$REPO_ROOT/packages/cli-linux-x64/package.json"
CLI_WIN32_X64_PKG="$REPO_ROOT/packages/cli-win32-x64/package.json"
NAPI_DARWIN_ARM64_PKG="$REPO_ROOT/packages/napi-darwin-arm64/package.json"
NAPI_DARWIN_X64_PKG="$REPO_ROOT/packages/napi-darwin-x64/package.json"
NAPI_LINUX_X64_PKG="$REPO_ROOT/packages/napi-linux-x64/package.json"
NAPI_WIN32_X64_PKG="$REPO_ROOT/packages/napi-win32-x64/package.json"

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
    set_json_version "$SOLGRID_PKG" "$CARGO_VERSION"

    set_json_version "$CLI_DARWIN_ARM64_PKG" "$CARGO_VERSION"
    set_json_version "$CLI_DARWIN_X64_PKG" "$CARGO_VERSION"
    set_json_version "$CLI_LINUX_X64_PKG" "$CARGO_VERSION"
    set_json_version "$CLI_WIN32_X64_PKG" "$CARGO_VERSION"

    set_json_version "$NAPI_DARWIN_ARM64_PKG" "$CARGO_VERSION"
    set_json_version "$NAPI_DARWIN_X64_PKG" "$CARGO_VERSION"
    set_json_version "$NAPI_LINUX_X64_PKG" "$CARGO_VERSION"
    set_json_version "$NAPI_WIN32_X64_PKG" "$CARGO_VERSION"
    echo "  Updated editors/vscode/package.json -> $CARGO_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/package.json -> $CARGO_VERSION"
    echo "  Updated packages/solgrid/package.json -> $CARGO_VERSION"
    echo "  Updated packages/cli-*/package.json -> $CARGO_VERSION"
    echo "  Updated packages/napi-*/package.json -> $CARGO_VERSION"
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
    set_json_version "$SOLGRID_PKG" "$NEW_VERSION"

    set_json_version "$CLI_DARWIN_ARM64_PKG" "$NEW_VERSION"
    set_json_version "$CLI_DARWIN_X64_PKG" "$NEW_VERSION"
    set_json_version "$CLI_LINUX_X64_PKG" "$NEW_VERSION"
    set_json_version "$CLI_WIN32_X64_PKG" "$NEW_VERSION"

    set_json_version "$NAPI_DARWIN_ARM64_PKG" "$NEW_VERSION"
    set_json_version "$NAPI_DARWIN_X64_PKG" "$NEW_VERSION"
    set_json_version "$NAPI_LINUX_X64_PKG" "$NEW_VERSION"
    set_json_version "$NAPI_WIN32_X64_PKG" "$NEW_VERSION"
    echo "  Updated Cargo.toml -> $NEW_VERSION"
    echo "  Updated editors/vscode/package.json -> $NEW_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/package.json -> $NEW_VERSION"
    echo "  Updated packages/solgrid/package.json -> $NEW_VERSION"
    echo "  Updated packages/cli-*/package.json -> $NEW_VERSION"
    echo "  Updated packages/napi-*/package.json -> $NEW_VERSION"
    echo ""
    echo "Next steps:"
    echo "  1. git add -A && git commit -m 'chore: bump version to $NEW_VERSION'"
    echo "  2. git tag v$NEW_VERSION"
    echo "  3. git push origin main --tags"
    ;;

  check|*)
    VSCODE_VERSION=$(get_json_version "$VSCODE_PKG")
    PRETTIER_VERSION=$(get_json_version "$PRETTIER_PKG")
    SOLGRID_VERSION=$(get_json_version "$SOLGRID_PKG")
    CLI_DARWIN_ARM64_VERSION=$(get_json_version "$CLI_DARWIN_ARM64_PKG")
    CLI_DARWIN_X64_VERSION=$(get_json_version "$CLI_DARWIN_X64_PKG")
    CLI_LINUX_X64_VERSION=$(get_json_version "$CLI_LINUX_X64_PKG")
    CLI_WIN32_X64_VERSION=$(get_json_version "$CLI_WIN32_X64_PKG")
    NAPI_DARWIN_ARM64_VERSION=$(get_json_version "$NAPI_DARWIN_ARM64_PKG")
    NAPI_DARWIN_X64_VERSION=$(get_json_version "$NAPI_DARWIN_X64_PKG")
    NAPI_LINUX_X64_VERSION=$(get_json_version "$NAPI_LINUX_X64_PKG")
    NAPI_WIN32_X64_VERSION=$(get_json_version "$NAPI_WIN32_X64_PKG")

    echo "Version check:"
    echo "  Cargo.toml (source of truth):              $CARGO_VERSION"
    echo "  editors/vscode/package.json:                $VSCODE_VERSION"
    echo "  packages/prettier-plugin-solgrid/package.json: $PRETTIER_VERSION"
    echo "  packages/solgrid/package.json:              $SOLGRID_VERSION"
    echo "  packages/cli-darwin-arm64/package.json:     $CLI_DARWIN_ARM64_VERSION"
    echo "  packages/cli-darwin-x64/package.json:       $CLI_DARWIN_X64_VERSION"
    echo "  packages/cli-linux-x64/package.json:        $CLI_LINUX_X64_VERSION"
    echo "  packages/cli-win32-x64/package.json:        $CLI_WIN32_X64_VERSION"
    echo "  packages/napi-darwin-arm64/package.json:    $NAPI_DARWIN_ARM64_VERSION"
    echo "  packages/napi-darwin-x64/package.json:      $NAPI_DARWIN_X64_VERSION"
    echo "  packages/napi-linux-x64/package.json:       $NAPI_LINUX_X64_VERSION"
    echo "  packages/napi-win32-x64/package.json:       $NAPI_WIN32_X64_VERSION"

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
    if [ "$SOLGRID_VERSION" != "$CARGO_VERSION" ]; then
      echo ""
      echo "ERROR: solgrid npm package version ($SOLGRID_VERSION) does not match Cargo.toml ($CARGO_VERSION)" >&2
      MISMATCH=1
    fi
    for pkg_var in CLI_DARWIN_ARM64 CLI_DARWIN_X64 CLI_LINUX_X64 CLI_WIN32_X64 NAPI_DARWIN_ARM64 NAPI_DARWIN_X64 NAPI_LINUX_X64 NAPI_WIN32_X64; do
      ver_var="${pkg_var}_VERSION"
      ver="${!ver_var}"
      if [ "$ver" != "$CARGO_VERSION" ]; then
        echo ""
        echo "ERROR: ${pkg_var} version ($ver) does not match Cargo.toml ($CARGO_VERSION)" >&2
        MISMATCH=1
      fi
    done

    if [ "$MISMATCH" -eq 1 ]; then
      echo ""
      echo "Fix with: ./scripts/version.sh --write" >&2
      exit 1
    fi

    echo ""
    echo "All versions are in sync."
    ;;
esac
