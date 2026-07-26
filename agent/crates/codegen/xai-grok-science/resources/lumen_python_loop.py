# Lumen Science persistent Python exec-loop.
#
# Adapted from Open Science (Apache-2.0), commit d8f11e34,
# resources/notebook/python_loop.py. Statement of changes at the end of this
# header, as Apache-2.0 section 4(b) requires.
#
# Reads one JSON request per line, runs it against a persistent namespace, and
# returns one JSON response per line. Deliberately NOT Jupyter: no ZMQ, no five
# sockets, no message signing — one process, two pipes, one line per exchange.
#
#   driver -> loop:  {"req_id", "code"}
#   loop -> driver:  {"req_id", "stdout", "stderr", "error", "result", "cwd",
#                     "figures": [{"mime", "path"}], "denied": [...]}
#
# WHAT LUMEN CHANGED, and why
# ---------------------------
# Upstream runs this as its own execution authority: the Node driver decides
# what to run and the loop trusts it. In Lumen the SessionActor decides, and
# this loop is an adapter that must be safe even if the layer above it is wrong.
# So the sandbox is enforced HERE as well, not only by the caller:
#
#   1. Network is denied by default. Upstream's audit hook covers file opens
#      only; a cell could open a socket and exfiltrate an artifact. The hook now
#      also refuses `socket.connect`, and is opt-in via LUMEN_KERNEL_ALLOW_NET
#      so an authorised live step can still work.
#   2. Subprocess execution is denied. Upstream has no guard, so `os.system` or
#      `subprocess.run` re-obtained the arbitrary-shell capability the whole
#      architecture forbids.
#   3. Writes are confined to one output directory. Upstream protects specific
#      application directories (a denylist); Lumen inverts it to an allowlist,
#      because a denylist cannot enumerate everything a cell must not touch.
#      Input artifacts are readable, never writable.
#   4. Denied attempts are REPORTED, not merely blocked. Each response carries
#      a `denied` list, so a step that quietly failed to reach the network is
#      distinguishable from one that never tried — evidence, not just defence.
#   5. Deterministic environment: PYTHONHASHSEED, TZ and locale are pinned by
#      the driver, and the loop refuses to start if PYTHONHASHSEED is unset,
#      since set-iteration order would otherwise vary between replays of the
#      same run.
#   6. Resource limits (address space, CPU seconds, file size, open files) are
#      applied from the environment via `resource`, so a runaway cell dies
#      instead of taking the host with it.
#   7. Env var prefix OPEN_SCIENCE_ -> LUMEN_KERNEL_.
#
# Retained from upstream, essentially unchanged, because the reasoning was
# already right: the private dup() of fd 1 so protocol output survives user code
# reassigning stdout; REPL semantics via ast trailing-expression eval;
# KeyboardInterrupt and SystemExit both handled so a soft timeout or a cell
# calling exit() cannot kill the loop; content-addressed figure capture.

import ast
import hashlib
import io
import json
import os
import sys
import traceback

# Protocol output must survive user code that reassigns fd 1: keep a private
# handle to the real stdout. (Upstream's reasoning, kept verbatim in spirit.)
_protocol_out = os.fdopen(os.dup(1), "w", buffering=1)

# Then take fd 1 away from user code entirely, pointing it at stderr.
#
# Upstream's dup protects the protocol from Python-level reassignment of
# sys.stdout, but not from a CHILD PROCESS: a spawned command inherits fd 1 and
# writes directly into the pipe the driver parses as protocol, producing a
# non-JSON line mid-stream. Found by the sandbox test, which drives the
# authorised-subprocess path — the very case the guard exists to make safe.
#
# After this, fd 1 is stderr: anything a child prints lands in diagnostics
# instead of corrupting the exchange, while _protocol_out keeps the real pipe.
# Cell-level prints are unaffected: _run swaps sys.stdout for a StringIO.
os.dup2(2, 1)

_figures_dir = os.environ.get("LUMEN_KERNEL_FIGURES_DIR", "")
_output_dir = os.environ.get("LUMEN_KERNEL_OUTPUT_DIR", "")
_allow_net = os.environ.get("LUMEN_KERNEL_ALLOW_NET", "") == "1"
_allow_subprocess = os.environ.get("LUMEN_KERNEL_ALLOW_SUBPROCESS", "") == "1"

