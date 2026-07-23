#!/usr/bin/env python3
"""Deterministic localhost OpenAI-compatible fixture for FINAL-2.0 L4/L5.

The server records only structural request metadata. Authorization headers,
message text, tool arguments, and environment values are deliberately never
written to the event log.
"""

from __future__ import annotations

import argparse
import json
import shlex
import signal
import socket
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any


MODEL = "lumen-final20-fixture"


def tool_names(body: dict[str, Any]) -> list[str]:
    names: list[str] = []
    for tool in body.get("tools") or []:
        if not isinstance(tool, dict):
            continue
        function = tool.get("function")
        if isinstance(function, dict) and isinstance(function.get("name"), str):
            names.append(function["name"])
        elif isinstance(tool.get("name"), str):
            names.append(tool["name"])
    return names


def has_tool_result(body: dict[str, Any]) -> bool:
    return any(
        isinstance(message, dict) and message.get("role") == "tool"
        for message in body.get("messages") or []
    )


def user_text(body: dict[str, Any]) -> str:
    parts: list[str] = []
    for message in body.get("messages") or []:
        if not isinstance(message, dict) or message.get("role") != "user":
            continue
        content = message.get("content")
        if isinstance(content, str):
            parts.append(content)
        elif isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and isinstance(block.get("text"), str):
                    parts.append(block["text"])
    return "\n".join(parts)


class FixtureState:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.lock = threading.Lock()
        self.agent_attempt = 0
        self.event_log = Path(args.state_dir) / "events.jsonl"
        self.event_log.parent.mkdir(parents=True, exist_ok=True)
        self.event_log.write_text("", encoding="utf-8")

    def record(self, event: str, **fields: Any) -> None:
        row = {"event": event, **fields}
        encoded = json.dumps(row, sort_keys=True, separators=(",", ":"))
        with self.lock:
            with self.event_log.open("a", encoding="utf-8") as handle:
                handle.write(encoded + "\n")

    def next_agent_attempt(self) -> int:
        with self.lock:
            self.agent_attempt += 1
            return self.agent_attempt


def usage(prompt_tokens: int = 64, cached_tokens: int = 32) -> dict[str, Any]:
    completion_tokens = 8
    return {
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": prompt_tokens + completion_tokens,
        "prompt_tokens_details": {"cached_tokens": cached_tokens},
    }


def text_events(text: str, *, prompt_tokens: int = 64, cached_tokens: int = 32) -> list[str]:
    return [
        json.dumps(
            {
                "id": "chatcmpl-final20",
                "object": "chat.completion.chunk",
                "created": 0,
                "model": MODEL,
                "choices": [
                    {
                        "index": 0,
                        "delta": {"role": "assistant", "content": text},
                        "finish_reason": None,
                    }
                ],
            },
            separators=(",", ":"),
        ),
        json.dumps(
            {
                "id": "chatcmpl-final20",
                "object": "chat.completion.chunk",
                "created": 0,
                "model": MODEL,
                "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
                "usage": usage(prompt_tokens, cached_tokens),
            },
            separators=(",", ":"),
        ),
        "[DONE]",
    ]


