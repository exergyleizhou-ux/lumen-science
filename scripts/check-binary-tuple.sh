#!/usr/bin/env bash
# Fail closed unless release + installed lumen are the same build of current HEAD.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="/opt/homebrew/bin:$HOME/.cargo/bin:$HOME/.local/bin:$PATH"

RELEASE_BIN="${LUMEN_RELEASE_BIN:-$ROOT/agent/target/release/lumen}"
INSTALLED_BIN="${LUMEN_INSTALLED_BIN:-$HOME/.local/bin/lumen}"
HEAD_SHORT="$(git -C "$ROOT" rev-parse --short=7 HEAD)"

for bin in "$RELEASE_BIN" "$INSTALLED_BIN"; do
  [[ -x "$bin" ]] || {
    echo "FAIL: lumen binary missing or not executable: $bin" >&2
    exit 1
  }
done

RELEASE_VERSION="$("$RELEASE_BIN" --version)"
INSTALLED_VERSION="$("$INSTALLED_BIN" --version)"
[[ "$RELEASE_VERSION" == "$INSTALLED_VERSION" ]] || {
  echo "FAIL: release/installed version mismatch" >&2
  echo "release=$RELEASE_VERSION" >&2
  echo "installed=$INSTALLED_VERSION" >&2
  exit 1
}
case "$RELEASE_VERSION" in
  *"($HEAD_SHORT)"*) ;;
  *)
    echo "FAIL: binary is not built from current HEAD $HEAD_SHORT: $RELEASE_VERSION" >&2
    exit 1
    ;;
esac

RELEASE_SHA="$(shasum -a 256 "$RELEASE_BIN" | awk '{print $1}')"
INSTALLED_SHA="$(shasum -a 256 "$INSTALLED_BIN" | awk '{print $1}')"
[[ "$RELEASE_SHA" == "$INSTALLED_SHA" ]] || {
  echo "FAIL: release/installed binary SHA-256 mismatch" >&2
  echo "release_sha256=$RELEASE_SHA" >&2
  echo "installed_sha256=$INSTALLED_SHA" >&2
  exit 1
}

if [[ -n "${LUMEN_EXPECTED_BINARY_SHA:-}" ]] && \
   [[ "$RELEASE_SHA" != "$LUMEN_EXPECTED_BINARY_SHA" ]]; then
  echo "FAIL: binary changed during readiness run" >&2
  echo "expected_sha256=$LUMEN_EXPECTED_BINARY_SHA" >&2
  echo "actual_sha256=$RELEASE_SHA" >&2
  exit 1
fi

echo "version=$RELEASE_VERSION"
echo "binary_sha256=$RELEASE_SHA"
echo "release_binary=$RELEASE_BIN"
echo "installed_binary=$INSTALLED_BIN"
