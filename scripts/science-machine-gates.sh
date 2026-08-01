#!/usr/bin/env bash
# Lumen Science machine gates — fail closed, no greenwash.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() { echo "GATE FAIL: $*" >&2; exit 1; }
ok() { echo "GATE OK: $*"; }

python3 scripts/verify-ecosystem-admission.py || fail "ecosystem admission"
ok "ecosystem admission"

# v2 is deliberately a draft intake lock, so the verifier itself returns 2.
# Its focused tests prove that draft status cannot be mistaken for a product
# admission and that every selected source path is present in its exact tree.
python3 scripts/test-upstream-intake-v2.py || fail "upstream v2 intake contract"
python3 scripts/test-upstream-intake-dashboard.py || fail "upstream v2 intake dashboard"
python3 scripts/test-upstream-git-tree-inventory.py || fail "metadata tree inventory"
python3 scripts/test-upstream-tree-inventory.py || fail "checkout tree inventory"
python3 scripts/test-upstream-component-coverage.py || fail "upstream component coverage"
python3 scripts/test-motif-provenance-lock.py || fail "Motif product provenance lock"
ok "v2 intake and Motif provenance contracts"

# The Lumen consumer pin remains a deliberate non-pass until its upstream R0
# and public extension contract have immutable evidence.  Its tests ensure a
# later activation cannot omit source, API, CI, binary, or rollback evidence.
python3 scripts/test-lumen-platform-pin.py || fail "canonical Lumen consumer pin contract"
ok "canonical Lumen consumer pin contract"

python3 scripts/test-capability-intake.py || fail "capability intake contracts"
ok "capability intake contracts"

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

python3 - <<'PY' || fail "independent Go release is frozen"
from pathlib import Path

workflow = Path(".github/workflows/science-release.yml").read_text()
code = "\n".join(
    line for line in workflow.splitlines()
    if not line.lstrip().startswith("#")
)
for forbidden in (
    "push:",
    "tags:",
    "contents: write",
    "independentFromCore",
    "actions/checkout",
    "actions/upload-artifact",
    "gh release",
    "make release",
    "go build",
):
    assert forbidden not in code, f"legacy release regained authority: {forbidden}"
assert "workflow_dispatch:" in code
assert "Legacy Science Go Release (frozen)" in workflow
print("legacy Go release: manual notice only; no tag/build/publish authority")
PY
ok "independent Go release freeze"

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

# Release checksum evidence for frozen Go Science CLI version (not Rust Core).
VER=$(tr -d '[:space:]' < packs/science/VERSION)
if [[ -f "outputs/release/${VER}/SHA256SUMS" ]]; then
  ok "release checksums present for Science CLI ${VER}"
else
  echo "WARN  outputs/release/${VER}/SHA256SUMS not present (run make release + copy sums)"
fi

# Go release/sign entry points must never fall back to root VERSION. Root is
# the Rust Core line and intentionally differs from packs/science/VERSION.
grep -Fq 'SCRIPT_DIR/VERSION' packs/science/release.sh \
  || fail "packs/science/release.sh does not use its component VERSION"
grep -Fq 'ROOT/packs/science/VERSION' scripts/sign-release.sh \
  || fail "scripts/sign-release.sh does not use packs/science/VERSION"
if grep -Eq '(\.\./\.\./VERSION|ROOT/VERSION)' \
  packs/science/release.sh scripts/sign-release.sh; then
  fail "Go release/sign entry point still reads root Rust Core VERSION"
fi
ok "Go release/sign version source boundary"

# Rust Core version truth: root VERSION == agent/VERSION == eight Core crates.
python3 scripts/release_version.py --root . check >/dev/null
ok "Rust Core VERSION contract (root + agent + 8 crates)"

# Offline Core-drift fixture gate (no upstream checkout required).
python3 scripts/check-core-drift.py --self-test
ok "Core drift fixture self-test"

# Audited-head Core drift comparison when that exact Lumen checkout is provided.
# Local: CORE_DRIFT_UPSTREAM_ROOT=/Users/lei/code/lumen
# CI: checkout the lock pin into a temp dir and set the same env var.
if [[ -n "${CORE_DRIFT_UPSTREAM_ROOT:-}" ]]; then
  python3 scripts/check-core-drift.py \
    --science-root . \
    --upstream-root "$CORE_DRIFT_UPSTREAM_ROOT" \
    --lock docs/science/5.0/core-v0.1.251-admission.lock.json
  ok "Core drift audited-head manifest comparison against $CORE_DRIFT_UPSTREAM_ROOT"
else
  echo "WARN  CORE_DRIFT_UPSTREAM_ROOT unset — audited-head Core drift comparison NOT RUN"
fi

echo
echo "All science machine gates passed (documentation + lock integrity)."
echo "Rust unit tests and release artifacts are separate gates."
