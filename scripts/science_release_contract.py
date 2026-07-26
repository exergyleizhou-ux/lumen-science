#!/usr/bin/env python3
"""Science CLI/MCP release contract — independent of Lumen Core pager version.

Does NOT require root VERSION to equal xai-grok-pager Cargo.toml.
Primary version source: packs/science/VERSION
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path


SCIENCE_VERSION = Path("packs/science/VERSION")
SEMVER = re.compile(
    r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?"
)


def fail(msg: str) -> None:
    print(f"FAIL: {msg}", file=sys.stderr)
    sys.exit(1)


def ok(msg: str) -> None:
    print(f"OK: {msg}")


def science_version(root: Path) -> str:
    p = root / SCIENCE_VERSION
    if not p.is_file():
        fail(f"missing {SCIENCE_VERSION}")
    raw = p.read_text(encoding="utf-8")
    if not raw.endswith("\n") or raw.count("\n") != 1:
        fail("packs/science/VERSION must be one newline-terminated SemVer line")
    version = raw.rstrip("\n")
    if not SEMVER.fullmatch(version):
        fail(f"not SemVer: {version}")
    return version


def git_commit(root: Path) -> str:
    r = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    if r.returncode != 0:
        fail("git rev-parse failed")
    return r.stdout.strip()


def cmd_preflight(root: Path, proposed_tag: str | None) -> None:
    version = science_version(root)
    commit = git_commit(root)
    tag = proposed_tag or f"v{version}"
    if not tag.startswith("v"):
        fail(f"tag must start with v: {tag}")
    if tag != f"v{version}":
        # allow v1.0.1 while VERSION still 1.0.0 during prep if explicitly proposed
        ok(f"note: proposed tag {tag} differs from VERSION {version} (prep mode)")
    # Ensure core VERSION is NOT required to match science
    cargo = root / "agent/crates/codegen/xai-grok-pager/Cargo.toml"
    if cargo.is_file():
        ok("Core pager Cargo.toml present — not required to equal Science VERSION")
    manifest = {
        "schemaVersion": 1,
        "product": "lumen-science-cli-mcp",
        "version": version,
        "proposedTag": tag,
        "git_commit": commit,
        "versionSource": str(SCIENCE_VERSION),
        "independentFromCore": True,
        "requiredFieldsOnPublish": [
            "git_commit",
            "tag",
            "toolchain",
            "source_date_epoch",
            "asset_sha256",
            "builder_workflow_run_id",
        ],
    }
    out = root / "packs/science/dist/science-release/SCIENCE-CONTRACT.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    ok(f"science version={version}")
    ok(f"commit={commit}")
    ok(f"wrote {out}")
    print(json.dumps(manifest, indent=2))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", type=Path, default=Path("."))
    ap.add_argument("command", choices=["preflight"])
    ap.add_argument("--tag", default=None)
    args = ap.parse_args()
    root = args.root.resolve()
    if args.command == "preflight":
        cmd_preflight(root, args.tag)


if __name__ == "__main__":
    main()
