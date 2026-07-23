#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/lumen-release-prep-XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
FIXTURE="$TMP/repo"
mkdir -p "$FIXTURE/scripts" "$FIXTURE/agent/crates/codegen"
cp "$ROOT/scripts/release_version.py" "$ROOT/scripts/update-changelog.py" "$FIXTURE/scripts/"
printf '1.2.3-alpha.4\n' >"$FIXTURE/VERSION"

packages=(
  xai-grok-version xai-grok-pager xai-grok-pager-bin xai-grok-shell xai-grok-tools
  xai-grok-tools-api xai-grok-update xai-grok-workspace
)
for package in "${packages[@]}"; do
  mkdir -p "$FIXTURE/agent/crates/codegen/$package"
  printf '[package]\nname = "%s"\nversion = "1.2.3-alpha.4"\n' "$package" \
    >"$FIXTURE/agent/crates/codegen/$package/Cargo.toml"
done
{
  for package in "${packages[@]}"; do
    printf '[[package]]\nname = "%s"\nversion = "1.2.3-alpha.4"\n\n' "$package"
  done
} >"$FIXTURE/agent/Cargo.lock"
cat >"$FIXTURE/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

### Added

- Hand-written pending note.
EOF

git -C "$FIXTURE" init -q -b main
git -C "$FIXTURE" config user.email fixture@example.invalid
git -C "$FIXTURE" config user.name Fixture
git -C "$FIXTURE" add .
git -C "$FIXTURE" commit -qm 'feat: initial release fixture'
git -C "$FIXTURE" commit --allow-empty -qm 'fix(release): keep versions synchronized'

[[ "$(python3 "$FIXTURE/scripts/release_version.py" --root "$FIXTURE" check)" == 1.2.3-alpha.4 ]]
[[ "$(python3 "$FIXTURE/scripts/release_version.py" --root "$FIXTURE" next patch)" == 1.2.3 ]]
[[ "$(python3 "$FIXTURE/scripts/release_version.py" --root "$FIXTURE" next prerelease)" == 1.2.3-alpha.5 ]]
python3 "$FIXTURE/scripts/release_version.py" --root "$FIXTURE" set 1.2.3 >/dev/null
[[ "$(python3 "$FIXTURE/scripts/release_version.py" --root "$FIXTURE" check)" == 1.2.3 ]]
[[ "$(grep -R 'version = "1.2.3"' "$FIXTURE/agent/crates/codegen" -l | wc -l | tr -d ' ')" == 8 ]]
[[ "$(grep -c 'version = "1.2.3"' "$FIXTURE/agent/Cargo.lock")" == 8 ]]

python3 "$FIXTURE/scripts/update-changelog.py" --root "$FIXTURE" --date 2026-07-18 1.2.3
grep -Fq '## [1.2.3] - 2026-07-18' "$FIXTURE/CHANGELOG.md"
grep -Fq -- '- Hand-written pending note.' "$FIXTURE/CHANGELOG.md"
grep -Fq -- '- keep versions synchronized (`' "$FIXTURE/CHANGELOG.md"
[[ "$(grep -c '^### Added$' "$FIXTURE/CHANGELOG.md")" == 1 ]]
if python3 "$FIXTURE/scripts/update-changelog.py" --root "$FIXTURE" --date 2026-07-18 1.2.3 \
  >"$TMP/duplicate.out" 2>&1; then
  echo "FAIL: duplicate changelog version was accepted" >&2
  exit 1
fi
grep -Fq 'already contains version 1.2.3' "$TMP/duplicate.out"

echo "OK: release version and changelog fixtures passed"
