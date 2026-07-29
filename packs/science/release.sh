#!/usr/bin/env bash
# Lumen Science release script
# Usage: ./release.sh [version]
# Generates cross-platform binaries, checksums, and release notes.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# This script releases the frozen Go CLI/MCP product line. Its version is
# packs/science/VERSION even when invoked from another working directory; root
# VERSION belongs exclusively to Rust Core.
VERSION="${1:-$(tr -d '[:space:]' < "$SCRIPT_DIR/VERSION")}"
RELEASE_DIR="dist/science-release-${VERSION}"
BINARIES=(artifacts notebook reviewer http_bridge)

echo "=== Lumen Science Release ${VERSION} ==="

# Clean and prepare
rm -rf "${RELEASE_DIR}"
mkdir -p "${RELEASE_DIR}"

# Build for current platform
echo ""
echo "--- Building for current platform ---"
for bin in "${BINARIES[@]}"; do
    echo "  lumen-mcp-${bin}..."
    go build -trimpath -ldflags="-s -w -X main.version=${VERSION}" \
        -o "${RELEASE_DIR}/lumen-mcp-${bin}" ./standalone/cmd/${bin}
done

# Cross-compile
echo ""
echo "--- Cross-compiling ---"
TARGETS=(
    "darwin arm64"
    "darwin amd64"
    "linux amd64"
    "linux arm64"
    "windows amd64"
)

for target in "${TARGETS[@]}"; do
    read -r goos goarch <<< "$target"
    suffix=""
    [ "$goos" = "windows" ] && suffix=".exe"
    for bin in "${BINARIES[@]}"; do
        echo "  lumen-mcp-${bin}-${goos}-${goarch}${suffix}..."
        GOOS="${goos}" GOARCH="${goarch}" \
            go build -trimpath -ldflags="-s -w -X main.version=${VERSION}" \
            -o "${RELEASE_DIR}/lumen-mcp-${bin}-${goos}-${goarch}${suffix}" \
            ./standalone/cmd/${bin}
    done
done

# Generate checksums
echo ""
echo "--- Checksums ---"
cd "${RELEASE_DIR}"
shasum -a 256 * > "lumen-science-${VERSION}-checksums.txt"
cat "lumen-science-${VERSION}-checksums.txt"

# Create archive
echo ""
echo "--- Packaging ---"
tar -czf "lumen-science-${VERSION}-macos-arm64.tar.gz" lumen-mcp-*-darwin-arm64
tar -czf "lumen-science-${VERSION}-linux-amd64.tar.gz" lumen-mcp-*-linux-amd64
zip -q "lumen-science-${VERSION}-windows-amd64.zip" lumen-mcp-*-windows-amd64.exe

echo ""
echo "=== Release complete: ${RELEASE_DIR} ==="
ls -lh "${RELEASE_DIR}"
