#!/usr/bin/env bash
# Detached GPG signature over release SHA256SUMS (integrity, not Apple notarization).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# These sums are produced by packs/science (the frozen Go CLI/MCP line), so the
# default signature/output version must come from that component, never the
# root Rust Core VERSION.
VERSION="${1:-$(tr -d '[:space:]' < "$ROOT/packs/science/VERSION")}"
SUMS_DIR="$ROOT/packs/science/dist/science-release"
OUT_DIR="$ROOT/outputs/release/${VERSION}"
mkdir -p "$OUT_DIR"

if [[ ! -f "$SUMS_DIR/SHA256SUMS" ]]; then
  echo "missing $SUMS_DIR/SHA256SUMS — run: cd packs/science && make release" >&2
  exit 1
fi

cp -f "$SUMS_DIR/SHA256SUMS" "$OUT_DIR/SHA256SUMS"

if ! command -v gpg >/dev/null; then
  echo "gpg not found; wrote SHA256SUMS only" >&2
  echo "signing=none" > "$OUT_DIR/SIGNING.txt"
  exit 0
fi

# Prefer explicit key; else default secret key
KEY="${LUMEN_GPG_KEY:-}"
ARGS=(--batch --yes --detach-sign --armor)
if [[ -n "$KEY" ]]; then
  ARGS+=(--local-user "$KEY")
fi

gpg "${ARGS[@]}" -o "$OUT_DIR/SHA256SUMS.asc" "$OUT_DIR/SHA256SUMS"
gpg --verify "$OUT_DIR/SHA256SUMS.asc" "$OUT_DIR/SHA256SUMS"
{
  echo "signing=gpg-detached"
  echo "file=SHA256SUMS.asc"
  echo "algorithm=OpenPGP"
  echo "note=Not Apple notarization / Windows Authenticode. Operators may verify with gpg --verify."
  gpg --list-secret-keys --keyid-format LONG 2>/dev/null | head -20 || true
} | tee "$OUT_DIR/SIGNING.txt"

echo "signed: $OUT_DIR/SHA256SUMS.asc"
