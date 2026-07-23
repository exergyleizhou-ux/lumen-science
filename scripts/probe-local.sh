#!/usr/bin/env bash
# Probe local OpenAI-compatible endpoints for real tool_calls, not chat alone.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export LUMEN_PROBE_ROOT="$ROOT"

exec python3 - "$@" <<'PY'
import argparse
import json
import os
import socket
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

LOCAL_IDS = ("lmstudio", "ollama", "vllm", "exo", "local-openai")
OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}))


def parse_args():
    parser = argparse.ArgumentParser(
        prog="probe-local.sh",
        description="Test whether a local OpenAI-compatible model emits a real tool_call.",
    )
    target = parser.add_mutually_exclusive_group()
    target.add_argument("--preset", choices=LOCAL_IDS, help="probe one built-in local preset")
    target.add_argument("--base-url", help="probe one custom OpenAI-compatible base URL")
    parser.add_argument("--model", help="served model id; default: first /models id or preset model")
    parser.add_argument("--api-key-env", help="read an optional API key from this environment variable")
    parser.add_argument("--timeout", type=float, default=30.0, help="per-request timeout in seconds")
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON")
    parser.add_argument("--list", action="store_true", help="list built-in local presets without probing")
    parser.add_argument(
        "--allow-remote",
        action="store_true",
        help="allow a non-loopback --base-url (off by default to avoid accidental key use)",
    )
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout must be greater than zero")
    return args, parser


def load_presets():
    root = Path(os.environ["LUMEN_PROBE_ROOT"])
    catalog = root / "agent/crates/codegen/xai-grok-models/default_models.json"
    try:
        data = json.loads(catalog.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise RuntimeError(f"load model catalog {catalog}: {exc}") from exc
    by_id = {item.get("id"): item for item in data.get("models", [])}
    missing = [model_id for model_id in LOCAL_IDS if model_id not in by_id]
    if missing:
        raise RuntimeError("model catalog is missing local presets: " + ", ".join(missing))
    return [by_id[model_id] for model_id in LOCAL_IDS]


def is_loopback(base_url):
    parsed = urllib.parse.urlparse(base_url)
    if parsed.scheme not in {"http", "https"} or not parsed.hostname:
        return False
    if parsed.hostname == "localhost":
        return True
    try:
        return __import__("ipaddress").ip_address(parsed.hostname).is_loopback
    except ValueError:
        return False


def key_for(entry, explicit_env):
    names = [explicit_env] if explicit_env else entry.get("env_key", [])
    if isinstance(names, str):
        names = [names]
    for name in names:
        if name and os.environ.get(name):
            return os.environ[name], name
    return "", None


def request_json(url, *, timeout, api_key="", payload=None):
    headers = {"Accept": "application/json"}
    data = None
    method = "GET"
    if api_key:
        headers["Authorization"] = "Bearer " + api_key
    if payload is not None:
        method = "POST"
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        # A local capability probe must never be redirected through HTTP(S)_PROXY.
        with OPENER.open(req, timeout=timeout) as response:
            raw = response.read(4 * 1024 * 1024)
            return json.loads(raw), response.status, "", True
    except urllib.error.HTTPError as exc:
        raw = exc.read(2048).decode("utf-8", "replace").strip().replace("\n", " ")
        detail = f"HTTP {exc.code}"
        if raw:
            detail += ": " + raw[:300]
        return None, exc.code, detail, True
    except (urllib.error.URLError, TimeoutError, socket.timeout, json.JSONDecodeError) as exc:
        return None, None, str(exc), False


def served_models(base_url, timeout, api_key):
    data, _, _, _ = request_json(base_url.rstrip("/") + "/models", timeout=timeout, api_key=api_key)
    if not isinstance(data, dict):
        return []
    models = []
    for item in data.get("data", []):
        if isinstance(item, dict) and isinstance(item.get("id"), str) and item["id"]:
            models.append(item["id"])
    return models


def is_expected_edit_call(function):
    if not isinstance(function, dict) or function.get("name") != "edit_file":
        return False
    arguments = function.get("arguments")
    if isinstance(arguments, str):
        try:
            arguments = json.loads(arguments)
        except json.JSONDecodeError:
            return False
    if not isinstance(arguments, dict):
        return False
    return (
        arguments.get("path") == "main.go"
        and type(arguments.get("line")) is int
        and arguments["line"] == 3
        and arguments.get("text") == "hello"
    )


def probe(entry, args):
    base_url = entry["base_url"].rstrip("/")
    api_key, key_env = key_for(entry, args.api_key_env)
    models = served_models(base_url, min(args.timeout, 5.0), api_key)
    model = args.model or (models[0] if models else entry.get("model", ""))
    payload = {
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "You are a coding agent. When asked to edit a file, call edit_file. Do not answer in prose.",
            },
            {
                "role": "user",
                "content": "In main.go, change line 3 to say hello. Use the edit_file tool.",
            },
        ],
        "tools": [
            {
                "type": "function",
                "function": {
                    "name": "edit_file",
                    "description": "Replace a line in a file.",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "line": {"type": "integer"},
                            "text": {"type": "string"},
                        },
                        "required": ["path", "line", "text"],
                    },
                },
            }
        ],
        "temperature": 0,
        # Thinking-capable local models can consume a few hundred reasoning
        # tokens before emitting the tool call. A tiny cap creates a false
        # prose/empty failure even when the endpoint supports tools.
        "max_tokens": 2048,
        "stream": False,
    }
    start = time.monotonic()
    data, status, error, connected = request_json(
        base_url + "/chat/completions",
        timeout=args.timeout,
        api_key=api_key,
        payload=payload,
    )
    elapsed_ms = round((time.monotonic() - start) * 1000)
    tool_calls = []
    rejected_tool_calls = []
    text_reply = ""
    usage = {}
    if isinstance(data, dict):
        choices = data.get("choices", [])
        if choices and isinstance(choices[0], dict):
            message = choices[0].get("message", {})
            if isinstance(message, dict):
                calls = message.get("tool_calls", [])
                if isinstance(calls, list):
                    for call in calls:
                        if not isinstance(call, dict):
                            continue
                        function = call.get("function", {})
                        if is_expected_edit_call(function):
                            tool_calls.append(function["name"])
                        elif isinstance(function, dict) and function.get("name"):
                            rejected_tool_calls.append(function["name"])
                content = message.get("content")
                if isinstance(content, str):
                    text_reply = content.strip()[:300]
        if isinstance(data.get("usage"), dict):
            usage = data["usage"]
    chat_ok = status is not None and 200 <= status < 300 and isinstance(data, dict)
    return {
        "preset": entry["id"],
        "base_url": base_url,
        "model": model,
        "served_models": models,
        "reachable": connected,
        "chat_ok": chat_ok,
        "can_tool_call": bool(tool_calls),
        "tool_names": tool_calls,
        "rejected_tool_names": rejected_tool_calls,
        "elapsed_ms": elapsed_ms,
        "usage": usage,
        "text_reply_excerpt": text_reply,
        "api_key_env_used": key_env,
        "error": error,
    }


