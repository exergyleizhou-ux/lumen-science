#!/usr/bin/env bash
# Deterministic contract test for probe-local exit codes and tool-call parsing.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$(mktemp -d)"
PIDS=()
cleanup() {
  local pid
  for pid in "${PIDS[@]:-}"; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in "${PIDS[@]:-}"; do
    wait "$pid" 2>/dev/null || true
  done
  rm -rf "$TMP"
}
trap cleanup EXIT

start_fixture() {
  local mode="$1" port_file="$TMP/$1.port"
  python3 - "$mode" "$port_file" >/dev/null 2>&1 <<'PY' &
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

mode, port_file = sys.argv[1:]

class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        if self.path == "/v1/models":
            self.send_json({"data": [{"id": "fixture-model"}]})
        else:
            self.send_error(404)

    def do_POST(self):
        if self.path != "/v1/chat/completions":
            self.send_error(404)
            return
        length = int(self.headers.get("Content-Length", "0"))
        request_body = json.loads(self.rfile.read(length))
        if request_body.get("max_tokens") != 2048:
            self.send_error(422, "probe token cap must remain 2048 for thinking models")
            return
        if mode == "tool":
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [{
                    "id": "call_fixture",
                    "type": "function",
                    "function": {
                        "name": "edit_file",
                        "arguments": "{\"path\":\"main.go\",\"line\":3,\"text\":\"hello\"}",
                    },
                }],
            }
        elif mode == "invalid":
            message = {
                "role": "assistant",
                "content": None,
                "tool_calls": [
                    {
                        "id": "call_wrong_name",
                        "type": "function",
                        "function": {
                            "name": "wrong_tool",
                            "arguments": "{\"path\":\"main.go\",\"line\":3,\"text\":\"hello\"}",
                        },
                    },
                    {
                        "id": "call_bad_arguments",
                        "type": "function",
                        "function": {
                            "name": "edit_file",
                            "arguments": "{\"path\":\"main.go\",\"line\":true}",
                        },
                    },
                    {
                        "id": "call_malformed_json",
                        "type": "function",
                        "function": {
                            "name": "edit_file",
                            "arguments": "{",
                        },
                    },
                ],
            }
        else:
            message = {"role": "assistant", "content": "I would edit the file."}
        self.send_json({"choices": [{"message": message}], "usage": {"completion_tokens": 8}})

    def send_json(self, payload):
        data = json.dumps(payload).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
Path(port_file).write_text(str(server.server_port), encoding="utf-8")
server.serve_forever()
PY
  PIDS+=("$!")
  for _ in $(seq 1 100); do
    [[ -s "$port_file" ]] && break
    sleep 0.05
  done
  test -s "$port_file"
  STARTED_PORT="$(cat "$port_file")"
}

run_probe() {
  local expected="$1" output="$2"
  shift 2
  set +e
  "$ROOT/scripts/probe-local.sh" "$@" --json >"$output"
  local actual=$?
  set -e
  if [[ "$actual" -ne "$expected" ]]; then
    echo "FAIL: expected exit $expected, got $actual for $*" >&2
    cat "$output" >&2
    exit 1
  fi
}

STARTED_PORT=""
start_fixture tool
TOOL_PORT="$STARTED_PORT"
start_fixture prose
PROSE_PORT="$STARTED_PORT"
start_fixture invalid
INVALID_PORT="$STARTED_PORT"
run_probe 0 "$TMP/tool.json" --base-url "http://127.0.0.1:$TOOL_PORT/v1" --timeout 2
run_probe 1 "$TMP/prose.json" --base-url "http://127.0.0.1:$PROSE_PORT/v1" --timeout 2
run_probe 1 "$TMP/invalid.json" --base-url "http://127.0.0.1:$INVALID_PORT/v1" --timeout 2

UNUSED_PORT="$(python3 - <<'PY'
import socket
s = socket.socket()
s.bind(("127.0.0.1", 0))
print(s.getsockname()[1])
s.close()
PY
)"
run_probe 2 "$TMP/unreachable.json" --base-url "http://127.0.0.1:$UNUSED_PORT/v1" --timeout 0.2

python3 - "$TMP/tool.json" "$TMP/prose.json" "$TMP/invalid.json" "$TMP/unreachable.json" <<'PY'
import json
import sys

tool, prose, invalid, unreachable = [json.load(open(path, encoding="utf-8"))["results"][0] for path in sys.argv[1:]]
assert tool["can_tool_call"] is True
assert tool["tool_names"] == ["edit_file"]
assert tool["rejected_tool_names"] == []
assert prose["reachable"] is True and prose["chat_ok"] is True
assert prose["can_tool_call"] is False and prose["text_reply_excerpt"]
assert invalid["reachable"] is True and invalid["chat_ok"] is True
assert invalid["can_tool_call"] is False
assert invalid["rejected_tool_names"] == ["wrong_tool", "edit_file", "edit_file"]
assert unreachable["reachable"] is False and unreachable["can_tool_call"] is False
assert all(not result.get("api_key_env_used") for result in (tool, prose, invalid, unreachable))
PY

echo "OK: probe-local distinguishes tool_call, prose-only, and unreachable endpoints"
