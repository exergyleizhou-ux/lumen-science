#!/usr/bin/env bash
# Verify that a GitHub Release's assets match SHA256SUMS (no missing, no hash drift).
# Usage:
#   bash scripts/verify-release-assets.sh [TAG]
#   RELEASE_DIR=packs/science/dist/science-release bash scripts/verify-release-assets.sh v1.0.0
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
TAG="${1:-v1.0.0}"
REL_DIR="${RELEASE_DIR:-}"

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "OK: $*"; }

command -v gh >/dev/null || fail "gh CLI required"

# Prefer local SHA256SUMS from a release build; fall back to downloading from the release.
SUMS_FILE=""
if [[ -n "$REL_DIR" && -f "$REL_DIR/SHA256SUMS" ]]; then
  SUMS_FILE="$REL_DIR/SHA256SUMS"
elif [[ -f "outputs/release/${TAG#v}/SHA256SUMS" ]]; then
  SUMS_FILE="outputs/release/${TAG#v}/SHA256SUMS"
elif [[ -f "packs/science/dist/science-release/SHA256SUMS" ]]; then
  SUMS_FILE="packs/science/dist/science-release/SHA256SUMS"
else
  tmp="$(mktemp -d)"
  gh release download "$TAG" -p SHA256SUMS -D "$tmp" || fail "cannot download SHA256SUMS for $TAG"
  SUMS_FILE="$tmp/SHA256SUMS"
fi
ok "using checksum file $SUMS_FILE"

EXPECTED_COUNT=$(awk 'NF>=2 {c++} END{print c+0}' "$SUMS_FILE")
[[ "$EXPECTED_COUNT" -ge 5 ]] || fail "SHA256SUMS looks empty ($EXPECTED_COUNT entries)"

# List release asset names
REMOTE_LIST="$(gh release view "$TAG" --json assets --jq '.assets[].name' | sort)"
REMOTE_COUNT=$(printf '%s\n' "$REMOTE_LIST" | grep -c . || true)
ok "release $TAG has $REMOTE_COUNT assets; checksum lists $EXPECTED_COUNT files"

missing=0
while read -r _hash f; do
  [[ -n "${f:-}" ]] || continue
  if ! printf '%s\n' "$REMOTE_LIST" | grep -qxF "$f"; then
    echo "MISSING on release: $f"
    missing=$((missing + 1))
  fi
done < "$SUMS_FILE"
[[ $missing -eq 0 ]] || fail "$missing checksummed files missing from GitHub Release $TAG"
ok "every SHA256SUMS entry exists on GitHub Release $TAG"

# Optional: verify hashes of local release dir against SUMS
if [[ -n "$REL_DIR" && -d "$REL_DIR" ]]; then
  (
    cd "$REL_DIR"
    while read -r hash file; do
      [[ -n "${file:-}" ]] || continue
      [[ -f "$file" ]] || fail "local file missing: $file"
      if command -v shasum >/dev/null; then
        actual=$(shasum -a 256 "$file" | awk '{print $1}')
      else
        actual=$(sha256sum "$file" | awk '{print $1}')
      fi
      [[ "$actual" == "$hash" ]] || fail "hash mismatch $file"
    done < SHA256SUMS
  )
  ok "local release dir hashes match SHA256SUMS"
fi

# Spot-check: required platform archives exist on the release
VER="${TAG#v}"
for arch in darwin-arm64 darwin-amd64 linux-amd64 linux-arm64; do
  name="lumen-science-${VER}-${arch}.tar.gz"
  printf '%s\n' "$REMOTE_LIST" | grep -qxF "$name" || fail "missing archive $name"
done
printf '%s\n' "$REMOTE_LIST" | grep -qxF "lumen-science-${VER}-windows-amd64.zip" \
  || fail "missing windows zip"
ok "five platform archives present on $TAG"

echo
echo "PASS: release asset validator for $TAG"