# Replay determinism: unordered set/dict iteration would otherwise differ run to
# run, and a workflow that claims reproducibility cannot start by randomising
# its own hashing. Fail loudly rather than producing quietly unstable output.
if not os.environ.get("PYTHONHASHSEED"):
    sys.stderr.write(
        "lumen kernel: PYTHONHASHSEED must be set by the driver for replay determinism\n"
    )
    raise SystemExit(2)


def _apply_resource_limits():
    """Bound the cell so a runaway cannot take the host down with it.

    Best-effort: `resource` is POSIX-only, and a limit that cannot be applied is
    reported rather than silently skipped, so the driver never believes a cap is
    in force when it is not.
    """
    try:
        import resource
    except ImportError:
        return ["resource module unavailable; no rlimits applied"]

    problems = []
    caps = (
        ("LUMEN_KERNEL_MAX_ADDRESS_SPACE", "RLIMIT_AS"),
        ("LUMEN_KERNEL_MAX_CPU_SECONDS", "RLIMIT_CPU"),
        ("LUMEN_KERNEL_MAX_FILE_BYTES", "RLIMIT_FSIZE"),
        ("LUMEN_KERNEL_MAX_OPEN_FILES", "RLIMIT_NOFILE"),
    )
    for env_name, limit_name in caps:
        raw = os.environ.get(env_name)
        if not raw:
            continue
        try:
            value = int(raw)
            which = getattr(resource, limit_name)
            resource.setrlimit(which, (value, value))
        except (ValueError, AttributeError, OSError) as exc:
            problems.append(f"{limit_name}: {exc}")
    return problems


_rlimit_problems = _apply_resource_limits()

# Every denial observed while running the current cell. Collected rather than
# only raised: a step that tried to reach the network and was stopped is a
# materially different fact from one that never tried, and the evidence chain
# should record which happened.
_denied: list = []

_BOOTSTRAP = r'''
import os, sys, warnings
warnings.filterwarnings("ignore", message=".*is non-interactive, and thus cannot be shown")

_output_dir = os.environ.get("LUMEN_KERNEL_OUTPUT_DIR", "")
_allow_net = os.environ.get("LUMEN_KERNEL_ALLOW_NET", "") == "1"
_allow_subprocess = os.environ.get("LUMEN_KERNEL_ALLOW_SUBPROCESS", "") == "1"
_writable_roots = [os.path.abspath(_output_dir)] if _output_dir else []


def _record(kind, detail):
    # Written where the loop can read it back after the cell returns.
    sys.modules["__main__"].__dict__.setdefault("__lumen_denied__", []).append(
        {"kind": kind, "detail": str(detail)[:512]}
    )


def _is_write_mode(args):
    # audit "open" args are (path, mode, flags); mode is None for os.open.
    if len(args) < 2 or not isinstance(args[1], str):
        return False
    return any(ch in args[1] for ch in ("w", "a", "x", "+"))


def _lumen_audit(event, args):
    # Writes: allowlist, not denylist. A denylist cannot enumerate everything a
    # cell must not touch; an allowlist states exactly where output may go.
    if event == "open" and args and _is_write_mode(args):
        target = args[0]
        if target is None or isinstance(target, int):
            return
        try:
            resolved = os.path.abspath(os.fspath(target))
        except (TypeError, ValueError):
            return
        if not any(
            resolved == root or resolved.startswith(root + os.sep)
            for root in _writable_roots
        ):
            _record("write-outside-output-dir", resolved)
            raise PermissionError(
                f"lumen kernel: writes are confined to the run output directory "
                f"(attempted {resolved})"
            )

    # Network: denied unless the step was explicitly authorised for it.
    if event in ("socket.connect", "socket.getaddrinfo") and not _allow_net:
        _record("network-denied", args[1] if len(args) > 1 else event)
        raise PermissionError(
            "lumen kernel: network access is disabled for this step"
        )

    # Subprocess: this is how arbitrary shell would re-enter, which the
    # architecture forbids regardless of what the calling layer intended.
    if event in ("subprocess.Popen", "os.system", "os.exec", "os.spawn") and not _allow_subprocess:
        _record("subprocess-denied", args[0] if args else event)
        raise PermissionError(
            "lumen kernel: launching processes is not permitted from a cell"
        )


sys.addaudithook(_lumen_audit)
'''

