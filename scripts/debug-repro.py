#!/usr/bin/env python3
"""Faithful reproduction of the shell_state wrapper exit-126 on GitHub ubuntu-24.04.

Replicates exactly what `ShellState::init` + `prepare_command` + `run_command`
do for the bash `set -a` test:
  1. init:  bash -O extglob -ilc '<DUMP> printf marker; dump_bash_state'
            (-i = interactive → reads ~/.bashrc; -l = login → reads
            /etc/profile + /etc/profile.d/* + ~/.profile — the runner image's
            rc files are the platform-specific difference vs macOS/docker)
  2. snapshot parsed out of init output (same parse as parse_dump)
  3. wrapper: bash -O extglob -c '<DUMP> snap=$(command cat <&3) && ... ' -- 'set -a'
            with fd 3 = snapshot (pipe), fd 4 = dump-out pipe

Prints every snapshot section (env/opts/functions/aliases) decoded, then the
wrapper exit code, then a diagnostic wrapper run that prints what `set`,
functions, and aliases look like INSIDE the wrapper.
"""
import base64
import os
import signal
import subprocess
import sys
import time


def _deadline_alarm(signum, frame):
    raise TimeoutError("overall repro deadline exceeded")


signal.signal(signal.SIGALRM, _deadline_alarm)
signal.alarm(240)

# --- Exact text of DUMP_BASH_STATE_SCRIPT from shell_state.rs (verbatim) ---
DUMP = r'''
dump_bash_state() {
  set -euo pipefail
  if ! command -v base64 >/dev/null 2>&1; then
    echo "Error: base64 command is required" >&2
    return 1
  fi

  _emit() {
    builtin printf '%s\n' "$1"
  }

  _emit_encoded() {
    local content="$1"
    local var_name="$2"
    if [[ -n "$content" ]]; then
      builtin printf 'grok_snap_%s=$(command base64 -d <<'"'"'GROK_SNAP_EOF_%s'"'"'\n' "$var_name" "$var_name"
      command base64 <<<"$content" | command tr -d '\n'
      builtin printf '\nGROK_SNAP_EOF_%s\n' "$var_name"
      builtin printf ')\n'
      builtin printf 'eval "$grok_snap_%s"\n' "$var_name"
    fi
  }

  _emit "__GROK_BASH_STATE_START__"

  _emit "$PWD"

  local env_vars
  env_vars=$(builtin export -p 2>/dev/null | command grep -viE '_proxy=|GROK_SANDBOX|GROK_AGENT=|SUDO_ASKPASS|GROK_ASKPASS|ELECTRON_RUN_AS_NODE|SSH_AUTH_SOCK|DBUS_SESSION_BUS_ADDRESS|XDG_RUNTIME_DIR|WAYLAND_DISPLAY|GPG_TTY' || true)
  _emit_encoded "$env_vars" "ENV_VARS_B64"

  # errexit/pipefail here are this function's own `set -euo pipefail` (set is
  # shell-global in bash); replaying them would abort later user commands.
  local posix_opts
  posix_opts=$(builtin shopt -po 2>/dev/null | command grep -vE '^set [-+]o (nounset|errexit|pipefail)$' || true)
  _emit_encoded "$posix_opts" "POSIX_OPTS_B64"

  local bash_opts
  bash_opts=$(builtin shopt -p 2>/dev/null || true)
  _emit_encoded "$bash_opts" "BASH_OPTS_B64"

  local all_functions
  all_functions=$(builtin declare -f 2>/dev/null || true)
  _emit_encoded "$all_functions" "FUNCTIONS_B64"

  local aliases
  aliases=$(builtin alias -p 2>/dev/null || true)
  _emit_encoded "$aliases" "ALIASES_B64"

  _emit "# end of bash state dump"
  _emit "__GROK_BASH_STATE_END__"
}
'''

