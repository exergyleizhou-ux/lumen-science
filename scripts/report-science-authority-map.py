#!/usr/bin/env python3
"""Report the copied-Core Science authority surface before any migration.

This is intentionally source-only evidence.  It identifies the four hops a
future public platform port must replace together: ACP route, handle API,
SessionCommand/run-loop arm, and SessionActor method.  It does not claim that
the routes are safe or product-tested.
"""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
SHELL = ROOT / "agent/crates/codegen/xai-grok-shell/src"
COMMANDS = SHELL / "session/commands.rs"
RUN_LOOP = SHELL / "session/acp_session_impl/run_loop.rs"
ACTOR = SHELL / "session/acp_session_impl/science.rs"
HANDLE = SHELL / "session/handle.rs"
ROUTES = SHELL / "extensions/science.rs"


def read(path: Path) -> str:
    if not path.is_file():
        raise ValueError(f"required authority file is missing: {path.relative_to(ROOT)}")
    return path.read_text(encoding="utf-8")


def unique(pattern: str, text: str) -> list[str]:
    return sorted(set(re.findall(pattern, text, flags=re.MULTILINE)))


def function_body(text: str, name: str, next_marker: str) -> str:
    start = text.find(name)
    end = text.find(next_marker, start)
    if start < 0 or end < 0:
        raise ValueError(f"cannot isolate {name} ACP handler")
    return text[start:end]


def report() -> dict[str, Any]:
    commands = read(COMMANDS)
    run_loop = read(RUN_LOOP)
    actor = read(ACTOR)
    handle = read(HANDLE)
    routes = read(ROUTES)
    seq_handler = function_body(routes, "async fn handle_seq_analyze", "/// Import an uploaded ZIP/.skill")
    command_variants = unique(r"^\s{4}([A-Za-z0-9]*Science[A-Za-z0-9]*)\(Box<", commands)
    run_loop_arms = unique(r"SessionCommand::([A-Za-z0-9]*Science[A-Za-z0-9]*)\(command\)\s*=>", run_loop)
    actor_methods = unique(r"^\s*pub\(super\)\s+(?:async\s+)?fn\s+([a-z0-9_]*science[a-z0-9_]*)\(", actor)
    handle_methods = unique(r"^\s*pub\s+async\s+fn\s+(run_science_[a-z0-9_]+)\(", handle)
    acp_routes = unique(r'"(x\.ai/science/[a-z0-9_]+)"\s*=>', routes)
    expected = {"BeginScienceSeqAnalyze", "FinishScienceSeqAnalyze", "BeginScienceProjectMutation", "FinishScienceProjectMutation"}
    missing_run_loop = sorted(set(command_variants) - set(run_loop_arms))
    return {
        "schema_version": 1,
        "scope": "source map of the copied Rust Core only; no runtime, binary, CI, or release claim",
        "files": [str(path.relative_to(ROOT)) for path in (COMMANDS, RUN_LOOP, ACTOR, HANDLE, ROUTES)],
        "session_command_variants": command_variants,
        "run_loop_arms": run_loop_arms,
        "session_actor_methods": actor_methods,
        "session_handle_methods": handle_methods,
        "acp_routes": acp_routes,
        "required_seq_and_project_commands_present": expected.issubset(set(command_variants)) and expected.issubset(set(run_loop_arms)),
        "command_variants_without_run_loop_arm": missing_run_loop,
        "raw_std_fs_write_in_seq_analyze_handler": "std::fs::write" in seq_handler,
        "migration_rule": "Do not delete any listed hop until a public Lumen platform port has exact compatibility and negative-path proof.",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        value = report()
        if args.check:
            if value["command_variants_without_run_loop_arm"]:
                raise ValueError("Science SessionCommand variants exist without run-loop arms")
            if value["required_seq_and_project_commands_present"] is not True:
                raise ValueError("seq_analyze or project_migrate authority chain is incomplete")
            if value["raw_std_fs_write_in_seq_analyze_handler"]:
                raise ValueError("seq_analyze ACP handler contains raw std::fs::write")
        print(json.dumps(value, indent=2, sort_keys=True))
    except (OSError, ValueError) as error:
        print(f"FAIL: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