def tool_events(
    name: str,
    arguments: dict[str, Any],
    *,
    call_id: str = "call_final20_1",
    prompt_tokens: int = 64,
    cached_tokens: int = 32,
) -> list[str]:
    return [
        json.dumps(
            {
                "id": "chatcmpl-final20",
                "object": "chat.completion.chunk",
                "created": 0,
                "model": MODEL,
                "choices": [
                    {
                        "index": 0,
                        "delta": {
                            "role": "assistant",
                            "content": None,
                            "tool_calls": [
                                {
                                    "index": 0,
                                    "id": call_id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": json.dumps(arguments, separators=(",", ":")),
                                    },
                                }
                            ],
                        },
                        "finish_reason": None,
                    }
                ],
            },
            separators=(",", ":"),
        ),
        json.dumps(
            {
                "id": "chatcmpl-final20",
                "object": "chat.completion.chunk",
                "created": 0,
                "model": MODEL,
                "choices": [
                    {"index": 0, "delta": {}, "finish_reason": "tool_calls"}
                ],
                "usage": usage(prompt_tokens, cached_tokens),
            },
            separators=(",", ":"),
        ),
        "[DONE]",
    ]


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "LumenFinal20Fixture/1"

    @property
    def fixture(self) -> FixtureState:
        return self.server.fixture  # type: ignore[attr-defined]

    def log_message(self, _format: str, *_args: Any) -> None:
        return

    def send_json(self, status: int, value: Any, **headers: str) -> None:
        payload = json.dumps(value, separators=(",", ":")).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Connection", "close")
        for name, value_ in headers.items():
            self.send_header(name, value_)
        self.end_headers()
        self.wfile.write(payload)
        self.close_connection = True

    def send_sse(self, events: list[str]) -> None:
        payload = "".join(f"data: {event}\n\n" for event in events).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Content-Length", str(len(payload)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(payload)
        self.close_connection = True

    def send_disconnect(self) -> None:
        payload = (
            "data: "
            + json.dumps(
                {
                    "id": "chatcmpl-cut",
                    "object": "chat.completion.chunk",
                    "created": 0,
                    "model": MODEL,
                    "choices": [
                        {
                            "index": 0,
                            "delta": {"content": "partial"},
                            "finish_reason": None,
                        }
                    ],
                },
                separators=(",", ":"),
            )
            + "\n\n"
        ).encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Transfer-Encoding", "chunked")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(f"{len(payload):X}\r\n".encode() + payload + b"\r\n")
        self.wfile.flush()
        self.close_connection = True
        try:
            self.connection.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.connection.close()

    def send_stall(self) -> None:
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.flush()
        time.sleep(self.fixture.args.stall_seconds)
        self.close_connection = True

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        if self.path == "/v1/models":
            self.send_json(
                200,
                {
                    "object": "list",
                    "data": [
                        {
                            "id": MODEL,
                            "object": "model",
                            "owned_by": "local-fixture",
                            "apiBackend": "chat_completions",
                            "_meta": {"agentType": "grok-build-plan"},
                        }
                    ],
                },
            )
        elif self.path == "/v1/settings":
            self.send_json(200, {"allow_access": True})
        elif self.path == "/v1/user":
            self.send_json(200, {"subscriptionTier": "pro"})
        else:
            self.send_json(404, {"error": {"message": "not found"}})

    def do_POST(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        if self.path != "/v1/chat/completions":
            self.send_json(404, {"error": {"message": "not found"}})
            return
        try:
            length = int(self.headers.get("Content-Length", "0"))
            if length < 0 or length > 8 * 1024 * 1024:
                raise ValueError("invalid Content-Length")
            body = json.loads(self.rfile.read(length) or b"{}")
        except (ValueError, json.JSONDecodeError):
            self.send_json(400, {"error": {"message": "invalid request"}})
            return

        names = tool_names(body) if isinstance(body, dict) else []
        is_agent = any(name in {"run_terminal_command", "read_file"} for name in names)
        is_compaction = isinstance(body, dict) and body.get("tool_choice") == "none"
        attempt = self.fixture.next_agent_attempt() if is_agent and not is_compaction else 0
        self.fixture.record(
            "request",
            path=self.path,
            agent=is_agent,
            compaction=is_compaction,
            attempt=attempt,
            message_count=len(body.get("messages") or []) if isinstance(body, dict) else 0,
            tool_count=len(names),
            has_tool_result=has_tool_result(body) if isinstance(body, dict) else False,
        )

        if not is_agent and not is_compaction:
            self.fixture.record("response", kind="aux_text", attempt=attempt)
            self.send_sse(text_events("FINAL20_AUX"))
            return

        scenario = self.fixture.args.scenario
        if scenario == "l5":
            self.handle_l5(body, is_compaction, attempt)
            return

        if attempt == 1 and scenario == "429":
            self.fixture.record("response", kind="fault_429", attempt=attempt)
            self.send_json(
                429,
                {"error": {"message": "fixture rate limit", "type": "rate_limit"}},
                **{"Retry-After": "0"},
            )
            return
        if attempt == 1 and scenario == "500":
            self.fixture.record("response", kind="fault_500", attempt=attempt)
            self.send_json(500, {"error": {"message": "fixture server error"}})
            return
        if attempt == 1 and scenario == "disconnect":
            self.fixture.record("response", kind="fault_disconnect", attempt=attempt)
            self.send_disconnect()
            return
        if attempt == 1 and scenario in {"timeout", "cancel"}:
            self.fixture.record("response", kind=f"fault_{scenario}", attempt=attempt)
            self.send_stall()
            return

        if has_tool_result(body):
            self.fixture.record("response", kind="final", attempt=attempt)
            self.send_sse(text_events("L4_RECOVERED", cached_tokens=64))
            return

        marker = Path(self.fixture.args.marker_file)
        command = f"printf 'effect\\n' >> {shlex.quote(str(marker))}"
        self.fixture.record("response", kind="tool_call", attempt=attempt)
        self.send_sse(
            tool_events(
                "run_terminal_command",
                {"command": command, "description": "record one L4 fixture effect"},
                cached_tokens=64,
            )
        )

    def handle_l5(self, body: dict[str, Any], is_compaction: bool, attempt: int) -> None:
        if is_compaction:
            summary = (
                "# L5 deterministic compaction summary\n\n"
                "The session created a durable marker with a terminal tool. "
                "Resume must read that exact marker, preserve the original goal, "
                "and continue without repeating any write side effect. "
                "This paragraph is intentionally substantial so the compaction "
                "validator does not accept an empty or degenerate summary. " * 4
            )
            self.fixture.record("response", kind="compaction_summary", attempt=attempt)
            self.send_sse(text_events(summary, prompt_tokens=512, cached_tokens=256))
            return

        text = user_text(body)
        if has_tool_result(body):
            self.fixture.record("response", kind="l5_final", attempt=attempt)
            self.send_sse(
                text_events("L5_TOOL_PHASE_DONE", prompt_tokens=768, cached_tokens=512)
            )
            return

        if "L5_RESUME_CHECK" in text or "L5_SOAK_TURN" in text:
            self.fixture.record("response", kind="l5_read_tool", attempt=attempt)
            self.send_sse(
                tool_events(
                    "read_file",
                    {"target_file": self.fixture.args.marker_file},
                    call_id=f"call_l5_read_{attempt}",
                    prompt_tokens=768,
                    cached_tokens=512,
                )
            )
            return

        if "L5_SEED" in text:
            if Path(self.fixture.args.marker_file).is_file():
                self.fixture.record("response", kind="l5_seed_complete", attempt=attempt)
                self.send_sse(
                    text_events("L5_SEED_COMPLETE", prompt_tokens=768, cached_tokens=512)
                )
                return
            command = (
                f"printf '%s\\n' {shlex.quote(self.fixture.args.session_token)} "
                f"> {shlex.quote(self.fixture.args.marker_file)}"
            )
            self.fixture.record("response", kind="l5_write_tool", attempt=attempt)
            self.send_sse(
                tool_events(
                    "run_terminal_command",
                    {"command": command, "description": "write the L5 session marker"},
                    call_id=f"call_l5_write_{attempt}",
                    prompt_tokens=self.fixture.args.compaction_prompt_tokens,
                    cached_tokens=512,
                )
            )
            return

        self.fixture.record("response", kind="l5_continue", attempt=attempt)
        self.send_sse(text_events("L5_CONTINUED", prompt_tokens=768, cached_tokens=512))


class FixtureServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, address: tuple[str, int], fixture: FixtureState) -> None:
        self.fixture = fixture
        super().__init__(address, Handler)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--scenario",
        required=True,
        choices=["429", "500", "disconnect", "timeout", "cancel", "l5"],
    )
    parser.add_argument("--state-dir", required=True)
    parser.add_argument("--port-file", required=True)
    parser.add_argument("--marker-file", required=True)
    parser.add_argument("--session-token", default="L5_FIXTURE_TOKEN")
    parser.add_argument("--stall-seconds", type=float, default=60.0)
    parser.add_argument("--compaction-prompt-tokens", type=int, default=30_000)
    args = parser.parse_args()
    if args.stall_seconds <= 0:
        parser.error("--stall-seconds must be > 0")
    if args.compaction_prompt_tokens < 1:
        parser.error("--compaction-prompt-tokens must be positive")
    return args


def main() -> int:
    args = parse_args()
    fixture = FixtureState(args)
    server = FixtureServer(("127.0.0.1", 0), fixture)
    port_file = Path(args.port_file)
    port_file.parent.mkdir(parents=True, exist_ok=True)
    port_file.write_text(str(server.server_address[1]) + "\n", encoding="utf-8")
    fixture.record("ready", port=server.server_address[1], scenario=args.scenario)

    def stop(_signum: int, _frame: Any) -> None:
        raise KeyboardInterrupt

    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        server.serve_forever(poll_interval=0.05)
    except KeyboardInterrupt:
        pass
    finally:
        server.server_close()
        fixture.record("stopped", scenario=args.scenario)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