# --- Exact wrapper from prepare_command (Rust f-string escapes resolved) ---
WRAPPER_BODY = (
    "snap=$(command cat <&3) && builtin shopt -s extglob && builtin eval -- \"$snap\" && "
    "{ builtin set +u 2>/dev/null || true; "
    "builtin export GROK_AGENT=1; "
    "builtin export PWD=\"$(builtin pwd)\"; "
    "builtin shopt -s expand_aliases 2>/dev/null; "
    "builtin printf '%s' \"${2:-}\"; "
    "__grok_user_cmd=\"$1\"; builtin declare +x __grok_user_cmd 2>/dev/null; builtin set --; "
    "builtin eval \"$__grok_user_cmd\" 2>&1; }; "
    "COMMAND_EXIT_CODE=$?; builtin unset __grok_user_cmd 2>/dev/null; "
    "dump_bash_state >&4; builtin exit $COMMAND_EXIT_CODE"
)

INIT_MARKER = "__GROK_INIT_STATE_MARKER__"
START_MARKER = "__GROK_BASH_STATE_START__"
END_MARKER = "__GROK_BASH_STATE_END__"


def run_init():
    script = f"{DUMP} builtin printf '{INIT_MARKER}\\n'; dump_bash_state"
    r = subprocess.run(
        ["bash", "-O", "extglob", "-ilc", script],
        capture_output=True,
        text=True,
        timeout=120,
    )
    out = r.stdout
    if INIT_MARKER not in out:
        print(f"INIT FAILED rc={r.returncode}; marker missing; stderr head:")
        print(r.stderr[:2000])
        sys.exit(2)
    raw = out.split(INIT_MARKER, 1)[1]
    start = raw.find(START_MARKER)
    end = raw.find(END_MARKER)
    if start < 0 or end < 0:
        print("snapshot markers not found")
        sys.exit(2)
    snapshot = raw[start + len(START_MARKER):end]
    return snapshot


def parse_snapshot(snapshot):
    """Parse the dump into {section: replayable-bash-text}."""
    sections = {}
    lines = snapshot.splitlines()
    i = 0
    pwd = None
    while i < len(lines):
        line = lines[i].strip()
        if line == "# end of bash state dump":
            break
        if pwd is None and not line.startswith("grok_snap_"):
            pwd = line
            i += 1
            continue
        if line.startswith("grok_snap_"):
            name = line[len("grok_snap_"):].split("=")[0]
            i += 1  # b64 line
            b64 = lines[i].strip()
            i += 1  # heredoc end
            i += 1  # ')' line
            i += 1  # eval line
            sections[name] = base64.b64decode(b64).decode(errors="replace")
            continue
        i += 1
    return pwd, sections


def run_wrapper(user_cmd, snapshot, label):
    """Run the exact wrapper with fd 3 = snapshot pipe, fd 4 = dump pipe.

    Mirrors the Rust flow: spawn the child FIRST, then write the snapshot
    (so snapshots larger than the 64KB pipe buffer cannot deadlock), then
    read the dump until the END marker with a bounded deadline.
    """
    wrapper = f"{DUMP} {WRAPPER_BODY}"
    r_in, w_in = os.pipe()
    r_out, w_out = os.pipe()
    # The wrapper hardcodes <&3 / >&4. Like the Rust fd_mappings (which dup2
    # in the post-fork pre-exec child), renumber INSIDE the child via
    # preexec_fn so concurrent invocations cannot clobber each other's fds.
    def _map_fds():
        os.dup2(r_in, 3)
        os.dup2(w_out, 4)

    proc = subprocess.Popen(
        ["bash", "-O", "extglob", "-c", wrapper, "--", user_cmd],
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        pass_fds=(r_in, w_out, 3, 4),  # 3/4: preexec dup2s them; keep them out of close_fds
        preexec_fn=_map_fds,
    )
    os.close(r_in)
    os.close(w_out)
    os.write(w_in, snapshot.encode())
    os.close(w_in)
    stdout, stderr = proc.communicate(timeout=120)
    # Drain fd 4 until END marker or deadline (a backgrounded grandchild may
    # keep the write end open forever; the Rust reader has the same guard).
    dump_data = b""
    deadline = time.time() + 10
    while time.time() < deadline and END_MARKER not in dump_data.decode(errors="replace"):
        try:
            chunk = os.read(r_out, 65536)
            if not chunk:
                break
            dump_data += chunk
        except BlockingIOError:
            time.sleep(0.05)
        except OSError:
            break
    os.close(r_out)
    print(f"=== {label}: exit={proc.returncode}", flush=True)
    if stdout:
        print("stdout:", stdout[:2000].decode(errors="replace"), flush=True)
    if stderr:
        print("stderr:", stderr[:2000].decode(errors="replace"), flush=True)
    return proc.returncode, dump_data


