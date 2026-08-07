#!/usr/bin/env python3
"""Reject Cargo dependency paths that escape the Lumen Science checkout.

This is a pre-migration anti-regression gate.  A local path dependency such as
``/Users/lei/code/lumen`` makes the product's source authority machine-local,
and therefore cannot be the one-source Lumen platform pin described by M1-A.
Workspace-internal path dependencies remain valid while the copied Core still
exists; this checker intentionally does not pretend they have been migrated.
"""

from __future__ import annotations

import argparse
import sys
import tomllib
from pathlib import Path
from typing import Any, Iterator


ROOT = Path(__file__).resolve().parent.parent


def dependency_tables(value: dict[str, Any]) -> Iterator[dict[str, Any]]:
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        table = value.get(key)
        if isinstance(table, dict):
            yield table

    workspace = value.get("workspace")
    if isinstance(workspace, dict):
        table = workspace.get("dependencies")
        if isinstance(table, dict):
            yield table

    patch = value.get("patch")
    if isinstance(patch, dict):
        for table in patch.values():
            if isinstance(table, dict):
                yield table

    target = value.get("target")
    if isinstance(target, dict):
        for target_table in target.values():
            if isinstance(target_table, dict):
                yield from dependency_tables(target_table)


def is_within(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True


def external_paths(manifest: Path, checkout_root: Path) -> list[str]:
    value = tomllib.loads(manifest.read_text(encoding="utf-8"))
    violations: list[str] = []
    for table in dependency_tables(value):
        for name, spec in table.items():
            if not isinstance(spec, dict):
                continue
            raw_path = spec.get("path")
            if not isinstance(raw_path, str) or not raw_path:
                continue
            resolved = (manifest.parent / raw_path).resolve(strict=False)
            if not is_within(resolved, checkout_root):
                violations.append(f"{manifest}: dependency {name!r} escapes checkout via {raw_path!r} -> {resolved}")
    return violations


def manifest_paths(agent_root: Path) -> list[Path]:
    return sorted(agent_root.rglob("Cargo.toml"))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, default=ROOT)
    parser.add_argument("--agent-root", type=Path)
    args = parser.parse_args()
    checkout_root = args.root.resolve()
    agent_root = (args.agent_root or checkout_root / "agent").resolve()
    if not agent_root.is_dir() or not is_within(agent_root, checkout_root):
        print("FAIL: agent root must exist within the checkout", file=sys.stderr)
        return 1

    violations: list[str] = []
    for manifest in manifest_paths(agent_root):
        try:
            violations.extend(external_paths(manifest, checkout_root))
        except (OSError, tomllib.TOMLDecodeError) as error:
            violations.append(f"{manifest}: cannot parse manifest: {error}")
    if violations:
        print("FAIL: external Cargo path dependencies are forbidden:", file=sys.stderr)
        print("\n".join(violations), file=sys.stderr)
        return 1
    print(f"PASS: {len(manifest_paths(agent_root))} Cargo manifests have no dependency path outside {checkout_root}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
