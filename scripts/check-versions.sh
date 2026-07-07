#!/usr/bin/env bash
# The version string lives in five files (flake.nix carries it three
# times: the package version and the two Info.plist keys). This checks
# they all agree, and, when given a version argument, that they match it
# too. CI passes the release tag; run it with no argument locally to just
# confirm the files are in sync before tagging.
set -euo pipefail
cd "$(dirname "$0")/.."

expected="${1:-}"
expected="${expected#v}" # tolerate a leading v from a git tag

semver='[0-9]+\.[0-9]+\.[0-9]+'
fail=0

# Pull the first semver matching a pattern out of a file.
extract() {
  local pattern="$1" file="$2"
  grep -oE "$pattern" "$file" | grep -oE "$semver" | head -1
}

declare -A found
found[tauri.conf.json]=$(extract "\"version\"[[:space:]]*:[[:space:]]*\"$semver\"" src-tauri/tauri.conf.json)
found[package.json]=$(extract "\"version\"[[:space:]]*:[[:space:]]*\"$semver\"" package.json)
found[src-tauri/Cargo.toml]=$(extract "^version[[:space:]]*=[[:space:]]*\"$semver\"" src-tauri/Cargo.toml)
found[daemon/Cargo.toml]=$(extract "^version[[:space:]]*=[[:space:]]*\"$semver\"" daemon/Cargo.toml)
found[flake.nix:package]=$(extract "version = \"$semver\"" flake.nix)

# The two Info.plist keys in flake.nix must match as well.
mapfile -t plist < <(grep -oE "CFBundle(Version|ShortVersionString)</key><string>$semver</string>" flake.nix | grep -oE "$semver")
found[flake.nix:CFBundleVersion]="${plist[0]:-MISSING}"
found[flake.nix:CFBundleShortVersionString]="${plist[1]:-MISSING}"

# The reference is the tag if given, otherwise the first file's version.
reference="${expected:-${found[tauri.conf.json]}}"

echo "Expected version: $reference"
for label in "${!found[@]}"; do
  value="${found[$label]}"
  if [ "$value" != "$reference" ]; then
    echo "  MISMATCH  $label: ${value:-EMPTY}"
    fail=1
  else
    echo "  ok        $label: $value"
  fi
done

if [ "$fail" -ne 0 ]; then
  echo "Version strings are out of sync. Update every listed file to $reference." >&2
  exit 1
fi
echo "All version strings agree on $reference."
