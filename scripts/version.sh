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

# Portable sed -i (BSD vs GNU)
if [[ "$OSTYPE" == darwin* ]]; then
  sedi() { sed -i '' "$@"; }
else
  sedi() { sed -i "$@"; }
fi

CARGO_TOML="$REPO_ROOT/Cargo.toml"
CARGO_LOCK="$REPO_ROOT/Cargo.lock"
VSCODE_PKG="$REPO_ROOT/editors/vscode/package.json"
PRETTIER_PKG="$REPO_ROOT/packages/prettier-plugin-solgrid/package.json"
SOLGRID_PKG="$REPO_ROOT/packages/solgrid/package.json"
CLI_DARWIN_ARM64_PKG="$REPO_ROOT/packages/solgrid/npm/cli-darwin-arm64/package.json"
CLI_LINUX_ARM64_PKG="$REPO_ROOT/packages/solgrid/npm/cli-linux-arm64/package.json"
CLI_LINUX_X64_PKG="$REPO_ROOT/packages/solgrid/npm/cli-linux-x64/package.json"
CLI_WIN32_X64_PKG="$REPO_ROOT/packages/solgrid/npm/cli-win32-x64/package.json"
NAPI_DARWIN_ARM64_PKG="$REPO_ROOT/packages/prettier-plugin-solgrid/npm/napi-darwin-arm64/package.json"
NAPI_LINUX_ARM64_PKG="$REPO_ROOT/packages/prettier-plugin-solgrid/npm/napi-linux-arm64/package.json"
NAPI_LINUX_X64_PKG="$REPO_ROOT/packages/prettier-plugin-solgrid/npm/napi-linux-x64/package.json"
NAPI_WIN32_X64_PKG="$REPO_ROOT/packages/prettier-plugin-solgrid/npm/napi-win32-x64/package.json"

# Extract version from Cargo.toml [workspace.package] section
get_cargo_version() {
  grep -A5 '^\[workspace\.package\]' "$CARGO_TOML" | grep '^version' | head -1 | sed 's/.*"\(.*\)".*/\1/'
}

# Extract version from a package.json
get_json_version() {
  grep '"version"' "$1" | head -1 | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([0-9A-Za-z.+-]*\)".*/\1/p'
}

is_valid_semver() {
  local version="$1"
  local core='(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)'
  local prerelease_id='(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
  local build_id='[0-9A-Za-z-]+'
  local pattern="^${core}(-${prerelease_id}(\.${prerelease_id})*)?(\+${build_id}(\.${build_id})*)?$"
  [[ "$version" =~ $pattern ]]
}

# Print the paths listed in the workspace `members` array, one per line.
get_workspace_members() {
  awk '
    /^[[:space:]]*members[[:space:]]*=/ { collecting = 1 }
    collecting {
      line = $0
      while (match(line, /"[^"]+"/)) {
        print substr(line, RSTART + 1, RLENGTH - 2)
        line = substr(line, RSTART + RLENGTH)
      }
      if (index($0, "]") > 0) {
        exit
      }
    }
  ' "$CARGO_TOML"
}

