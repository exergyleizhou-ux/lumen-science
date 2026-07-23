#!/usr/bin/env bash
# Build Lumen for Windows and create a team zip on Desktop.
# Requires: Windows machine with Rust (or macOS Xcode + cross-compile toolchain)
#
# This script detects whether we are on Windows or cross-compiling from macOS.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST_DIR="${LUMEN_INSTALL_DIR:-$HOME/Desktop}"
BUMP="${1:-patch}"  # patch, minor, or version string

detect_target() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) echo "x86_64-pc-windows-msvc" ;;
    Darwin)
      # macOS → windows cross-compile requires:
      #   rustup target add x86_64-pc-windows-msvc
      #   (opt) brew install mingw-w64
      if rustup target list --installed 2>/dev/null | grep -q windows-msvc; then
        echo "x86_64-pc-windows-msvc"
      else
        echo ""
      fi
      ;;
    Linux)
      if rustup target list --installed 2>/dev/null | grep -q windows-msvc; then
        echo "x86_64-pc-windows-msvc"
      else
        echo ""
      fi
      ;;
    *) echo "" ;;
  esac
}

TARGET="$(detect_target)"
if [[ -z "$TARGET" ]]; then
  cat << 'EOF'

=========================================================================
  Windows cross-compile toolchain not found.

  From macOS/Linux:
    rustup target add x86_64-pc-windows-msvc

  From Windows (native):
    Install Rust via https://rustup.rs, then:
    rustup target add x86_64-pc-windows-msvc

  Then re-run this script.
=========================================================================
EOF
  exit 1
fi

echo "Target: $TARGET"

# Preflight: clean tree
if [[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=all 2>/dev/null)" ]]; then
  echo "WARNING: dirty tree. Build may not match HEAD."
fi

SOURCE_COMMIT="$(git -C "$ROOT" rev-parse --short HEAD 2>/dev/null || echo "unknown")"
echo "Building Lumen for Windows from commit $SOURCE_COMMIT..."

# Build
(cd "$ROOT/agent" && \
  CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  cargo build --release --target "$TARGET" -p xai-grok-pager-bin)

BIN_SRC="$ROOT/agent/target/$TARGET/release/lumen"
if [[ "$TARGET" == *windows* ]]; then
  BIN_SRC="$ROOT/agent/target/$TARGET/release/lumen.exe"
fi

test -x "$BIN_SRC" || test -f "$BIN_SRC" || {
  echo "ERROR: binary not found at $BIN_SRC"
  exit 1
}

# Create zip
ZIP_NAME="lumen-${SOURCE_COMMIT}-${TARGET}.zip"
ZIP_PATH="$DEST_DIR/$ZIP_NAME"
OUT_DIR="/tmp/lumen-win-pkg-$$"
mkdir -p "$OUT_DIR"

cp "$BIN_SRC" "$OUT_DIR/lumen.exe"
SHA=$(shasum -a 256 "$OUT_DIR/lumen.exe" | awk '{print $1}')
echo "$SHA  lumen.exe" > "$OUT_DIR/SHA256SUMS.txt"

cat > "$OUT_DIR/INSTALL.txt" << INSTALL_EOF
Lumen for Windows
=================
Version: commit $SOURCE_COMMIT
Target: $TARGET
SHA256: $SHA

Requirements:
  - Windows 10/11 64-bit
  - Terminal with UTF-8 support (Windows Terminal recommended)
  - Tesseract OCR (optional, for image tools)

Install:
  1. Extract lumen.exe to a folder, e.g. C:\lumen\
  2. Add that folder to your PATH:
       - Open "System Properties" → "Environment Variables"
       - Edit "Path" → "New" → "C:\lumen\"
       - OK
  3. Open a new Command Prompt or PowerShell
  4. Verify:
       lumen --version
  5. Set your API key and run:
       set DEEPSEEK_API_KEY=your-key-here
       lumen

Notes:
  - Windows Smartscreen may block the first run. Click "More info" → "Run anyway"
  - Or use PowerShell to unblock:
       Unblock-File C:\lumen\lumen.exe

Repo: https://github.com/exergyleizhou-ux/lumen
EOF

# Create zip (use 7z if available for maximum compatibility)
if command -v 7z >/dev/null 2>&1; then
  (cd "$OUT_DIR" && 7z a -tzip "$ZIP_PATH" lumen.exe INSTALL.txt SHA256SUMS.txt -mx9)
elif command -v zip >/dev/null 2>&1; then
  (cd "$OUT_DIR" && zip -9 "$ZIP_PATH" lumen.exe INSTALL.txt SHA256SUMS.txt)
else
  echo "ERROR: neither 7z nor zip found; install one"
  exit 1
fi

ls -lh "$ZIP_PATH"
echo ""
echo "Windows package: $ZIP_PATH"
echo "SHA256: $SHA"
echo ""
echo "Installed? $("$BIN_SRC" --version 2>/dev/null || echo "n/a")"
echo ""
echo "Done."
