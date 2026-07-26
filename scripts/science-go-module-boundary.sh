#!/usr/bin/env bash
# Enforce Go module build boundary for packs/science.
#
# Product / CI scope (must `go test` cleanly):
#   ./mcp  ./mcp/artifacts/...  ./mcp/notebook/...  ./mcp/reviewer/...
#   ./mcp/http_bridge/...  ./standalone/...  ./e2e/...  ./renderers/...
#
# Legacy monorepo packs (import lumen/internal/*) are OUT of standalone scope:
#   lab launcher native proxy config gui research oauth guard runtime migrate
# They require the full Lumen Go tree and must not be claimed as product-green.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/packs/science"

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "OK: $*"; }

PRODUCT_PKGS=(
  ./mcp
  ./mcp/artifacts/...
  ./mcp/notebook/...
  ./mcp/reviewer/...
  ./mcp/http_bridge/...
  ./standalone/...
  ./e2e/...
  ./renderers/...
)

LEGACY_DIRS=(lab launcher native proxy config gui research oauth guard runtime migrate paths internal)

# 1) Product packages build and test
echo "=== product go test ==="
go test "${PRODUCT_PKGS[@]}" -count=1 -timeout=120s
ok "product packages pass"

# 2) Product packages must not import lumen/internal
echo "=== product import boundary ==="
if go list -f '{{.ImportPath}} {{.Imports}} {{.TestImports}}' "${PRODUCT_PKGS[@]}" 2>/dev/null \
  | grep -E 'lumen/internal'; then
  fail "product packages must not import lumen/internal/*"
fi
ok "product packages free of lumen/internal imports"

# 3) Document legacy packages still present (honest inventory)
echo "=== legacy inventory (not product-green) ==="
legacy_hits=0
for d in "${LEGACY_DIRS[@]}"; do
  if [[ -d "$d" ]]; then
    hits=$(grep -RIn --include='*.go' 'lumen/internal' "$d" 2>/dev/null | wc -l | tr -d ' ' || true)
    if [[ "${hits:-0}" != "0" ]]; then
      echo "  LEGACY $d: $hits lumen/internal references"
      legacy_hits=$((legacy_hits + hits))
    fi
  fi
done
ok "legacy inventory complete ($legacy_hits references outside product scope)"

# 4) Top-level dirs that import lumen/internal must be in the known legacy set
echo "=== unknown top-level scope scan ==="
unknown=0
KNOWN_LEGACY="lab launcher native proxy config gui research oauth guard runtime migrate paths internal c2d"
for d in */; do
  d=${d%/}
  case "$d" in
    mcp|standalone|e2e|renderers|skills|build|dist) continue ;;
  esac
  hits=$(grep -RIn --include='*.go' 'lumen/internal' "$d" 2>/dev/null | wc -l | tr -d ' ' || true)
  if [[ "${hits:-0}" != "0" ]]; then
    case " $KNOWN_LEGACY " in
      *" $d "*) ;;
      *)
        echo "UNKNOWN-SCOPE top-level dir imports lumen/internal: $d ($hits refs)"
        unknown=$((unknown + 1))
        ;;
    esac
  fi
done
[[ $unknown -eq 0 ]] || fail "$unknown unknown-scope top-level packages import lumen/internal"
ok "no unknown-scope top-level packages"

echo
echo "PASS: science Go module boundary (product green; legacy documented)"