get_manifest_package_name() {
  awk '
    /^\[package\][[:space:]]*$/ { in_package = 1; next }
    in_package && /^\[/ { exit }
    in_package && /^[[:space:]]*name[[:space:]]*=/ {
      line = $0
      sub(/^[^"]*"/, "", line)
      sub(/".*/, "", line)
      print line
      exit
    }
  ' "$1"
}

get_workspace_package_names() {
  local member
  local manifest
  local package_name
  local count=0

  while IFS= read -r member; do
    manifest="$REPO_ROOT/$member/Cargo.toml"
    if [ ! -f "$manifest" ]; then
      echo "ERROR: Workspace member manifest not found: $manifest" >&2
      return 1
    fi
    package_name=$(get_manifest_package_name "$manifest")
    if [ -z "$package_name" ]; then
      echo "ERROR: Could not read package name from workspace member: $manifest" >&2
      return 1
    fi
    echo "$package_name"
    count=$((count + 1))
  done < <(get_workspace_members)

  if [ "$count" -eq 0 ]; then
    echo "ERROR: No Cargo workspace members found in $CARGO_TOML" >&2
    return 1
  fi
}

# Restrict Cargo's update to workspace packages so a version bump refreshes their
# lockfile records without upgrading third-party dependencies.
refresh_workspace_lock_versions() {
  local version="$1"
  cargo update --workspace --quiet --manifest-path "$CARGO_TOML"
  check_workspace_lock_versions "$version"
}

check_workspace_lock_versions() {
  local expected="$1"
  local package_names

  if [ ! -f "$CARGO_LOCK" ]; then
    echo "ERROR: Cargo lockfile not found: $CARGO_LOCK" >&2
    return 1
  fi
  cargo metadata --locked --no-deps --format-version 1 \
    --manifest-path "$CARGO_TOML" >/dev/null
  package_names=$(get_workspace_package_names | paste -sd, -)

  awk -v RS='' -v packages="$package_names" -v expected="$expected" '
    BEGIN {
      count = split(packages, package_list, /,/)
      for (i = 1; i <= count; i++) {
        wanted[package_list[i]] = 1
      }
    }

    /^\[\[package\]\]/ {
      name = ""
      version = ""
      has_source = 0
      line_count = split($0, lines, /\n/)
      for (i = 1; i <= line_count; i++) {
        if (lines[i] ~ /^name = "/) {
          name = lines[i]
          sub(/^name = "/, "", name)
          sub(/".*$/, "", name)
        } else if (lines[i] ~ /^version = "/) {
          version = lines[i]
          sub(/^version = "/, "", version)
          sub(/".*$/, "", version)
        } else if (lines[i] ~ /^source = "/) {
          has_source = 1
        }
      }

      if ((name in wanted) && !has_source) {
        found[name] += 1
        actual[name] = version
      }
    }

    END {
      for (name in wanted) {
        if (found[name] != 1) {
          printf "ERROR: Expected exactly one source-less Cargo.lock entry for workspace package %s; found %d\n", name, found[name] > "/dev/stderr"
          failed = 1
        } else if (actual[name] != expected) {
          printf "ERROR: Cargo.lock package %s version (%s) does not match Cargo.toml (%s)\n", name, actual[name], expected > "/dev/stderr"
          failed = 1
        }
      }
      if (failed) {
        exit 1
      }
    }
  ' "$CARGO_LOCK"
}

# Update version in a package.json
set_json_version() {
  local file="$1"
  local version="$2"
  local old_version
  old_version=$(get_json_version "$file")
  sedi "s/\"version\": \"$old_version\"/\"version\": \"$version\"/" "$file"
}

# Update version in Cargo.toml workspace
set_cargo_version() {
  local version="$1"
  local old_version
  old_version=$(get_cargo_version)
  sedi "s/^version = \"$old_version\"/version = \"$version\"/" "$CARGO_TOML"
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
    set_json_version "$CLI_LINUX_ARM64_PKG" "$CARGO_VERSION"
    set_json_version "$CLI_LINUX_X64_PKG" "$CARGO_VERSION"
    set_json_version "$CLI_WIN32_X64_PKG" "$CARGO_VERSION"

    set_json_version "$NAPI_DARWIN_ARM64_PKG" "$CARGO_VERSION"
    set_json_version "$NAPI_LINUX_ARM64_PKG" "$CARGO_VERSION"
    set_json_version "$NAPI_LINUX_X64_PKG" "$CARGO_VERSION"
    set_json_version "$NAPI_WIN32_X64_PKG" "$CARGO_VERSION"
    refresh_workspace_lock_versions "$CARGO_VERSION"
    echo "  Updated editors/vscode/package.json -> $CARGO_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/package.json -> $CARGO_VERSION"
    echo "  Updated packages/solgrid/package.json -> $CARGO_VERSION"
    echo "  Updated packages/solgrid/npm/cli-*/package.json -> $CARGO_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/npm/napi-*/package.json -> $CARGO_VERSION"
    echo "  Updated Cargo.lock workspace packages -> $CARGO_VERSION"
    echo "Done."
    ;;

  --set)
    NEW_VERSION="${2:?Usage: $0 --set X.Y.Z}"
    if ! is_valid_semver "$NEW_VERSION"; then
      echo "Error: Version must be valid semver (X.Y.Z with optional prerelease/build metadata), got: $NEW_VERSION" >&2
      exit 1
    fi
    echo "Setting version to $NEW_VERSION across all packages"
    set_cargo_version "$NEW_VERSION"
    set_json_version "$VSCODE_PKG" "$NEW_VERSION"
    set_json_version "$PRETTIER_PKG" "$NEW_VERSION"
    set_json_version "$SOLGRID_PKG" "$NEW_VERSION"

    set_json_version "$CLI_DARWIN_ARM64_PKG" "$NEW_VERSION"
    set_json_version "$CLI_LINUX_ARM64_PKG" "$NEW_VERSION"
    set_json_version "$CLI_LINUX_X64_PKG" "$NEW_VERSION"
    set_json_version "$CLI_WIN32_X64_PKG" "$NEW_VERSION"

    set_json_version "$NAPI_DARWIN_ARM64_PKG" "$NEW_VERSION"
    set_json_version "$NAPI_LINUX_ARM64_PKG" "$NEW_VERSION"
    set_json_version "$NAPI_LINUX_X64_PKG" "$NEW_VERSION"
    set_json_version "$NAPI_WIN32_X64_PKG" "$NEW_VERSION"
    refresh_workspace_lock_versions "$NEW_VERSION"
    echo "  Updated Cargo.toml -> $NEW_VERSION"
    echo "  Updated editors/vscode/package.json -> $NEW_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/package.json -> $NEW_VERSION"
    echo "  Updated packages/solgrid/package.json -> $NEW_VERSION"
    echo "  Updated packages/solgrid/npm/cli-*/package.json -> $NEW_VERSION"
    echo "  Updated packages/prettier-plugin-solgrid/npm/napi-*/package.json -> $NEW_VERSION"
    echo "  Updated Cargo.lock workspace packages -> $NEW_VERSION"

    # Stamp CHANGELOG.md: move [Unreleased] content into the new version
    bash "$REPO_ROOT/scripts/changelog.sh" --stamp "$NEW_VERSION"

    echo ""
    echo "Next steps:"
    echo "  If using the Release PR workflow (recommended):"
    echo "    Run the 'Release PR' workflow from GitHub Actions with version $NEW_VERSION"
    echo ""
    echo "  If releasing manually:"
    echo "    1. Review CHANGELOG.md — edit the new [$NEW_VERSION] section if needed"
    echo "    2. git add -A && git commit -m 'chore: bump version to $NEW_VERSION'"
    echo "    3. Open a PR to main"
    echo "    4. After merge, tag: git tag v$NEW_VERSION && git push origin v$NEW_VERSION"
    ;;

  check|*)
    VSCODE_VERSION=$(get_json_version "$VSCODE_PKG")
    PRETTIER_VERSION=$(get_json_version "$PRETTIER_PKG")
    SOLGRID_VERSION=$(get_json_version "$SOLGRID_PKG")
    CLI_DARWIN_ARM64_VERSION=$(get_json_version "$CLI_DARWIN_ARM64_PKG")
    CLI_LINUX_ARM64_VERSION=$(get_json_version "$CLI_LINUX_ARM64_PKG")
    CLI_LINUX_X64_VERSION=$(get_json_version "$CLI_LINUX_X64_PKG")
    CLI_WIN32_X64_VERSION=$(get_json_version "$CLI_WIN32_X64_PKG")
    NAPI_DARWIN_ARM64_VERSION=$(get_json_version "$NAPI_DARWIN_ARM64_PKG")
    NAPI_LINUX_ARM64_VERSION=$(get_json_version "$NAPI_LINUX_ARM64_PKG")
    NAPI_LINUX_X64_VERSION=$(get_json_version "$NAPI_LINUX_X64_PKG")
    NAPI_WIN32_X64_VERSION=$(get_json_version "$NAPI_WIN32_X64_PKG")

    echo "Version check:"
    echo "  Cargo.toml (source of truth):              $CARGO_VERSION"
    echo "  editors/vscode/package.json:                $VSCODE_VERSION"
    echo "  packages/prettier-plugin-solgrid/package.json: $PRETTIER_VERSION"
    echo "  packages/solgrid/package.json:              $SOLGRID_VERSION"
    echo "  packages/solgrid/npm/cli-darwin-arm64/package.json:     $CLI_DARWIN_ARM64_VERSION"
    echo "  packages/solgrid/npm/cli-linux-arm64/package.json:      $CLI_LINUX_ARM64_VERSION"
    echo "  packages/solgrid/npm/cli-linux-x64/package.json:        $CLI_LINUX_X64_VERSION"
    echo "  packages/solgrid/npm/cli-win32-x64/package.json:        $CLI_WIN32_X64_VERSION"
    echo "  packages/prettier-plugin-solgrid/npm/napi-darwin-arm64/package.json:    $NAPI_DARWIN_ARM64_VERSION"
    echo "  packages/prettier-plugin-solgrid/npm/napi-linux-arm64/package.json:     $NAPI_LINUX_ARM64_VERSION"
    echo "  packages/prettier-plugin-solgrid/npm/napi-linux-x64/package.json:       $NAPI_LINUX_X64_VERSION"
    echo "  packages/prettier-plugin-solgrid/npm/napi-win32-x64/package.json:       $NAPI_WIN32_X64_VERSION"

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
    for pkg_var in CLI_DARWIN_ARM64 CLI_LINUX_ARM64 CLI_LINUX_X64 CLI_WIN32_X64 NAPI_DARWIN_ARM64 NAPI_LINUX_ARM64 NAPI_LINUX_X64 NAPI_WIN32_X64; do
      ver_var="${pkg_var}_VERSION"
      ver="${!ver_var}"
      if [ "$ver" != "$CARGO_VERSION" ]; then
        echo ""
        echo "ERROR: ${pkg_var} version ($ver) does not match Cargo.toml ($CARGO_VERSION)" >&2
        MISMATCH=1
      fi
    done
    if ! check_workspace_lock_versions "$CARGO_VERSION"; then
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
