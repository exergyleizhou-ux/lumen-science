#!/usr/bin/env bash
# Lumen Science machine gates — fail closed, no greenwash.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }
ok() { echo "GATE OK: $*"; }

python3 - <<'PY' || fail "fusion-sources.lock.json"
import json
from pathlib import Path
p = Path("docs/science/fusion-sources.lock.json")
d = json.loads(p.read_text())
items = d["items"]
assert len(items) == 42, len(items)
unresolved = [i["connector_id"] for i in items if i.get("final_disposition") is None]
assert not unresolved, unresolved
assert d.get("renderer_sources"), "motif renderer_sources missing"
motif = next(x for x in d["renderer_sources"] if x["id"] == "jvogan-motif")
assert motif["commit"] == "876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0"
assert motif["runtime_authority"] == "none"
print("lock: 42/0 + motif locked")
PY
ok "fusion-sources.lock.json"

python3 - <<'PY' || fail "skills registry honesty"
import json
from pathlib import Path
d = json.loads(Path("packs/science/skills/registry.json").read_text())
assert d["schema_version"] >= 2
assert d["summary"]["total"] == len(d["skills"])
for s in d["skills"]:
    for k in ("source_repository", "exact_commit", "source_path", "source_sha256",
              "file_license", "prompt_injection_audit", "runtime_permissions",
              "final_disposition"):
        assert k in s and s[k] not in (None, ""), (s["skill_id"], k)
    perms = s["runtime_permissions"]
    assert perms.get("independent_execution_authority") is False, s["skill_id"]
    if s.get("final_disposition") == "approved":
        audit = s["prompt_injection_audit"]
        assert audit.get("status") == "pass", s["skill_id"]
        assert perms.get("may_call_lumen_tools_only") is True, s["skill_id"]
        tools = perms.get("controlled_tools") or []
        assert len(tools) >= 1, f"{s['skill_id']} approved without controlled_tools"
        assert perms.get("shell") in ("denied", "none", None) or perms.get("shell") == "denied"
print(f"skills: approved={d['summary']['approved']} pending={d['summary'].get('pending')} full DS-43 fields")
PY
ok "skills registry"

test -f docs/science/MOTIF_SUPPLY_CHAIN_AUDIT.md || fail "missing Motif audit"
test -f third_party/provenance/motif.md || fail "missing motif provenance"
test -f third_party/motif/NOTICE || fail "missing motif NOTICE"
test -f packs/science/renderers/static/motif.html || fail "missing motif.html"
grep -q "Content-Security-Policy" packs/science/renderers/static/motif.html || fail "motif CSP missing"
grep -q "runtime_authority" packs/science/renderers/static/motif.html || fail "motif authority notice missing"
ok "Motif contract surface"

test -f docs/science/PRODUCT_PATH_CONTRACT.md || fail "missing product path contract"
test -f docs/science/LUMEN_SCIENCE_1_0_STATUS.md -o -f docs/science/LUMEN_SCIENCE_1_0_RC_STATUS.md \
  || fail "missing 1.0 status doc"
ok "honesty docs"

# Release checksum evidence for current VERSION
VER=$(cat VERSION | tr -d '[:space:]')
if [[ -f "outputs/release/${VER}/SHA256SUMS" ]]; then
  ok "release checksums present for ${VER}"
else
  echo "WARN  outputs/release/${VER}/SHA256SUMS not present (run make release + copy sums)"
fi

echo
echo "All science machine gates passed (documentation + lock integrity)."
echo "Rust unit tests and release artifacts are separate gates."
