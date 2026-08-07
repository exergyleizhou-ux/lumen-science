#!/usr/bin/env python3
"""Reproduce the shell_state wrapper exit-126 on the GitHub ubuntu-24.04 runner."""
import subprocess
import sys

DUMP = r'''
dump_bash_state() {
  set -euo pipefail
  _emit() { builtin printf "%s\n" "$1"; }
  _emit_encoded() {
    local content="$1"
    local var_name="$2"
    if [[ -n "$content" ]]; then
      builtin printf "grok_snap_%s=\$(command base64 -d <<'GROK_SNAP_EOF_%s'\n" "$var_name" "$var_name"
      command base64 <<<"$content" | command tr -d "\n"
      builtin printf "\nGROK_SNAP_EOF_%s\n" "$var_name"
      builtin printf ")\n"
      builtin printf "eval \"\$grok_snap_%s\"\n" "$var_name"
    fi
  }
  _emit "__GROK_BASH_STATE_START__"
  _emit "$PWD"
  local env_vars
  env_vars=$(builtin export -p 2>/dev/null | command grep -viE "_proxy=|GROK_SANDBOX|GROK_AGENT=|SUDO_ASKPASS|GROK_ASKPASS|ELECTRON_RUN_AS_NODE|SSH_AUTH_SOCK|DBUS_SESSION_BUS_ADDRESS|XDG_RUNTIME_DIR|WAYLAND_DISPLAY|GPG_TTY" || true)
  _emit_encoded "$env_vars" "ENV_VARS_B64"
  local posix_opts
  posix_opts=$(builtin shopt -po 2>/dev/null | command grep -vE "^set [-+]o (nounset|errexit|pipefail)$" || true)
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

open("/tmp/dump.sh", "w").write(DUMP)
r = subprocess.run(["bash", "-c", "source /tmp/dump.sh; dump_bash_state"], capture_output=True, text=True)
raw = r.stdout
lines = raw.splitlines()
snapshot = "\n".join(lines[2:-1]) + "\n"
open("/tmp/snapshot.txt", "w").write(snapshot)
print("snapshot lines:", len(snapshot.splitlines()))
print("=== env vars section (decoded) ===")
# decode the ENV_VARS section for inspection
start = snapshot.find("grok_snap_ENV_VARS_B64=$(command base64 -d <<'GROK_SNAP_EOF_ENV_VARS_B64'")
if start >= 0:
    end = snapshot.find("GROK_SNAP_EOF_ENV_VARS_B64", start)
    b64 = snapshot[start:end].splitlines()[-1]
    import base64
    print(base64.b64decode(b64).decode())

WRAPPER = ('snap=$(command cat <&3) && builtin shopt -s extglob && builtin eval -- "$snap" && '
           '{ builtin set +u 2>/dev/null || true; '
           'builtin export GROK_AGENT=1; '
           'builtin export PWD="$(builtin pwd)"; '
           'builtin shopt -s expand_aliases 2>/dev/null; '
           '__grok_user_cmd="$1"; builtin declare +x __grok_user_cmd 2>/dev/null; builtin set --; '
           'builtin eval "$__grok_user_cmd" 2>&1; }; '
           'COMMAND_EXIT_CODE=$?; builtin unset __grok_user_cmd 2>/dev/null; '
           'builtin exit $COMMAND_EXIT_CODE')
r = subprocess.run(
    ["bash", "-c", f"exec 3</tmp/snapshot.txt; {WRAPPER}", "grok", "set -a"],
    capture_output=True, text=True)
print("WRAPPER_STDOUT:", r.stdout[:300])
print("WRAPPER_STDERR:", r.stderr[:600])
print("WRAPPER_EXIT:", r.returncode)
sys.exit(0)
