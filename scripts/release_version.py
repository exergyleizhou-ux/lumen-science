#!/usr/bin/env python3
"""Read, validate, and atomically bump Lumen's synchronized release version."""

from __future__ import annotations

import argparse
import os
import re
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


VERSION_FILE = Path("VERSION")
MANIFESTS = (
    Path("agent/crates/codegen/xai-grok-version/Cargo.toml"),
    Path("agent/crates/codegen/xai-grok-pager/Cargo.toml"),
    Path("agent/crates/codegen/xai-grok-pager-bin/Cargo.toml"),
    Path("agent/crates/codegen/xai-grok-shell/Cargo.toml"),
    Path("agent/crates/codegen/xai-grok-tools/Cargo.toml"),
    Path("agent/crates/codegen/xai-grok-tools-api/Cargo.toml"),
    Path("agent/crates/codegen/xai-grok-update/Cargo.toml"),
    Path("agent/crates/codegen/xai-grok-workspace/Cargo.toml"),
)
LOCK_FILE = Path("agent/Cargo.lock")
PACKAGE_NAMES = {path.parent.name for path in MANIFESTS}
SEMVER_RE = re.compile(
    r"^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    r"(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?"
    r"(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$"
)


class VersionError(RuntimeError):
    pass


@dataclass(frozen=True)
class Version:
    major: int
    minor: int
    patch: int
    prerelease: tuple[str, ...] = ()
    build: tuple[str, ...] = ()

    @classmethod
    def parse(cls, raw: str) -> "Version":
        match = SEMVER_RE.fullmatch(raw)
        if not match:
            raise VersionError(f"not valid SemVer: {raw!r}")
        prerelease = tuple((match.group(4) or "").split(".")) if match.group(4) else ()
        build = tuple((match.group(5) or "").split(".")) if match.group(5) else ()
        for identifier in prerelease:
            if (
                identifier.isdigit()
                and len(identifier) > 1
                and identifier.startswith("0")
            ):
                raise VersionError(
                    f"numeric prerelease identifier has a leading zero: {raw!r}"
                )
        return cls(
            int(match.group(1)),
            int(match.group(2)),
            int(match.group(3)),
            prerelease,
            build,
        )

    def __str__(self) -> str:
        value = f"{self.major}.{self.minor}.{self.patch}"
        if self.prerelease:
            value += "-" + ".".join(self.prerelease)
        if self.build:
            value += "+" + ".".join(self.build)
        return value

    def precedence_key(self) -> tuple[object, ...]:
        if not self.prerelease:
            prerelease: tuple[object, ...] = (1,)
        else:
            parts: list[object] = [0]
            for identifier in self.prerelease:
                parts.append(
                    (0, int(identifier)) if identifier.isdigit() else (1, identifier)
                )
            prerelease = tuple(parts)
        return (self.major, self.minor, self.patch, prerelease)


def atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    mode = path.stat().st_mode if path.exists() else 0o644
    with tempfile.NamedTemporaryFile(
        "w", encoding="utf-8", dir=path.parent, delete=False
    ) as handle:
        handle.write(content)
        temporary = Path(handle.name)
    os.chmod(temporary, mode)
    os.replace(temporary, path)


def version_from_file(root: Path) -> Version:
    path = root / VERSION_FILE
    if not path.is_file():
        raise VersionError(f"missing version source: {VERSION_FILE}")
    raw = path.read_text(encoding="utf-8")
    if not raw.endswith("\n") or raw.count("\n") != 1:
        raise VersionError("VERSION must contain exactly one newline-terminated SemVer")
    return Version.parse(raw.rstrip("\n"))


def manifest_package_version(path: Path) -> tuple[str, Version]:
    in_package = False
    package_name: str | None = None
    version: Version | None = None
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line.startswith("["):
            in_package = line == "[package]"
            continue
        if not in_package:
            continue
        name_match = re.fullmatch(r'name\s*=\s*"([^"]+)"(?:\s*#.*)?', line)
        version_match = re.fullmatch(r'version\s*=\s*"([^"]+)"(?:\s*#.*)?', line)
        if name_match:
            package_name = name_match.group(1)
        elif version_match:
            version = Version.parse(version_match.group(1))
    if package_name is None or version is None:
        raise VersionError(f"missing [package] name/version in {path}")
    return package_name, version


def lock_versions(path: Path) -> dict[str, Version]:
    versions: dict[str, Version] = {}
    current_name: str | None = None
    current_version: Version | None = None

    def record() -> None:
        if current_name in PACKAGE_NAMES and current_version is not None:
            versions[current_name] = current_version

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line == "[[package]]":
            record()
            current_name = None
            current_version = None
        elif current_name is None:
            match = re.fullmatch(r'name\s*=\s*"([^"]+)"', line)
            if match:
                current_name = match.group(1)
        elif current_version is None:
            match = re.fullmatch(r'version\s*=\s*"([^"]+)"', line)
            if match:
                current_version = Version.parse(match.group(1))
    record()
    return versions


def check(root: Path) -> Version:
    expected = version_from_file(root)
    seen_names: set[str] = set()
    for relative in MANIFESTS:
        path = root / relative
        if not path.is_file():
            raise VersionError(f"missing package manifest: {relative}")
        package_name, actual = manifest_package_version(path)
        if package_name != relative.parent.name:
            raise VersionError(f"unexpected package name in {relative}: {package_name}")
        seen_names.add(package_name)
        if actual != expected:
            raise VersionError(
                f"version mismatch: {relative} has {actual}, VERSION has {expected}"
            )
    if seen_names != PACKAGE_NAMES:
        raise VersionError("release package manifest set is incomplete")
    lock = root / LOCK_FILE
    if not lock.is_file():
        raise VersionError(f"missing Cargo lockfile: {LOCK_FILE}")
    locked = lock_versions(lock)
    if set(locked) != PACKAGE_NAMES:
        missing = sorted(PACKAGE_NAMES - set(locked))
        raise VersionError(
            f"release packages missing from Cargo.lock: {', '.join(missing)}"
        )
    for package_name, actual in sorted(locked.items()):
        if actual != expected:
            raise VersionError(
                f"version mismatch: Cargo.lock {package_name} has {actual}, VERSION has {expected}"
            )
    return expected


def next_version(current: Version, spec: str) -> Version:
    if spec == "patch":
        if current.prerelease or current.build:
            return Version(current.major, current.minor, current.patch)
        return Version(current.major, current.minor, current.patch + 1)
    if spec == "minor":
        return Version(current.major, current.minor + 1, 0)
    if spec == "major":
        return Version(current.major + 1, 0, 0)
    if spec == "prerelease":
        if current.prerelease and current.prerelease[-1].isdigit():
            identifiers = (
                *current.prerelease[:-1],
                str(int(current.prerelease[-1]) + 1),
            )
            return Version(current.major, current.minor, current.patch, identifiers)
        if current.prerelease:
            return Version(
                current.major, current.minor, current.patch, (*current.prerelease, "1")
            )
        return Version(current.major, current.minor, current.patch + 1, ("alpha", "1"))
    requested = Version.parse(spec.removeprefix("v"))
    if requested.precedence_key() <= current.precedence_key():
        raise VersionError(
            f"new version {requested} must have higher precedence than {current}"
        )
    return requested


def replace_manifest_version(
    content: str, expected_name: str, old: Version, new: Version
) -> str:
    pattern = re.compile(
        rf'(?ms)(\[package\]\s+.*?^name\s*=\s*"{re.escape(expected_name)}".*?^version\s*=\s*)"{re.escape(str(old))}"'
    )
    updated, count = pattern.subn(rf'\g<1>"{new}"', content, count=1)
    if count != 1:
        raise VersionError(
            f"could not replace exactly one package version for {expected_name}"
        )
    return updated


def replace_lock_versions(content: str, old: Version, new: Version) -> str:
    updated = content
    for package_name in sorted(PACKAGE_NAMES):
        pattern = re.compile(
            rf'(?ms)(\[\[package\]\]\s+name\s*=\s*"{re.escape(package_name)}"\s+version\s*=\s*)"{re.escape(str(old))}"'
        )
        updated, count = pattern.subn(rf'\g<1>"{new}"', updated, count=1)
        if count != 1:
            raise VersionError(
                f"could not replace Cargo.lock version for {package_name}"
            )
    return updated


def set_version(root: Path, new: Version) -> None:
    old = check(root)
    if new.precedence_key() <= old.precedence_key():
        raise VersionError(f"new version {new} must have higher precedence than {old}")
    pending: dict[Path, str] = {root / VERSION_FILE: f"{new}\n"}
    for relative in MANIFESTS:
        path = root / relative
        pending[path] = replace_manifest_version(
            path.read_text(encoding="utf-8"), relative.parent.name, old, new
        )
    lock = root / LOCK_FILE
    pending[lock] = replace_lock_versions(lock.read_text(encoding="utf-8"), old, new)
    for path, content in pending.items():
        atomic_write(path, content)
    check(root)


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument(
        "--root", type=Path, default=Path(__file__).resolve().parent.parent
    )
    subcommands = result.add_subparsers(dest="command", required=True)
    subcommands.add_parser(
        "check", help="verify VERSION, manifests, and Cargo.lock agree"
    )
    next_parser = subcommands.add_parser(
        "next", help="print the version produced by a bump spec"
    )
    next_parser.add_argument(
        "spec", help="patch, minor, major, prerelease, or an explicit SemVer"
    )
    set_parser = subcommands.add_parser(
        "set", help="apply a bump spec to all version-bearing files"
    )
    set_parser.add_argument(
        "spec", help="patch, minor, major, prerelease, or an explicit SemVer"
    )
    return result


def main() -> int:
    args = parser().parse_args()
    root = args.root.resolve()
    try:
        current = check(root)
        if args.command == "check":
            print(current)
        elif args.command == "next":
            print(next_version(current, args.spec))
        else:
            target = next_version(current, args.spec)
            set_version(root, target)
            print(target)
    except (OSError, VersionError) as exc:
        print(f"FAIL: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
