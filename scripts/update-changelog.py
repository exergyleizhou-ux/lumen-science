#!/usr/bin/env python3
"""Move Unreleased notes into a version and add categorized Git history."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from collections import defaultdict
from datetime import date
from pathlib import Path


SEMVER_TAG = re.compile(
    r"^v[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
CATEGORY_ORDER = (
    "Added",
    "Fixed",
    "Security",
    "Changed",
    "Documentation",
    "Maintenance",
)


def git(root: Path, *args: str) -> str:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout.rstrip("\n")


def previous_tag(root: Path) -> str | None:
    for tag in git(
        root, "tag", "--merged", "HEAD", "--sort=-version:refname"
    ).splitlines():
        if SEMVER_TAG.fullmatch(tag):
            return tag
    return None


def categorized_commits(root: Path, start: str | None) -> dict[str, list[str]]:
    revision = f"{start}..HEAD" if start else "HEAD"
    output = git(root, "log", "--no-merges", "--format=%h%x09%s", revision)
    categories: dict[str, list[str]] = defaultdict(list)
    for line in output.splitlines():
        if "\t" not in line:
            continue
        short_sha, subject = line.split("\t", 1)
        subject = " ".join(subject.split()).replace("<", "&lt;").replace(">", "&gt;")
        conventional = re.match(r"^([A-Za-z]+)(?:\([^)]*\))?(!)?:\s*(.+)$", subject)
        kind = conventional.group(1).lower() if conventional else ""
        breaking = bool(conventional and conventional.group(2))
        text = conventional.group(3) if conventional else subject
        if kind == "feat":
            category = "Added"
        elif kind == "fix":
            category = "Fixed"
        elif kind in {"security", "sec"}:
            category = "Security"
        elif kind in {"docs", "doc"}:
            category = "Documentation"
        elif kind in {"chore", "ci", "build", "test", "style"}:
            category = "Maintenance"
        else:
            category = "Changed"
        if breaking:
            text = f"**Breaking:** {text}"
        bullet = f"- {text} (`{short_sha}`)"
        if not (
            kind == "chore"
            and text.lower().startswith(("prepare v", "release v", "bump version"))
        ):
            categories[category].append(bullet)
    return categories


def render_categories(categories: dict[str, list[str]]) -> str:
    sections: list[str] = []
    for category in CATEGORY_ORDER:
        bullets = categories.get(category, [])
        if bullets:
            sections.append(f"### {category}\n\n" + "\n".join(bullets))
    return "\n\n".join(sections)


def merge_unreleased(
    body: str, categories: dict[str, list[str]]
) -> tuple[list[str], dict[str, list[str]]]:
    extras: list[str] = []
    parts = re.split(r"(?m)^### ([^\n]+)\n", body)
    if parts[0].strip():
        extras.append(parts[0].strip())
    for index in range(1, len(parts), 2):
        heading = parts[index].strip()
        section = parts[index + 1].strip()
        if heading in CATEGORY_ORDER:
            existing = [line for line in section.splitlines() if line.strip()]
            categories[heading] = existing + categories.get(heading, [])
        elif section:
            extras.append(f"### {heading}\n\n{section}")
    for heading, entries in categories.items():
        categories[heading] = list(dict.fromkeys(entries))
    return extras, categories


def update(root: Path, version: str, release_date: str) -> None:
    changelog = root / "CHANGELOG.md"
    if not changelog.is_file():
        raise RuntimeError("CHANGELOG.md is missing")
    content = changelog.read_text(encoding="utf-8")
    if f"## [{version}]" in content:
        raise RuntimeError(f"CHANGELOG.md already contains version {version}")
    marker = "## [Unreleased]"
    marker_at = content.find(marker)
    if marker_at < 0:
        raise RuntimeError("CHANGELOG.md is missing an [Unreleased] section")
    body_start = marker_at + len(marker)
    next_section = re.search(r"(?m)^## \[", content[body_start:])
    body_end = body_start + next_section.start() if next_section else len(content)
    unreleased = content[body_start:body_end].strip()
    extras, categories = merge_unreleased(
        unreleased, categorized_commits(root, previous_tag(root))
    )
    history = render_categories(categories)
    release_parts = [*extras, *([history] if history else [])]
    if not release_parts:
        release_parts = ["- No user-facing changes."]
    release = f"## [{version}] - {release_date}\n\n" + "\n\n".join(release_parts)
    prefix = content[:body_start].rstrip() + "\n\n"
    suffix = content[body_end:].lstrip()
    updated = prefix + release + "\n"
    if suffix:
        updated += "\n" + suffix
    changelog.write_text(updated, encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parent.parent
    )
    parser.add_argument("--date", default=date.today().isoformat())
    args = parser.parse_args()
    if not re.fullmatch(
        r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?", args.version
    ):
        print(f"FAIL: invalid release version: {args.version}", file=sys.stderr)
        return 1
    try:
        update(args.root.resolve(), args.version, args.date)
    except (OSError, RuntimeError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    print(f"OK: updated CHANGELOG.md for v{args.version}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