def run_parallel(user_cmd, snapshot, count, label):
    """Spawn `count` concurrent wrapper invocations (mimics tokio test parallelism)."""
    import threading

    results = []
    lock = threading.Lock()

    def worker(i):
        code, _ = run_wrapper(user_cmd, snapshot, f"PAR{i}")
        with lock:
            results.append(code)

    threads = [threading.Thread(target=worker, args=(i,)) for i in range(count)]
    for t in threads:
        t.start()
    for t in threads:
        t.join(timeout=180)
    from collections import Counter

    print(f"=== {label}: {count} concurrent -> {dict(Counter(results))}", flush=True)


def main():
    print(f"bash: {subprocess.run(['bash', '--version'], capture_output=True, text=True).stdout.splitlines()[0]}", flush=True)
    snapshot = run_init()
    print("snapshot bytes:", len(snapshot), flush=True)
    pwd, sections = parse_snapshot(snapshot)
    print("PWD:", pwd, flush=True)
    # Rust parse_dump strips the first ($PWD) line from the replayable snapshot;
    # mirror that exactly (snapshot here starts with "\n" + PWD + "\n" + sections).
    body = snapshot[1:]  # drop the newline that ends the START marker line
    newline_pos = body.find("\n")  # end of the PWD line
    replay_snapshot = body[newline_pos:]  # includes leading \n, like Rust `rest`
    print("replay snapshot bytes:", len(replay_snapshot))
    print("ENV section:\n" + sections.get("ENV_VARS_B64", "<missing>"))
    print("POSIX_OPTS section:\n" + sections.get("POSIX_OPTS_B64", "<missing>"))
    print("BASH_OPTS section:\n" + sections.get("BASH_OPTS_B64", "<missing>"))
    funcs = sections.get("FUNCTIONS_B64", "")
    print("FUNCTIONS section (names only):")
    for line in funcs.splitlines():
        if line and not line.startswith((" ", "\t", "}")):
            print("  ", line)
    print("ALIASES section (full):")
    print(sections.get("ALIASES_B64", "<missing>"))

    # Reproduction: the exact failing command
    code, _ = run_wrapper("set -a", replay_snapshot, "REPRO set -a")
    print("REPRO_EXIT:", code)

    # Diagnostic: what does `set` resolve to inside the wrapper?
    diag = ("builtin type -a set; echo FUNC_NAMES; builtin declare -F | command head -60; "
            "echo ALIAS_TABLE; builtin alias -p | command head -60; echo DIAG_DONE")
    code2, _ = run_wrapper(diag, replay_snapshot, "DIAGNOSTIC inside wrapper")
    print("DIAG_EXIT:", code2)

    # Parallel pressure: 16 concurrent wrappers, 2 rounds (mimics the tokio
    # multi-threaded test harness where 3000+ tests spawn shells concurrently).
    for round_no in (1, 2):
        run_parallel("set -a", replay_snapshot, 16, f"PARALLEL round {round_no}")

    # Isolation: wrapper with an empty snapshot (is the failure in the
    # snapshot replay or the wrapper itself?)
    code3, _ = run_wrapper("set -a", "\n", "EMPTY-SNAPSHOT set -a")
    print("EMPTY_SNAPSHOT_EXIT:", code3)
    sys.exit(0 if code == 0 else 1)


if __name__ == "__main__":
    main()
