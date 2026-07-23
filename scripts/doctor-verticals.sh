#!/usr/bin/env bash
# doctor-verticals.sh — verify vertical packs exist and have entry points.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OK=0
ERR=0

echo "=== Lumen vertical packs doctor ==="
echo ""

count_go() {
  local dir="$1"
  find "$dir" -type f -name '*.go' 2>/dev/null | wc -l | tr -d ' '
}

check_pack() {
  local name="$1"
  local dir="$ROOT/packs/$name"
  if [[ -d "$dir" ]]; then
    echo "  ✅ packs/$name/ exists"
    if [[ -f "$dir/README.md" ]]; then
      echo "     README.md present"
    else
      echo "     ⚠️  README.md missing"
      ERR=$((ERR + 1))
      return
    fi
    local ngo
    ngo=$(count_go "$dir")
    if [[ "$ngo" -gt 0 ]]; then
      echo "     $ngo Go source files (recursive)"
      OK=$((OK + 1))
    else
      echo "     ⚠️  No Go source files under packs/$name"
      # README-only stub still counts as present for doctor exit, but warn.
      OK=$((OK + 1))
    fi
  else
    echo "  ❌ packs/$name/ missing"
    ERR=$((ERR + 1))
  fi
}

check_pack science
check_pack oasis
check_pack quant

echo ""
echo "=== Result ==="
echo "OK packs: $OK"
echo "Errors: $ERR"

if [[ "$ERR" -gt 0 ]]; then
  echo "FAIL: $ERR pack(s) missing or incomplete."
  exit 1
fi

# science must have real code (not empty stub)
sci=$(count_go "$ROOT/packs/science")
if [[ "$sci" -lt 10 ]]; then
  echo "FAIL: packs/science expected substantial Go sources, found $sci" >&2
  exit 1
fi

echo "OK: all vertical packs present (science go files=$sci)"

# The default private-beta vertical must have a real, independently buildable
# path. This is intentionally scoped to standalone/ because the staged legacy
# proxy/Lab sources still depend on shared packages in the old Go repository.
SCI_STANDALONE="$ROOT/packs/science/standalone"
if [[ ! -f "$SCI_STANDALONE/go.mod" ]]; then
  echo "FAIL: packs/science/standalone/go.mod missing" >&2
  exit 1
fi
if ! command -v go >/dev/null 2>&1; then
  echo "FAIL: Go is required to verify the Science private-beta path" >&2
  exit 1
fi
SCI_TMP="$(mktemp -d)"
trap 'rm -rf "$SCI_TMP"' EXIT
echo "Checking Science standalone module..."
go test -C "$SCI_STANDALONE" ./...
go build -C "$SCI_STANDALONE" -o "$SCI_TMP/lumen-science" ./cmd/science
"$SCI_TMP/lumen-science" doctor --root "$ROOT/packs/science"
echo "OK: Science standalone build + doctor pass"
exit 0
