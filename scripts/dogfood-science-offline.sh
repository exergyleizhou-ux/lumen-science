#!/usr/bin/env bash
# Offline productivity dogfood — no live network required.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACK="$ROOT/packs/science"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT="${SCIENCE_EVIDENCE_DIR:-$ROOT/SCRATCH/science-offline-$STAMP}"
BIN="$PACK/build/lumen-science"
mkdir -p "$OUT" "$PACK/build"

echo "=== lumen-science offline dogfood ==="
echo "commit=$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || echo unknown)"
echo "out=$OUT"

echo "STEP 1/5 build + unit tests"
(
  cd "$PACK"
  go test ./standalone/internal/seqbench/... ./standalone/internal/pipeline/... ./mcp/artifacts/... -count=1
  # Go Science CLI is the frozen 1.x product line — not root Rust VERSION.
  go build -trimpath -ldflags="-s -w -X main.version=$(cat "$ROOT/packs/science/VERSION")" \
    -o build/lumen-science ./standalone/cmd/science
)

echo "STEP 2/5 doctor"
"$BIN" doctor --root "$PACK"

echo "STEP 3/5 machine gates"
"$BIN" gates --root "$ROOT"

echo "STEP 4/5 seq analyze + offline pipeline"
FA="$OUT/input.fa"
cat >"$FA" <<'FA'
>productivity_demo EcoRI_and_NotI
GAATTCATGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAAAAAGCGGCCGCTAA
FA
"$BIN" seq analyze --md "$FA" | tee "$OUT/seq-report.md" >/dev/null
STORE="$OUT/artifacts"
"$BIN" pipeline offline --project dogfood --run "$STAMP" --store "$STORE" "$FA" \
  | tee "$OUT/pipeline.json" >/dev/null
python3 - <<PY
import json
from pathlib import Path
p=Path("$OUT/pipeline.json")
d=json.loads(p.read_text())
assert d["review"]["status"]=="pass", d["review"]
assert d["source_artifact"]["sha256"]
assert d["analysis_artifact"]["sha256"]
assert d["report_artifact"]["sha256"]
print("pipeline review PASS")
print("source_sha256=", d["source_artifact"]["sha256"])
PY

echo "STEP 5/5 MCP contract tests"
(
  cd "$PACK"
  go test ./mcp/artifacts/... ./mcp/notebook/... ./mcp/reviewer/... ./mcp/http_bridge/... -count=1
)

echo "PASS offline dogfood"
echo "evidence=$OUT"
