#!/usr/bin/env bash
# Install lumen-science productivity CLI to ~/.local/bin
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACK="$ROOT/packs/science"
VERSION="$(cat "$ROOT/VERSION" 2>/dev/null || echo dev)"
OUT_DIR="${LUMEN_SCIENCE_BIN_DIR:-$HOME/.local/bin}"
mkdir -p "$OUT_DIR" "$PACK/build"

echo "building lumen-science $VERSION …"
(
  cd "$PACK"
  go build -trimpath -ldflags="-s -w -X main.version=${VERSION}" \
    -o build/lumen-science ./standalone/cmd/science
)

install -m 755 "$PACK/build/lumen-science" "$OUT_DIR/lumen-science"
echo "installed: $OUT_DIR/lumen-science"
"$OUT_DIR/lumen-science" version
"$OUT_DIR/lumen-science" doctor --root "$PACK" || true

echo
echo "Release matrix (optional): cd packs/science && make release"
echo "Checksums: outputs/release/${VERSION}/SHA256SUMS"
echo
echo "Try:"
echo "  lumen-science seq analyze your.fa"
echo "  lumen-science pipeline offline --project local --run demo your.fa"
echo "  lumen-science brief aspirin    # live network — authorized only"
echo "  lumen-science gates"