def print_human(results):
    print("Endpoint\tModel\tReachable\tTool call\tLatency\tDetail")
    for result in results:
        detail = ""
        if result["can_tool_call"]:
            detail = ",".join(result["tool_names"])
        elif result["rejected_tool_names"]:
            detail = "invalid tool_calls: " + ",".join(result["rejected_tool_names"])
        elif result["chat_ok"]:
            detail = "prose only"
        else:
            detail = result["error"] or "invalid response"
        print(
            f'{result["preset"]}\t{result["model"] or "-"}\t'
            f'{"yes" if result["reachable"] else "no"}\t'
            f'{"YES" if result["can_tool_call"] else "NO"}\t'
            f'{result["elapsed_ms"]}ms\t{detail}'
        )


def main():
    args, parser = parse_args()
    try:
        presets = load_presets()
    except RuntimeError as exc:
        print(f"probe-local: {exc}", file=sys.stderr)
        return 2

    if args.list:
        listed = [
            {"preset": p["id"], "base_url": p["base_url"], "model": p.get("model", "")}
            for p in presets
        ]
        if args.json:
            print(json.dumps(listed, indent=2, ensure_ascii=False))
        else:
            for item in listed:
                print(f'{item["preset"]}\t{item["base_url"]}\t{item["model"]}')
        return 0

    if args.base_url:
        if not args.allow_remote and not is_loopback(args.base_url):
            parser.error("--base-url must be loopback; pass --allow-remote explicitly for remote endpoints")
        selected = [
            {
                "id": "custom",
                "base_url": args.base_url,
                "model": args.model or "",
                "env_key": args.api_key_env or [],
            }
        ]
    elif args.preset:
        selected = [p for p in presets if p["id"] == args.preset]
    else:
        selected = presets

    if not args.allow_remote:
        nonlocal_urls = [entry["base_url"] for entry in selected if not is_loopback(entry["base_url"])]
        if nonlocal_urls:
            parser.error(
                "selected endpoint is not loopback; pass --allow-remote explicitly: "
                + ", ".join(nonlocal_urls)
            )

    results = [probe(entry, args) for entry in selected]
    if args.json:
        print(json.dumps({"schema_version": 1, "results": results}, indent=2, ensure_ascii=False))
    else:
        print_human(results)

    if any(result["can_tool_call"] for result in results):
        return 0
    if any(result["reachable"] for result in results):
        return 1
    return 2


raise SystemExit(main())
PY
