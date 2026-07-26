#!/usr/bin/env bash
# Science CLI/MCP release preflight (v1.0.1 prep) — does NOT publish.
# Uses packs/science/VERSION; does NOT require Core pager version match.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "OK: $*"; }

SCI_VER_FILE="packs/science/VERSION"
[[ -f "$SCI_VER_FILE" ]] || fail "missing $SCI_VER_FILE"
SCI_VER="$(tr -d '[:space:]' <"$SCI_VER_FILE")"
[[ -n "$SCI_VER" ]] || fail "empty science VERSION"
ok "science version=$SCI_VER"

# Dirty tree check (allow documented exceptions)
if [[ -n "$(git status --porcelain | grep -v '^??' || true)" ]]; then
  echo "WARN: working tree has modifications (release should use clean commit)"
else
  ok "working tree clean (tracked)"
fi

COMMIT="$(git rev-parse HEAD)"
ok "commit=$COMMIT"

# Machine gates
bash scripts/science-machine-gates.sh
ok "machine gates"

bash scripts/science-go-module-boundary.sh
ok "go module boundary"

# Go tests product packages
(
  cd packs/science
  go test ./mcp ./mcp/artifacts/... ./mcp/notebook/... ./mcp/reviewer/... \
    ./standalone/internal/seqbench/... ./standalone/internal/projectstore/... \
    -count=1 -timeout=90s
)
ok "go tests"

# Optional: Rust science if cargo available
if command -v cargo >/dev/null 2>&1; then
  (
    cd agent
    cargo test -p xai-grok-science --lib --locked -- --test-threads=4 2>&1 | tail -20
  ) || echo "WARN: cargo test xai-grok-science failed or skipped"
else
  echo "SKIP: cargo not available"
fi

# Manifest template for 1.0.1 (write under SCRATCH or dist)
OUT="packs/science/dist/science-release/PREFLIGHT-MANIFEST.json"
mkdir -p "$(dirname "$OUT")"
python3 - <<PY
import json, time
from pathlib import Path
manifest = {
  "schemaVersion": 1,
  "product": "lumen-science-cli-mcp",
  "version": "$SCI_VER",
  "proposedTag": "v$SCI_VER" if "$SCI_VER".count(".")>=2 else "v1.0.1",
  "git_commit": "$COMMIT",
  "generatedAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
  "notes": [
    "Preflight only — not a published release",
    "Do not re-use broken v1.0.0 tag provenance",
    "Release workflow must set builder run ID and per-asset sha256",
  ],
  "requiredNextSteps": [
    "tag only after CI green on this commit",
    "workflow-built assets only (no manual upload from dirty tree)",
    "upload smoke logs as release assets",
  ],
}
Path("$OUT").write_text(json.dumps(manifest, indent=2) + "\n")
print("wrote", "$OUT")
PY
ok "preflight manifest $OUT"

echo
echo "PREFLIGHT COMPLETE for Science $SCI_VER @ $COMMIT"
echo "Next: cut tag only after Desktop CI + Science CI green; run release workflow."
