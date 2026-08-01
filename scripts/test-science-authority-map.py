#!/usr/bin/env python3
"""Focused source-map regressions for the Science copied-Core authority seam."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts/report-science-authority-map.py"


def main() -> int:
    proc = subprocess.run([sys.executable, str(SCRIPT), "--check"], check=False, capture_output=True, text=True)
    value = json.loads(proc.stdout) if proc.returncode == 0 else {}
    routes = set(value.get("acp_routes", []))
    commands = set(value.get("session_command_variants", []))
    arms = set(value.get("run_loop_arms", []))
    expected = {"BeginScienceSeqAnalyze", "FinishScienceSeqAnalyze", "BeginScienceProjectMutation", "FinishScienceProjectMutation"}
    results = [
        ("authority map source check passes", proc.returncode == 0),
        ("seq_analyze and project mutation retain command and run-loop hops", expected.issubset(commands) and expected.issubset(arms)),
        ("ACP exposes the two migration-oracle routes", {"x.ai/science/seq_analyze", "x.ai/science/project_migrate"}.issubset(routes)),
        ("seq_analyze ACP handler has no raw std::fs::write bypass", value.get("raw_std_fs_write_in_seq_analyze_handler") is False),
        ("every Science command variant has a run-loop arm", value.get("command_variants_without_run_loop_arm") == []),
    ]
    for name, passed in results:
        print(f"  {'ok' if passed else 'FAIL':<4}  {name}")
    print(f"\n{'OK' if all(passed for _, passed in results) else 'FAIL'}: {sum(passed for _, passed in results)}/{len(results)} passed")
    return 0 if all(passed for _, passed in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
