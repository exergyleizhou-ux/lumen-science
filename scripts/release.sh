#!/usr/bin/env bash
# Prepare and publish a Lumen release through the tag-triggered GitHub workflow.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION_TOOL="$ROOT/scripts/release_version.py"
CHANGELOG_TOOL="$ROOT/scripts/update-changelog.py"
RELEASE_BRANCH="${LUMEN_RELEASE_BRANCH:-main}"
REMOTE="${LUMEN_RELEASE_REMOTE:-origin}"
DRY_RUN=0
NO_PUSH=0
UNSIGNED_TAG=0

usage() {
  cat <<'EOF'
Usage: scripts/release.sh [--dry-run] [--no-push] [--unsigned-tag] BUMP

BUMP is patch, minor, major, prerelease, or an explicit SemVer (with optional v).

  --dry-run       Validate version state and print the next version only.
  --no-push       Prepare the release commit and tag without pushing them.
  --unsigned-tag  Create an annotated tag. Allowed only together with --no-push.

Formal releases create a signed tag and atomically push the release commit and
tag to origin. The tag triggers .github/workflows/release.yml, which builds,
checksums, signs, and publishes all four native artifacts.
EOF
}

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

BUMP=""
while (($#)); do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --no-push) NO_PUSH=1 ;;
    --unsigned-tag) UNSIGNED_TAG=1 ;;
    -h|--help) usage; exit 0 ;;
    --*) fail "unknown option: $1" ;;
    *)
      [[ -z "$BUMP" ]] || fail "only one bump may be specified"
      BUMP="$1"
      ;;
  esac
  shift
done
[[ -n "$BUMP" ]] || { usage >&2; exit 2; }
((UNSIGNED_TAG == 0 || NO_PUSH == 1)) || fail "--unsigned-tag is allowed only with --no-push"

command -v python3 >/dev/null || fail "python3 is required"
CURRENT="$(python3 "$VERSION_TOOL" --root "$ROOT" check)"
NEXT="$(python3 "$VERSION_TOOL" --root "$ROOT" next "$BUMP")"
TAG="v$NEXT"
echo "Release plan: $CURRENT -> $NEXT ($TAG)"
if ((DRY_RUN)); then
  exit 0
fi

for command_name in git cargo; do
  command -v "$command_name" >/dev/null || fail "$command_name is required"
done
[[ -z "$(git -C "$ROOT" status --porcelain --untracked-files=all)" ]] \
  || fail "release preparation requires a clean working tree"
BRANCH="$(git -C "$ROOT" symbolic-ref --quiet --short HEAD)" \
  || fail "release preparation requires a branch, not detached HEAD"
[[ "$BRANCH" == "$RELEASE_BRANCH" ]] \
  || fail "release must run on $RELEASE_BRANCH (current branch: $BRANCH)"
git -C "$ROOT" remote get-url "$REMOTE" >/dev/null 2>&1 \
  || fail "missing release remote: $REMOTE"
REMOTE_URL="$(git -C "$ROOT" remote get-url "$REMOTE")"
[[ "$REMOTE_URL" =~ github\.com[:/][^/]+/lumen(\.git)?$ ]] \
  || fail "$REMOTE does not point to a GitHub lumen repository: $REMOTE_URL"
git -C "$ROOT" fetch --prune "$REMOTE" "$RELEASE_BRANCH" --tags
REMOTE_HEAD="$(git -C "$ROOT" rev-parse "$REMOTE/$RELEASE_BRANCH")"
LOCAL_HEAD="$(git -C "$ROOT" rev-parse HEAD)"
[[ "$LOCAL_HEAD" == "$REMOTE_HEAD" ]] \
  || fail "HEAD must exactly match $REMOTE/$RELEASE_BRANCH before release"
if git -C "$ROOT" show-ref --verify --quiet "refs/tags/$TAG" \
  || git -C "$ROOT" ls-remote --exit-code --tags "$REMOTE" "refs/tags/$TAG" >/dev/null 2>&1; then
  fail "release tag already exists: $TAG"
fi

python3 "$VERSION_TOOL" --root "$ROOT" set "$NEXT" >/dev/null
python3 "$CHANGELOG_TOOL" --root "$ROOT" "$NEXT"
python3 "$VERSION_TOOL" --root "$ROOT" check >/dev/null
git -C "$ROOT" diff --check
(cd "$ROOT/agent" && cargo check --locked --package xai-grok-pager-bin --features release-dist)
"$ROOT/scripts/test-release-prep.sh"
RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
  CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
  "$ROOT/scripts/test-release-contract.sh"

VERSION_PATHS=(
  VERSION
  CHANGELOG.md
  agent/Cargo.lock
  agent/crates/codegen/xai-grok-version/Cargo.toml
  agent/crates/codegen/xai-grok-pager/Cargo.toml
  agent/crates/codegen/xai-grok-pager-bin/Cargo.toml
  agent/crates/codegen/xai-grok-shell/Cargo.toml
  agent/crates/codegen/xai-grok-tools/Cargo.toml
  agent/crates/codegen/xai-grok-tools-api/Cargo.toml
  agent/crates/codegen/xai-grok-update/Cargo.toml
  agent/crates/codegen/xai-grok-workspace/Cargo.toml
)
while IFS= read -r changed_path; do
  case "$changed_path" in
    VERSION|CHANGELOG.md|agent/Cargo.lock|\
    agent/crates/codegen/xai-grok-version/Cargo.toml|\
    agent/crates/codegen/xai-grok-pager/Cargo.toml|\
    agent/crates/codegen/xai-grok-pager-bin/Cargo.toml|\
    agent/crates/codegen/xai-grok-shell/Cargo.toml|\
    agent/crates/codegen/xai-grok-tools/Cargo.toml|\
    agent/crates/codegen/xai-grok-tools-api/Cargo.toml|\
    agent/crates/codegen/xai-grok-update/Cargo.toml|\
    agent/crates/codegen/xai-grok-workspace/Cargo.toml) ;;
    *) fail "release verification changed an unexpected path: $changed_path" ;;
  esac
done < <(
  {
    git -C "$ROOT" diff --name-only
    git -C "$ROOT" ls-files --others --exclude-standard
  } | sort -u
)
git -C "$ROOT" add -- "${VERSION_PATHS[@]}"
git -C "$ROOT" diff --cached --quiet && fail "version bump produced no staged changes"
git -C "$ROOT" commit -m "chore(release): prepare $TAG"

if ((UNSIGNED_TAG)); then
  git -C "$ROOT" tag -a "$TAG" -m "Lumen $TAG"
else
  git -C "$ROOT" tag -s "$TAG" -m "Lumen $TAG"
  git -C "$ROOT" tag -v "$TAG"
fi

if ((NO_PUSH)); then
  echo "OK: prepared $TAG locally; no remote changes were made"
  exit 0
fi
git -C "$ROOT" push --atomic "$REMOTE" "HEAD:$RELEASE_BRANCH" "refs/tags/$TAG"
echo "OK: pushed $TAG; GitHub Actions will build and publish the four-platform release"
