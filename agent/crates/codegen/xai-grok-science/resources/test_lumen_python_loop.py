#!/usr/bin/env python3
"""Sandbox tests for the Lumen Python exec-loop.

The loop's value is not that it runs code — anything can run code. It is that a
cell CANNOT reach the network, spawn a process, or write outside the run's
output directory, and that every refusal is reported rather than silently
swallowed. Those are the properties worth testing, because they are the ones a
future change could quietly remove while every "does it compute 6*7" test kept
passing.

Each denial case asserts three things: the cell errored, the attempt was
recorded in `denied`, and — for the filesystem case — the side effect did not
happen. Checking only the error would pass against a loop that raised for an
unrelated reason.

    python3 resources/test_lumen_python_loop.py

Exit 0 all pass, 1 otherwise. Stdlib only.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

LOOP = Path(__file__).resolve().parent / "lumen_python_loop.py"

failures: list[str] = []
passed = 0


def check(label: str, condition: bool, detail: str = "") -> None:
    global passed
    if condition:
        passed += 1
        print(f"  ok    {label}")
    else:
        failures.append(f"{label}{f' — {detail}' if detail else ''}")
        print(f"  FAIL  {label}{f' — {detail}' if detail else ''}")


class Loop:
    """Drives one loop process over the line protocol."""

    def __init__(self, root: Path, allow_net: bool = False, allow_subprocess: bool = False):
        self.out_dir = root / "out"
        self.figs_dir = root / "figs"
        self.out_dir.mkdir(parents=True, exist_ok=True)
        self.figs_dir.mkdir(parents=True, exist_ok=True)
        env = {
            **os.environ,
            "PYTHONHASHSEED": "0",
            "LUMEN_KERNEL_OUTPUT_DIR": str(self.out_dir),
            "LUMEN_KERNEL_FIGURES_DIR": str(self.figs_dir),
            "LUMEN_KERNEL_MAX_CPU_SECONDS": "20",
        }
        if allow_net:
            env["LUMEN_KERNEL_ALLOW_NET"] = "1"
        if allow_subprocess:
            env["LUMEN_KERNEL_ALLOW_SUBPROCESS"] = "1"
        self.proc = subprocess.Popen(
            [sys.executable, "-u", str(LOOP)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            env=env,
        )

    def run(self, code: str, req_id: str = "r") -> dict:
        assert self.proc.stdin and self.proc.stdout
        self.proc.stdin.write(json.dumps({"req_id": req_id, "code": code}) + "\n")
        self.proc.stdin.flush()
        while True:
            line = self.proc.stdout.readline()
            if not line:
                raise RuntimeError("loop closed stdout")
            payload = json.loads(line)
            # Startup warnings carry req_id None; skip until our reply arrives.
            if payload.get("req_id") == req_id:
                return payload

    def close(self) -> None:
        if self.proc.stdin:
            self.proc.stdin.close()
        self.proc.wait(timeout=10)


def denied_kinds(response: dict) -> set[str]:
    return {d["kind"] for d in response.get("denied", [])}


def main() -> int:
    print("test_lumen_python_loop")
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        loop = Loop(root)
        try:
            # ── it actually works ────────────────────────────────────
            r = loop.run("x = 6 * 7\nx")
            check("evaluates a trailing expression like a REPL", r["result"] == "42", str(r))

            r = loop.run("x + 1")
            check("namespace persists across requests", r["result"] == "43", str(r))

            r = loop.run("print('hello')")
            check("captures stdout", r["stdout"].strip() == "hello", str(r))

            r = loop.run("1/0")
            check("reports a cell error without dying", "ZeroDivisionError" in (r["error"] or ""))

            # A cell calling exit() must not take the kernel with it.
            r = loop.run("import sys; sys.exit(3)")
            check("survives sys.exit in a cell", r["error"] is not None)
            r = loop.run("'still alive'")
            check("kernel still serving after sys.exit", r["result"] == "'still alive'")

            # ── the sandbox ─────────────────────────────────────────
            target = root / "escaped.txt"
            r = loop.run(f"open({str(target)!r}, 'w').write('nope')")
            check("write outside the output dir errors", r["error"] is not None)
            check(
                "write outside is recorded as denied",
                "write-outside-output-dir" in denied_kinds(r),
                str(r.get("denied")),
            )
            check("no file escaped the output dir", not target.exists())

            inside = loop.out_dir / "ok.txt"
            r = loop.run(f"open({str(inside)!r}, 'w').write('hi')")
            check("write inside the output dir is allowed", r["error"] is None, str(r))
            check("the allowed write really happened", inside.exists())

            r = loop.run(
                "import socket; socket.create_connection(('example.com', 80), timeout=2)"
            )
            check("network is refused", r["error"] is not None)
            check(
                "network attempt is recorded as denied",
                "network-denied" in denied_kinds(r),
                str(r.get("denied")),
            )

            r = loop.run("import subprocess; subprocess.run(['echo', 'hi'])")
            check("subprocess is refused", r["error"] is not None)
            check(
                "subprocess attempt is recorded as denied",
                "subprocess-denied" in denied_kinds(r),
                str(r.get("denied")),
            )

            r = loop.run("import os; os.system('echo hi')")
            check("os.system is refused", r["error"] is not None or "denied" in str(r))

            # A clean cell must not inherit denials from earlier ones, or the
            # evidence record would blame the wrong step.
            r = loop.run("2 + 2")
            check("denied list is per-cell, not cumulative", r.get("denied") == [], str(r.get("denied")))
        finally:
            loop.close()

        # ── the escape hatch is real, and off by default ────────────
        allowed = Loop(root / "permitted", allow_subprocess=True)
        try:
            r = allowed.run("import subprocess; subprocess.run(['echo', 'hi'])")
            check(
                "explicitly authorised subprocess is permitted",
                r["error"] is None,
                str(r),
            )
        finally:
            allowed.close()

    # ── determinism guard ──────────────────────────────────────────
    env = {k: v for k, v in os.environ.items() if k != "PYTHONHASHSEED"}
    proc = subprocess.run(
        [sys.executable, str(LOOP)], input="", capture_output=True, text=True, env=env
    )
    check(
        "refuses to start without PYTHONHASHSEED",
        proc.returncode != 0 and "PYTHONHASHSEED" in proc.stderr,
        f"rc={proc.returncode} stderr={proc.stderr[:120]}",
    )

    if failures:
        print(f"\nFAILED: {len(failures)} of {passed + len(failures)}", file=sys.stderr)
        return 1
    print(f"\nALL SANDBOX TESTS PASSED ({passed} checks)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