_globals = {"__name__": "__main__"}
sys.modules["__main__"].__dict__["__lumen_denied__"] = []
exec(compile(_BOOTSTRAP, "<lumen-bootstrap>", "exec"), _globals)


def _capture_figures():
    """Render every open matplotlib figure to a content-addressed PNG, then close.

    No-op when matplotlib was never imported, so a pure-compute cell pays
    nothing. Content-addressed so an identical figure across a replay lands on
    the same path and the same digest.
    """
    figures = []
    module = sys.modules.get("matplotlib")
    if module is None or not _figures_dir:
        return figures
    try:
        from matplotlib._pylab_helpers import Gcf
    except Exception:
        return figures
    for manager in list(Gcf.get_all_fig_managers()):
        try:
            buf = io.BytesIO()
            manager.canvas.figure.savefig(buf, format="png", bbox_inches="tight")
            data = buf.getvalue()
            digest = hashlib.sha256(data).hexdigest()
            path = os.path.join(_figures_dir, digest + ".png")
            with open(path, "wb") as handle:
                handle.write(data)
            figures.append({"mime": "image/png", "path": path, "sha256": digest})
        except Exception:
            continue
    try:
        import matplotlib.pyplot as plt
        plt.close("all")
    except Exception:
        return figures
    return figures


def _drain_denied():
    """Take and clear the denials recorded during the last cell."""
    bucket = sys.modules["__main__"].__dict__.get("__lumen_denied__", [])
    taken = list(bucket)
    bucket.clear()
    return taken


def _run(code):
    """Run one request against the persistent namespace.

    Execs all but a trailing bare expression, then evals that expression so its
    repr echoes like a REPL. KeyboardInterrupt (from a soft-timeout SIGINT) is
    caught so the process survives and the driver can map the reply to a
    timeout; SystemExit likewise, so a cell calling exit() reports an error
    instead of killing the kernel.
    """
    out, err = io.StringIO(), io.StringIO()
    old_out, old_err = sys.stdout, sys.stderr
    sys.stdout, sys.stderr = out, err
    error = None
    result = None
    try:
        parsed = ast.parse(code, mode="exec")
        body = parsed.body
        tail = None
        if body and isinstance(body[-1], ast.Expr):
            tail = ast.Expression(body.pop().value)
        if body:
            exec(compile(ast.Module(body, type_ignores=[]), "<cell>", "exec"), _globals)
        if tail is not None:
            value = eval(compile(tail, "<cell>", "eval"), _globals)
            if value is not None:
                result = repr(value)
    except KeyboardInterrupt:
        error = "KeyboardInterrupt\n" + traceback.format_exc()
    except SystemExit:
        error = traceback.format_exc()
    except Exception:
        error = traceback.format_exc()
    finally:
        sys.stdout, sys.stderr = old_out, old_err
    figures = _capture_figures()
    return {
        "stdout": out.getvalue(),
        "stderr": err.getvalue(),
        "error": error,
        "result": result,
        "cwd": os.getcwd(),
        "figures": figures,
        "denied": _drain_denied(),
    }


def main():
    # Report any rlimit that could not be applied before serving, so the driver
    # never assumes a cap is in force when it is not.
    if _rlimit_problems:
        _protocol_out.write(
            json.dumps({"req_id": None, "kernel_warning": _rlimit_problems}) + "\n"
        )
        _protocol_out.flush()

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except Exception:
            continue
        req_id = request.get("req_id")
        try:
            # The emit stays inside this guard: a soft-timeout SIGINT can land
            # anywhere, including during figure capture or the response write.
            response = _run(request.get("code", ""))
            response["req_id"] = req_id
            _protocol_out.write(json.dumps(response) + "\n")
            _protocol_out.flush()
        except (KeyboardInterrupt, Exception):
            fallback = {
                "stdout": "",
                "stderr": "",
                "error": traceback.format_exc(),
                "result": None,
                "cwd": os.getcwd(),
                "figures": [],
                "denied": _drain_denied(),
                "req_id": req_id,
            }
            try:
                _protocol_out.write(json.dumps(fallback) + "\n")
                _protocol_out.flush()
            except Exception:
                # The pipe is gone; drop this response and keep serving.
                continue


if __name__ == "__main__":
    main()
