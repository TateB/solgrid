#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_ROOT=$(mktemp -d "${TMPDIR:-/tmp}/solgrid-version-test.XXXXXX")
trap 'rm -rf "$TEST_ROOT"' EXIT

PACKAGE_JSON_PATHS=(
  "editors/vscode/package.json"
  "packages/prettier-plugin-solgrid/package.json"
  "packages/solgrid/package.json"
  "packages/solgrid/npm/cli-darwin-arm64/package.json"
  "packages/solgrid/npm/cli-linux-arm64/package.json"
  "packages/solgrid/npm/cli-linux-x64/package.json"
  "packages/solgrid/npm/cli-win32-x64/package.json"
  "packages/prettier-plugin-solgrid/npm/napi-darwin-arm64/package.json"
  "packages/prettier-plugin-solgrid/npm/napi-linux-arm64/package.json"
  "packages/prettier-plugin-solgrid/npm/napi-linux-x64/package.json"
  "packages/prettier-plugin-solgrid/npm/napi-win32-x64/package.json"
)

fail() {
  echo "version_test.sh: $*" >&2
  exit 1
}

assert_contains() {
  local file="$1"
  local expected="$2"
  grep -Fq -- "$expected" "$file" ||
    fail "expected $file to contain: $expected"
}

new_fixture() {
  local root
  local package_json
  root=$(mktemp -d "$TEST_ROOT/fixture.XXXXXX")

  mkdir -p "$root/scripts" "$root/crates/alpha/src" "$root/crates/beta/src"
  cp "$SCRIPT_DIR/version.sh" "$SCRIPT_DIR/changelog.sh" "$root/scripts/"

  cat > "$root/Cargo.toml" <<'EOF'
[workspace]
resolver = "2"
members = [
  "crates/alpha",
  "crates/beta",
]

[workspace.package]
version = "0.0.9"
edition = "2021"
EOF

  cat > "$root/crates/alpha/Cargo.toml" <<'EOF'
[package]
name = "fixture_alpha"
version.workspace = true
edition.workspace = true
EOF
  cat > "$root/crates/beta/Cargo.toml" <<'EOF'
[package]
name = "fixture_beta"
version.workspace = true
edition.workspace = true
EOF
  printf 'pub fn alpha() {}\n' > "$root/crates/alpha/src/lib.rs"
  printf 'pub fn beta() {}\n' > "$root/crates/beta/src/lib.rs"

  cargo generate-lockfile --quiet --manifest-path "$root/Cargo.toml"
  sed -i.bak 's/version = "0.0.9"/version = "0.1.0"/' "$root/Cargo.toml"
  rm "$root/Cargo.toml.bak"

  for package_json in "${PACKAGE_JSON_PATHS[@]}"; do
    mkdir -p "$(dirname "$root/$package_json")"
    printf '{\n  "version": "0.1.0"\n}\n' > "$root/$package_json"
  done

  cat > "$root/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

### Fixed
- Fixture change

## [0.1.0] - 2026-01-01

[Unreleased]: https://github.com/TateB/solgrid/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/TateB/solgrid/releases/tag/v0.1.0
EOF

  echo "$root"
}

assert_set_version() {
  local version="$1"
  local root
  local package_json
  root=$(new_fixture)

  "$root/scripts/version.sh" --set "$version" > "$root/set.out"
  assert_contains "$root/Cargo.toml" "version = \"$version\""
  assert_contains "$root/Cargo.lock" "version = \"$version\""
  assert_contains "$root/CHANGELOG.md" "## [$version]"
  for package_json in "${PACKAGE_JSON_PATHS[@]}"; do
    assert_contains "$root/$package_json" "\"version\": \"$version\""
  done
  "$root/scripts/version.sh" > "$root/check.out"
  assert_contains "$root/check.out" "All versions are in sync."
}

# Cargo.lock starts one version behind Cargo.toml. Check mode must reject it,
# while --write must refresh both workspace package records through Cargo.
stale_root=$(new_fixture)
if "$stale_root/scripts/version.sh" > "$stale_root/stale.out" 2>&1; then
  fail "check mode accepted stale workspace versions in Cargo.lock"
fi
assert_contains "$stale_root/stale.out" \
  "Cargo.lock package fixture_alpha version (0.0.9) does not match Cargo.toml (0.1.0)"
assert_contains "$stale_root/stale.out" \
  "Cargo.lock package fixture_beta version (0.0.9) does not match Cargo.toml (0.1.0)"
"$stale_root/scripts/version.sh" --write > "$stale_root/write.out"
"$stale_root/scripts/version.sh" > "$stale_root/check.out"
assert_contains "$stale_root/check.out" "All versions are in sync."
updated_lock_entries=$(grep -Fc 'version = "0.1.0"' "$stale_root/Cargo.lock")
[ "$updated_lock_entries" -eq 2 ] ||
  fail "expected two refreshed workspace entries in Cargo.lock, found $updated_lock_entries"

# Check mode must propagate cargo metadata failures instead of continuing into
# the lockfile text check and accidentally returning success.
metadata_root=$(new_fixture)
"$metadata_root/scripts/version.sh" --write > "$metadata_root/write.out"
real_cargo=$(command -v cargo)
mkdir -p "$metadata_root/fake-bin"
cat > "$metadata_root/fake-bin/cargo" <<EOF
#!/usr/bin/env bash
if [ "\${1:-}" = "metadata" ]; then
  echo "forced cargo metadata failure" >&2
  exit 86
fi
exec "$real_cargo" "\$@"
EOF
chmod +x "$metadata_root/fake-bin/cargo"
if PATH="$metadata_root/fake-bin:$PATH" \
  "$metadata_root/scripts/version.sh" > "$metadata_root/metadata.out" 2>&1; then
  fail "check mode ignored a cargo metadata failure"
fi
assert_contains "$metadata_root/metadata.out" "forced cargo metadata failure"
assert_contains "$metadata_root/metadata.out" \
  "ERROR: cargo metadata could not validate the locked workspace"

# Stable, prerelease, and build-metadata SemVer forms are accepted end to end.
assert_set_version "0.0.0"
assert_set_version "1.2.3"
assert_set_version "1.2.3-alpha.1"
assert_set_version "1.2.3+build.5"
assert_set_version "1.2.3-rc.1+build.5"

# Trailing garbage, leading zeroes, empty identifiers, and incomplete versions
# must be rejected before any repository file is changed.
invalid_root=$(new_fixture)
invalid_versions=(
  "1.2.3garbage"
  "01.2.3"
  "1.02.3"
  "1.2.03"
  "1.2"
  "v1.2.3"
  "1.2.3-01"
  "1.2.3-alpha..1"
  "1.2.3+"
)
for version in "${invalid_versions[@]}"; do
  if "$invalid_root/scripts/version.sh" --set "$version" \
    > "$invalid_root/invalid.out" 2>&1; then
    fail "accepted invalid SemVer: $version"
  fi
done
assert_contains "$invalid_root/Cargo.toml" 'version = "0.1.0"'
assert_contains "$invalid_root/editors/vscode/package.json" '"version": "0.1.0"'

echo "version script tests passed"
