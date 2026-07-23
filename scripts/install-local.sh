#!/usr/bin/env bash
# Build the current source commit and atomically install `lumen` to ~/.local/bin.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"
export PROTOC="${PROTOC:-/opt/homebrew/bin/protoc}"

# A binary stamped with HEAD cannot truthfully identify uncommitted source.
# Ignored scratch output is harmless, but tracked or untracked source can alter
# Cargo auto-discovery/build-script inputs and is rejected unless explicitly
# overridden.
if [[ "${LUMEN_ALLOW_DIRTY:-0}" != "1" ]]; then
  if [[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=all)" ]]; then
    echo "FAIL: refusing to build/install from a dirty tree." >&2
    echo "Commit source first, or set LUMEN_ALLOW_DIRTY=1 explicitly." >&2
    git -C "$ROOT" status --short >&2
    exit 1
  fi
fi

BIN_SRC="$ROOT/agent/target/release/lumen"
DEST_DIR="${LUMEN_INSTALL_DIR:-$HOME/.local/bin}"
DEST="$DEST_DIR/lumen"
SOURCE_COMMIT="$(git -C "$ROOT" rev-parse --short HEAD)"

if [[ "${LUMEN_SKIP_BUILD:-0}" != "1" ]]; then
  echo "Building release lumen from source commit $SOURCE_COMMIT..."
  (cd "$ROOT/agent" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo build -p xai-grok-pager-bin --release)
else
  echo "LUMEN_SKIP_BUILD=1: verifying existing release binary against $SOURCE_COMMIT..."
fi
test -x "$BIN_SRC"

VERSION_LINE="$($BIN_SRC --version)"
case "$VERSION_LINE" in
  *"($SOURCE_COMMIT)"*) ;;
  *)
    echo "FAIL: release binary is stale: expected commit $SOURCE_COMMIT, got: $VERSION_LINE" >&2
    echo "Unset LUMEN_SKIP_BUILD and rebuild." >&2
    exit 1
    ;;
esac

mkdir -p "$DEST_DIR"
# ad-hoc code-sign BIN_SRC so macOS taskgated won't kill it
codesign --force --sign - "$BIN_SRC" 2>/dev/null || true
TMP_DEST="$DEST.tmp.$$"
trap 'rm -f "$TMP_DEST"' EXIT
cp "$BIN_SRC" "$TMP_DEST"
chmod +x "$TMP_DEST"
mv -f "$TMP_DEST" "$DEST"
trap - EXIT

SRC_SHA="$(shasum -a 256 "$BIN_SRC" | awk '{print $1}')"
DEST_SHA="$(shasum -a 256 "$DEST" | awk '{print $1}')"
if [[ "$SRC_SHA" != "$DEST_SHA" ]]; then
  echo "FAIL: installed binary checksum mismatch" >&2
  exit 1
fi

echo "Installed: $DEST"
echo "source_commit=$SOURCE_COMMIT"
echo "binary_sha256=$DEST_SHA"
"$DEST" --version
echo ""
echo "Ensure PATH includes: $DEST_DIR"
echo "Set:  export DEEPSEEK_API_KEY=..."
echo "Then: lumen"
echo ""
echo "Productivity diary template: journal/TEMPLATE-productivity-day.md"
