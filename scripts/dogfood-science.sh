#!/usr/bin/env bash
# Build, diagnose, and run the three-step Science private-beta path.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PACK="$ROOT/packs/science"
TOPIC="${1:-aspirin}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="${SCIENCE_EVIDENCE_DIR:-$ROOT/SCRATCH/science-$STAMP}"
BIN="$PACK/lumen-science"
BRIEF="$OUT_DIR/brief.md"
LOG="$OUT_DIR/run.log"

mkdir -p "$OUT_DIR"
exec > >(tee "$LOG") 2>&1

echo "timestamp=$STAMP"
echo "source_commit=$(git -C "$ROOT" rev-parse HEAD)"
echo "topic=$TOPIC"
echo "evidence_dir=$OUT_DIR"

echo "STEP 1/3 build + tests"
go test -C "$PACK/standalone" ./...
go vet -C "$PACK/standalone" ./...
go build -C "$PACK/standalone" -o ../lumen-science ./cmd/science

echo "STEP 2/3 read-only doctor"
"$BIN" doctor --root "$PACK"

echo "STEP 3/3 live PubMed + ChEMBL brief"
"$BIN" brief --out "$BRIEF" "$TOPIC"
grep -Eq 'PMID [0-9]+' "$BRIEF" || {
  echo "FAIL: live brief contains no PubMed evidence" >&2
  exit 1
}
grep -q '## Provenance' "$BRIEF" || {
  echo "FAIL: live brief is missing provenance" >&2
  exit 1
}

echo "PASS: Science three-step dogfood completed"
echo "brief=$BRIEF"
echo "log=$LOG"
