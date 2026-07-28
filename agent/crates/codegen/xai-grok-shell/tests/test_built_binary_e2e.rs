//! Built-binary end-to-end tests for the grok (xai-grok-pager) binary.
//!
//! These tests verify that the built grok binary works end-to-end against a mock
//! inference server. They catch dynamic linking failures (libgit2/OpenSSL),
//! session initialization crashes, and protocol regressions.
//!
//! The tests exercise:
//! - **Smoke** (`grok --version`): binary loads without crashing
//! - **ACP stdio** (`grok agent stdio`): full protocol lifecycle via ClientSideConnection
//!
//! Tests are `#[ignore]`d by default — they require a pre-built binary.
//!
//! Run locally (auto-builds the binary if not already present):
//! ```bash
//! cargo test -p xai-grok-shell --test test_built_binary_e2e -- --ignored
//! ```
//!
//! In CI, set `GROK_BINARY` to point at the release artifact:
//! ```bash
//! GROK_BINARY=./artifacts/grok-0.1.159-linux-x86_64 \
//!   cargo test -p xai-grok-shell --test test_built_binary_e2e -- --ignored
//! ```

use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;

use agent_client_protocol as acp;
use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256};
use xai_grok_test_support::acp_client::PermissionResponse;
use xai_grok_test_support::env::test_env_cmd_tokio;
use xai_grok_test_support::*;

/// Run an async test body inside a `LocalSet` (required by ACP's `!Send` futures).
/// Eliminates the `let local = LocalSet::new(); local.run_until(async { ... }).await`
/// boilerplate from every stdio test.
async fn with_local_set<F, Fut>(f: F)
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = ()>,
{
    tokio::task::LocalSet::new().run_until(f()).await;
}

struct LocalSshdFixture {
    _root: tempfile::TempDir,
    child: Child,
    port: u16,
    identity_file: PathBuf,
    known_hosts_file: PathBuf,
    ssh_config_file: PathBuf,
    host_key_sha256: String,
}

impl LocalSshdFixture {
    fn start(workspace: &Path) -> Self {
        use std::net::TcpListener;
        let root = tempfile::tempdir_in(workspace).expect("create sshd fixture directory");
        let port = TcpListener::bind("127.0.0.1:0")
            .expect("reserve fixture port")
            .local_addr()
            .unwrap()
            .port();
        let host_key = root.path().join("host_ed25519");
        let identity_file = root.path().join("client_ed25519");
        for key in [&host_key, &identity_file] {
            assert!(
                Command::new("/usr/bin/ssh-keygen")
                    .args(["-q", "-t", "ed25519", "-N", "", "-f"])
                    .arg(key)
                    .status()
                    .expect("run ssh-keygen")
                    .success()
            );
        }
        let authorized_keys = root.path().join("authorized_keys");
        std::fs::copy(identity_file.with_extension("pub"), &authorized_keys)
            .expect("install fixture public key");
        let host_public =
            std::fs::read_to_string(host_key.with_extension("pub")).expect("read host public key");
        let parts: Vec<_> = host_public.split_whitespace().collect();
        let key_bytes = base64::engine::general_purpose::STANDARD
            .decode(parts[1])
            .expect("decode host key");
        let host_key_sha256 = format!("{:x}", Sha256::digest(key_bytes));
        let known_hosts_file = root.path().join("known_hosts");
        std::fs::write(
            &known_hosts_file,
            format!(
                "[fixture.lumen.test]:{port} {} {}\n[127.0.0.1]:{port} {} {}\n",
                parts[0], parts[1], parts[0], parts[1]
            ),
        )
        .expect("write fixture known hosts");
        let config = root.path().join("sshd_config");
        let username = std::env::var("USER").expect("fixture user");
        std::fs::write(&config, format!("Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nAuthorizedKeysFile {}\nPidFile {}\nUsePAM no\nPasswordAuthentication no\nChallengeResponseAuthentication no\nStrictModes no\nAllowUsers {username}\nSubsystem sftp internal-sftp\n", host_key.display(), authorized_keys.display(), root.path().join("sshd.pid").display()))
            .expect("write sshd config");
        assert!(
            Command::new("/usr/sbin/sshd")
                .args(["-t", "-f"])
                .arg(&config)
                .status()
                .expect("validate sshd config")
                .success()
        );
        let child = Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&config)
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("start fixture sshd");
        let ssh_config_file = root.path().join("ssh_config");
        std::fs::write(&ssh_config_file, format!("Host fixture.lumen.test\n  HostName 127.0.0.1\n  Port {port}\n  User {username}\n  IdentityFile {}\n  UserKnownHostsFile {}\n  StrictHostKeyChecking yes\n  BatchMode yes\n", identity_file.display(), known_hosts_file.display()))
            .expect("write fixture client config");
        for _ in 0..50 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Self {
            _root: root,
            child,
            port,
            identity_file,
            known_hosts_file,
            ssh_config_file,
            host_key_sha256,
        }
    }
}

impl Drop for LocalSshdFixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Start a mock server with one model named `model` on the given API backend.
async fn single_model_server(model: &str, backend: &str) -> MockInferenceServer {
    MockInferenceServer::start_with_models(vec![
        MockModelEntry::new(model).with_api_backend(backend),
    ])
    .await
    .expect("start mock server")
}

async fn grok_build_server() -> MockInferenceServer {
    MockInferenceServer::start_with_models(vec![
        MockModelEntry::with_agent_type("grok-4.5", "grok-build")
            .with_api_backend("responses")
            .with_supports_backend_search(true),
    ])
    .await
    .expect("start mock server")
}

/// Parse a headless run's stdout as a single JSON object.
fn parse_stdout_json(result: &HeadlessResult) -> serde_json::Value {
    serde_json::from_str(result.stdout.trim())
        .unwrap_or_else(|e| panic!("stdout not valid JSON: {e}\n{}", result.stdout))
}

fn request_tool_name(tool: &Value) -> Option<&str> {
    tool.pointer("/function/name")
        .or_else(|| tool.get("name"))
        .and_then(Value::as_str)
        .or_else(|| {
            tool.get("type")
                .and_then(Value::as_str)
                .and_then(|kind| kind.starts_with("web_search").then_some("web_search"))
        })
}

fn inference_request(server: &MockInferenceServer) -> Value {
    server
        .request_bodies()
        .into_iter()
        .find(|body| {
            body.get("tools")
                .and_then(Value::as_array)
                .is_some_and(|tools| {
                    !tools.is_empty()
                        && !tools
                            .iter()
                            .any(|tool| request_tool_name(tool) == Some("session_title"))
                })
        })
        .expect("mock server should receive a main inference request with tools")
}

fn inference_tool_names(server: &MockInferenceServer) -> Vec<String> {
    let request = inference_request(server);
    request["tools"]
        .as_array()
        .expect("inference request tools should be an array")
        .iter()
        .filter_map(request_tool_name)
        .map(str::to_owned)
        .collect()
}

async fn run_headless_with_env(
    server: &MockInferenceServer,
    args: &[&str],
    cwd: &Path,
    env: &[(&str, &str)],
) -> HeadlessResult {
    let home = tempfile::TempDir::new().expect("create temp home");
    let mut cmd = tokio::process::Command::new(grok_binary());
    cmd.args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .envs(env.iter().copied());
    test_env_cmd_tokio(&mut cmd, &server.url(), home.path());
    run_headless_with_cmd(cmd).await
}

// ============================================================================
// Smoke tests
// ============================================================================

/// Smoke test: the binary loads and exits without crashing.
/// This does NOT require the mock server — it's the absolute minimum bar.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_version_exits_zero() {
    let binary = grok_binary();
    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", binary.display()));

    assert!(
        output.status.success(),
        "grok --version failed (exit {:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Verify the crash handler installs without interfering with normal startup.
/// Exercises install() (sigaction, sigaltstack, mmap, ucontext struct layouts)
/// on every platform the binary is built for.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_version_with_crash_handler_exits_zero() {
    let binary = grok_binary();
    let output = Command::new(&binary)
        .arg("--version")
        .env("GROK_CRASH_HANDLER", "1")
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", binary.display()));

    assert!(
        output.status.success(),
        "grok --version with GROK_CRASH_HANDLER=1 failed (exit {:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// THE critical test. Exercises the full session lifecycle in a git repo:
/// binary start → agent init → libgit2 init → fs watchers → session create →
/// model resolve → inference request to mock server → SSE parse → response render → exit.
///
/// This catches the recurring libgit2/OpenSSL dynamic linking bug that has
/// caused ~5 broken releases.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_session_in_git_repo() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = git_workdir();
    let result = run_headless(&server, &["-p", "say hello", "--yolo"], workdir.path()).await;

    assert_headless_success(&result, "grok -p in git repo", Some(&server));
    assert_no_crashes(&result.stderr);
    assert!(
        server.request_count() > 0,
        "mock server received no inference requests\nrequest log:\n{}",
        server.request_log_summary()
    );
    assert!(
        server.has_chat_completion_request() || server.has_responses_request(),
        "headless mode should hit /v1/chat/completions or /v1/responses\nrequest log:\n{}",
        server.request_log_summary()
    );
}

/// Verify grok works in a non-git directory (exercises the fallback codepath
/// where libgit2 discovers there's no repo instead of initializing one).
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_session_in_non_git_dir() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = tempfile::tempdir().unwrap();
    std::fs::write(workdir.path().join("test.txt"), "test\n").unwrap();

    let result = run_headless(&server, &["-p", "say hello", "--yolo"], workdir.path()).await;

    assert_headless_success(&result, "grok -p in non-git dir", Some(&server));
    assert_no_crashes(&result.stderr);
}

#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_tools_allowlist_keeps_enabled_web_tools() {
    let server = grok_build_server().await;
    server.preset_allow_access();
    let workdir = git_workdir();

    let result = run_headless_with_env(
        &server,
        &[
            "-p",
            "say hello",
            "--yolo",
            "--tools",
            "read_file,grep,list_dir,web_search,web_fetch",
        ],
        workdir.path(),
        &[("GROK_WEB_FETCH", "1")],
    )
    .await;

    assert_headless_success(&result, "grok -p --tools with web tools", Some(&server));
    assert_no_crashes(&result.stderr);
    let names = inference_tool_names(&server);
    for expected in ["read_file", "grep", "list_dir", "web_search", "web_fetch"] {
        assert!(names.iter().any(|name| name == expected), "got: {names:?}");
    }
    for excluded in ["run_terminal_command", "search_replace"] {
        assert!(!names.iter().any(|name| name == excluded), "got: {names:?}");
    }
    let request = inference_request(&server);
    let tools = request["tools"]
        .as_array()
        .expect("inference request tools should be an array");
    assert!(
        tools.iter().any(|tool| {
            tool.get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind.starts_with("web_search"))
        }),
        "backend-capable model should receive hosted web search: {tools:?}"
    );
    assert!(
        !tools.iter().any(|tool| {
            tool.get("type").and_then(Value::as_str) == Some("function")
                && tool.get("name").and_then(Value::as_str) == Some("web_search")
        }),
        "backend-capable model must not receive function web_search: {tools:?}"
    );
    assert!(
        tools.iter().any(|tool| {
            tool.get("type").and_then(Value::as_str) == Some("function")
                && tool.get("name").and_then(Value::as_str) == Some("web_fetch")
        }),
        "web_fetch should remain a function tool: {tools:?}"
    );
}

#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_tools_allowlist_does_not_fail_open_for_disabled_web_fetch() {
    let server = grok_build_server().await;
    server.set_settings(serde_json::json!({
        "allow_access": true,
        "web_fetch_enabled": false,
    }));
    let workdir = git_workdir();

    let result = run_headless_with_env(
        &server,
        &[
            "-p",
            "say hello",
            "--yolo",
            "--tools",
            "read_file,web_fetch",
        ],
        workdir.path(),
        &[("GROK_WEB_FETCH", "0")],
    )
    .await;

    assert_headless_success(
        &result,
        "grok -p --tools with disabled web_fetch",
        Some(&server),
    );
    assert_no_crashes(&result.stderr);
    let names = inference_tool_names(&server);
    assert!(
        names.iter().any(|name| name == "read_file"),
        "got: {names:?}"
    );
    for excluded in ["web_fetch", "run_terminal_command", "search_replace"] {
        assert!(!names.iter().any(|name| name == excluded), "got: {names:?}");
    }
}

#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_terminal_only_allowlist_is_foreground_only() {
    let server = grok_build_server().await;
    let workdir = git_workdir();

    let result = run_headless(
        &server,
        &["-p", "say hello", "--yolo", "--tools", "run_terminal_cmd"],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "grok -p --tools run_terminal_cmd", Some(&server));
    assert_no_crashes(&result.stderr);
    let request = inference_request(&server);
    let terminal = request["tools"]
        .as_array()
        .expect("inference request tools should be an array")
        .iter()
        .find(|tool| request_tool_name(tool) == Some("run_terminal_command"))
        .expect("terminal tool should remain in the allowlist");
    let properties = terminal
        .pointer("/function/parameters/properties")
        .or_else(|| terminal.pointer("/parameters/properties"))
        .and_then(Value::as_object)
        .expect("terminal tool should have an input schema");
    assert!(
        !properties.contains_key("is_background"),
        "foreground-only terminal schema must omit is_background: {terminal}"
    );
}

/// Free-usage paywall in headless mode: 429s whose flat body carries the
/// `subscription:free-usage-exhausted` well-known code must surface the
/// pager's free-usage message instead of the generic rate-limit one. The
/// code reaches the pager embedded in the flattened error text (no
/// structured plumbing), so this exercises the whole detection path.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_free_usage_exhausted_prints_paywall_message() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let free_usage = || {
        ScriptedResponse::json(
            429,
            serde_json::json!({
                "code": "subscription:free-usage-exhausted",
                "error": "You have used all your free usage."
            }),
        )
    };
    // The binary may target either backend, generic-429 handling retries
    // once before going fatal, and the background session-title generation
    // may consume a script on the same path — queue three per path (any
    // leftovers are simply unused).
    for path in ["/v1/chat/completions", "/v1/responses"] {
        for _ in 0..3 {
            server.enqueue_response(path, free_usage());
        }
    }
    let workdir = git_workdir();

    let result = run_headless(&server, &["-p", "say hello", "--yolo"], workdir.path()).await;

    assert!(
        !result.timed_out && !result.status.success(),
        "a free-usage-exhausted turn must finish and exit non-zero\nstderr tail:\n{}",
        stderr_tail(&result.stderr, 500)
    );
    assert_no_crashes(&result.stderr);
    let combined = format!("{}\n{}", result.stdout, result.stderr);
    assert!(
        combined.contains("reached your free Grok Build usage limit"),
        "expected the free-usage paywall message\nstdout:\n{}\nstderr tail:\n{}",
        result.stdout,
        stderr_tail(&result.stderr, 1000)
    );
    assert!(
        !combined.contains("hit the rate limit for your plan"),
        "generic rate-limit message must be replaced by the paywall text"
    );
}

/// Verify the streaming JSON output format works end-to-end.
/// This is the format used by programmatic integrations (`--output-format streaming-json`).
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_streaming_json_output() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "say hello",
            "--yolo",
            "--output-format",
            "streaming-json",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(
        &result,
        "grok -p --output-format streaming-json",
        Some(&server),
    );
    assert_no_crashes(&result.stderr);

    let events: Vec<serde_json::Value> = result
        .stdout
        .lines()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("invalid streaming-json line `{line}`: {e}"))
        })
        .collect();
    assert!(
        !events.is_empty(),
        "streaming-json stdout should not be empty"
    );
    assert!(
        !events
            .iter()
            .any(|event| event.get("type").and_then(serde_json::Value::as_str) == Some("error")),
        "streaming-json emitted an error event: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|event| event.get("type").and_then(serde_json::Value::as_str) == Some("text")),
        "streaming-json output should include at least one text event: {:?}",
        events
    );
    assert_eq!(
        events
            .last()
            .and_then(|event| event.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("end"),
        "streaming-json output should end with an `end` event: {:?}",
        events
    );
}

#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_json_reports_server_cost() {
    use xai_grok_test_support::scripted::SseEvent;

    let server = single_model_server("grok-4.5", "chat_completions").await;
    let chunk = |body: serde_json::Value| SseEvent::data(body.to_string());
    server.enqueue_response(
        "/v1/chat/completions",
        xai_grok_test_support::scripted::ScriptedResponse::sse(vec![
            chunk(serde_json::json!({
                "id": "chatcmpl-cost", "object": "chat.completion.chunk", "created": 0,
                "model": "grok-4.5",
                "choices": [{ "index": 0, "delta": { "content": "4" }, "finish_reason": "stop" }]
            })),
            chunk(serde_json::json!({
                "id": "chatcmpl-cost", "object": "chat.completion.chunk", "created": 0,
                "model": "grok-4.5", "choices": [],
                "usage": {
                    "prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                    "cost_in_usd_ticks": 1_234_500_000_i64
                }
            })),
            SseEvent::data("[DONE]"),
        ]),
    );

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "what is 2+2",
            "--yolo",
            "--model",
            "grok-4.5",
            "--max-turns",
            "1",
            "--output-format",
            "json",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "grok -p (scripted cost)", Some(&server));
    let output = parse_stdout_json(&result);
    assert_eq!(output["total_cost_usd"], 0.12345);
    assert_eq!(output["total_cost_usd_ticks"], 1_234_500_000_i64);
    assert!(output.get("cost_is_partial").is_none());
    assert!(output["usage"]["input_tokens"].as_u64().unwrap() >= 10);
    assert_eq!(output["num_turns"], 1);
    let (_, model) = output["modelUsage"]
        .as_object()
        .expect("modelUsage")
        .iter()
        .next()
        .expect("one model");
    assert_eq!(model["costUSD"], 0.12345);
    assert!(model["modelCalls"].as_u64().unwrap() >= 1);
}

#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_json_reports_usage_on_max_turns() {
    let server = single_model_server("grok-4.5", "chat_completions").await;
    server.enqueue_response(
        "/v1/chat/completions",
        xai_grok_test_support::scripted::ScriptedResponse::sse(
            xai_grok_test_support::sse::chat_completions_reasoning_then_tool_call_events(
                "let me look",
                "call-1",
                "read_file",
                r#"{"path":"README.md"}"#,
                "grok-4.5",
            ),
        ),
    );

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "read the readme",
            "--yolo",
            "--model",
            "grok-4.5",
            "--max-turns",
            "1",
            "--output-format",
            "json",
        ],
        workdir.path(),
    )
    .await;

    assert!(!result.status.success());
    let output = parse_stdout_json(&result);
    assert!(output["usage"]["input_tokens"].as_u64().unwrap() >= 10);
    assert!(output["num_turns"].as_u64().unwrap() >= 1);
}

#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_streaming_json_usage() {
    let server = single_model_server("grok-4.5", "chat_completions").await;
    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "say hello",
            "--yolo",
            "--model",
            "grok-4.5",
            "--output-format",
            "streaming-json",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "streaming-json usage", Some(&server));
    let events: Vec<serde_json::Value> = result
        .stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid NDJSON line"))
        .collect();
    let end = events.last().unwrap();
    assert_eq!(end["type"], "end");
    assert!(end["usage"]["input_tokens"].as_u64().unwrap() >= 10);
    assert!(end["num_turns"].as_u64().unwrap() >= 1);
}

/// Chat Completions backend: the schema is enforced natively via
/// `response_format`, and the model's final JSON answer surfaces as
/// `structuredOutput`. The StructuredOutput tool is NOT used.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn headless_json_schema_chat_completions_uses_response_format() {
    let server = single_model_server("grok-4.5", "chat_completions").await;
    server.set_response(r#"{"name":"Alice","age":30}"#);

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "extract name and age",
            "--yolo",
            "--model",
            "grok-4.5",
            "--json-schema",
            r#"{"type":"object","properties":{"name":{"type":"string"},"age":{"type":"integer"}},"required":["name","age"],"additionalProperties":false}"#,
            "--max-turns",
            "1",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(
        &result,
        "grok -p --json-schema (chat_completions)",
        Some(&server),
    );
    assert_no_crashes(&result.stderr);

    let output = parse_stdout_json(&result);
    assert_eq!(output["structuredOutput"]["name"], "Alice");
    assert_eq!(output["structuredOutput"]["age"], 30);
    assert!(output.get("structuredOutputError").is_none());

    // Native path: the schema rides response_format; the StructuredOutput tool
    // is never advertised.
    let response_format_on_wire = server.requests().iter().any(|r| {
        r.body.as_ref().is_some_and(|body| {
            body.pointer("/response_format/type")
                .and_then(|v| v.as_str())
                == Some("json_schema")
        })
    });
    assert!(
        response_format_on_wire,
        "response_format json_schema must reach the wire\n{}",
        server.request_log_summary()
    );
    assert!(
        !any_request_advertises_structured_output_tool(&server),
        "native path must NOT advertise the StructuredOutput tool\n{}",
        server.request_log_summary()
    );
}

/// Responses backend: native schema rides `text.format` (not the tool), and the
/// final JSON answer surfaces as `structuredOutput`.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn headless_json_schema_responses_uses_text_format() {
    let server = single_model_server("grok-4.5", "responses").await;
    server.set_response(r#"{"name":"Alice","age":30}"#);

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "extract name and age",
            "--yolo",
            "--model",
            "grok-4.5",
            "--json-schema",
            NAME_AGE_SCHEMA,
            "--max-turns",
            "1",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "grok -p --json-schema (responses)", Some(&server));
    assert_no_crashes(&result.stderr);

    let output = parse_stdout_json(&result);
    assert_eq!(output["structuredOutput"]["name"], "Alice");
    assert_eq!(output["structuredOutput"]["age"], 30);
    assert!(output.get("structuredOutputError").is_none());

    let text_format_on_wire = server.requests().iter().any(|r| {
        r.body.as_ref().is_some_and(|body| {
            body.pointer("/text/format/type").and_then(|v| v.as_str()) == Some("json_schema")
        })
    });
    assert!(
        text_format_on_wire,
        "text.format json_schema must reach the wire\n{}",
        server.request_log_summary()
    );
    assert!(
        !any_request_advertises_structured_output_tool(&server),
        "native path must NOT advertise the StructuredOutput tool\n{}",
        server.request_log_summary()
    );
}

/// Messages backend (Anthropic-style) can't constrain output natively, so the
/// model returns its answer by calling the synthetic `StructuredOutput` tool.
/// Verifies the tool reaches the wire and its validated args surface as
/// `structuredOutput`.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn headless_json_schema_messages_backend_uses_structured_output_tool() {
    let server = single_model_server("messages-compatible-model", "messages").await;
    server.enqueue_response(
        "/v1/messages",
        structured_output_tool_call_sse("messages-compatible-model", r#"{"name":"Bob","age":42}"#),
    );

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "extract name and age",
            "--yolo",
            "--model",
            "messages-compatible-model",
            "--json-schema",
            r#"{"type":"object","properties":{"name":{"type":"string"},"age":{"type":"integer"}},"required":["name","age"],"additionalProperties":false}"#,
            "--max-turns",
            "2",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "grok -p --json-schema (messages)", Some(&server));
    assert_no_crashes(&result.stderr);

    let output = parse_stdout_json(&result);
    assert_eq!(output["structuredOutput"]["name"], "Bob");
    assert_eq!(output["structuredOutput"]["age"], 42);
    assert!(output.get("structuredOutputError").is_none());

    // The schema is advertised as the StructuredOutput tool, not response_format.
    assert!(
        any_request_advertises_structured_output_tool(&server),
        "StructuredOutput tool must reach the wire\n{}",
        server.request_log_summary()
    );
}

/// Whether any request advertised a tool named `StructuredOutput` in `tools[]`.
fn any_request_advertises_structured_output_tool(server: &MockInferenceServer) -> bool {
    server.requests().iter().any(|r| {
        r.body.as_ref().is_some_and(|body| {
            body.pointer("/tools")
                .and_then(|t| t.as_array())
                .is_some_and(|tools| {
                    tools
                        .iter()
                        .any(|t| t.get("name").and_then(|n| n.as_str()) == Some("StructuredOutput"))
                })
        })
    })
}

/// Anthropic Messages API SSE that streams a single `StructuredOutput` tool call
/// whose input is `args_json`.
fn structured_output_tool_call_sse(model: &str, args_json: &str) -> ScriptedResponse {
    use serde_json::json;
    ScriptedResponse::sse(vec![
        SseEvent::data(
            json!({"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[],"model":model,"stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}).to_string(),
        ),
        SseEvent::data(
            json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"StructuredOutput","input":{}}}).to_string(),
        ),
        SseEvent::data(
            json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":args_json}}).to_string(),
        ),
        SseEvent::data(json!({"type":"content_block_stop","index":0}).to_string()),
        SseEvent::data(
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":5,"input_tokens":10}}).to_string(),
        ),
        SseEvent::data(json!({"type":"message_stop"}).to_string()),
    ])
}

const NAME_AGE_SCHEMA: &str = r#"{"type":"object","properties":{"name":{"type":"string"},"age":{"type":"integer"}},"required":["name","age"],"additionalProperties":false}"#;

/// Messages backend, model ignores the StructuredOutput tool and answers as
/// prose: the turn-end fallback still validates the text against the schema and
/// surfaces `structuredOutput` (closes the "unvalidated fallback" gap).
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn headless_json_schema_messages_validates_text_when_tool_not_called() {
    let server = single_model_server("messages-compatible-model", "messages").await;
    server.set_response(r#"{"name":"Cara","age":7}"#);

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "extract name and age",
            "--yolo",
            "--model",
            "messages-compatible-model",
            "--json-schema",
            NAME_AGE_SCHEMA,
            "--max-turns",
            "1",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(
        &result,
        "grok -p --json-schema (messages, text)",
        Some(&server),
    );
    assert_no_crashes(&result.stderr);

    let output = parse_stdout_json(&result);
    assert_eq!(output["structuredOutput"]["name"], "Cara");
    assert_eq!(output["structuredOutput"]["age"], 7);
    assert!(output.get("structuredOutputError").is_none());
}

/// Messages backend, first StructuredOutput call violates the schema (no `age`):
/// the agent feeds the error back and the model's retry conforms. Exercises the
/// validation + bounded-retry path.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn headless_json_schema_messages_retries_on_schema_violation() {
    let server = single_model_server("messages-compatible-model", "messages").await;
    server.enqueue_response(
        "/v1/messages",
        structured_output_tool_call_sse("messages-compatible-model", r#"{"name":"Dan"}"#),
    );
    server.enqueue_response(
        "/v1/messages",
        structured_output_tool_call_sse("messages-compatible-model", r#"{"name":"Dan","age":9}"#),
    );

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "extract name and age",
            "--yolo",
            "--model",
            "messages-compatible-model",
            "--json-schema",
            NAME_AGE_SCHEMA,
            "--max-turns",
            "3",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(
        &result,
        "grok -p --json-schema (messages, retry)",
        Some(&server),
    );
    assert_no_crashes(&result.stderr);

    let output = parse_stdout_json(&result);
    assert_eq!(output["structuredOutput"]["name"], "Dan");
    assert_eq!(output["structuredOutput"]["age"], 9);
    assert!(output.get("structuredOutputError").is_none());
}

/// An invalid `--json-schema` (valid JSON object, but fails schema compilation)
/// disables both structured-output paths and surfaces the compile error.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn invalid_json_schema_disables_structured_output_and_surfaces_error() {
    let server = single_model_server("grok-4.5", "chat_completions").await;
    server.set_response(r#"{"name":"Alice","age":30}"#);

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "extract name and age",
            "--yolo",
            "--model",
            "grok-4.5",
            // Valid JSON object, but `pattern` is an invalid regex → schema
            // compilation (`jsonschema::validator_for`) fails.
            "--json-schema",
            r#"{"type":"string","pattern":"["}"#,
            "--max-turns",
            "1",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(
        &result,
        "grok -p --json-schema (invalid schema)",
        Some(&server),
    );
    assert_no_crashes(&result.stderr);

    let output = parse_stdout_json(&result);
    assert!(
        output["structuredOutput"].is_null(),
        "invalid schema must not produce a value\n{}",
        result.stdout
    );
    assert!(
        output["structuredOutputError"]
            .as_str()
            .is_some_and(|e| e.contains("invalid output schema")),
        "invalid schema must surface structuredOutputError\n{}",
        result.stdout
    );

    // Structured output disabled: no native response_format, no tool.
    let response_format_on_wire = server.requests().iter().any(|r| {
        r.body
            .as_ref()
            .is_some_and(|body| body.pointer("/response_format/type").is_some())
    });
    assert!(
        !response_format_on_wire,
        "invalid schema must NOT send response_format\n{}",
        server.request_log_summary()
    );
    assert!(
        !any_request_advertises_structured_output_tool(&server),
        "invalid schema must NOT advertise the StructuredOutput tool\n{}",
        server.request_log_summary()
    );
}

// ============================================================================
// ACP stdio tests (grok agent stdio)
//
// These test the agent as a server: spawn `grok agent stdio`, speak the full
// ACP protocol over pipes, verify the lifecycle works end-to-end.
// ============================================================================

/// Full ACP lifecycle: initialize → authenticate → create session → prompt.
/// Verifies the agent boots, authenticates with a test API key, creates a
/// session (libgit2 init), and completes a prompt round-trip to the mock server.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_stdio_full_session_lifecycle() {
    with_local_set(|| async {
        let server = MockInferenceServer::start().await.expect("start mock server");
        let workdir = git_workdir();
        let client = GrokStdioClient::spawn(&server, workdir.path()).await;

        // Initialize and authenticate
        let init_resp = client.initialize_with_timeout().await;
        assert!(
            !init_resp.auth_methods.is_empty(),
            "agent should return at least one auth method"
        );

        // Create session (triggers libgit2 init)
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        assert!(!session_id.0.is_empty(), "session ID should be non-empty");

        // Send prompt — triggers inference to mock server
        let result = client.prompt_with_timeout(&session_id, "say hello").await;
        assert!(
            result.is_ok(),
            "prompt failed: {:?}\nrequest log:\n{}\ncaptured text: {:?}\nnotifications: {}\nstderr:\n{}",
            result.err(),
            server.request_log_summary(),
            client.captured_text(),
            client.notification_count(),
            stderr_tail(&client.stderr(), 1200)
        );

        // Verify the mock server received at least one inference request
        assert!(
            server.request_count() > 0,
            "mock server received no inference requests\nrequest log:\n{}\nstderr:\n{}",
            server.request_log_summary(),
            stderr_tail(&client.stderr(), 1200)
        );
    })
    .await;
}

/// Science GB3 product proof: a separately spawned `lumen agent stdio`
/// process accepts the ACP extension, routes through its existing SessionActor
/// and production permission bridge, then persists a successful CSV result.
///
/// This remains ignored because it requires a pre-built composition-root
/// binary. It deliberately uses the shared typed ACP harness rather than a
/// kernel helper so it cannot bypass the product protocol.
#[tokio::test]
#[ignore]
async fn test_stdio_science_csv_allow_product_path() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        // The product enforces store/artifact roots inside the session cwd.
        let store_root = workdir.path().join("science-store");
        let artifact_root = workdir.path().join("science-artifacts");
        let fixture = workdir.path().join("micro.csv");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../xai-grok-science/fixtures/micro.csv"
            ),
            &fixture,
        )
        .expect("copy fixed science fixture");

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            client.ext_method(
                "x.ai/science/run_csv",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-product-allow",
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "artifactRoot": artifact_root,
                    "fixturePath": fixture,
                    "approvalTimeoutMs": 5_000,
                }),
            ),
        )
        .await
        .expect("science extension timed out")
        .unwrap_or_else(|error| {
            panic!(
                "science extension failed: {error:?}\nstderr:\n{}",
                client.stderr()
            )
        });
        let result: serde_json::Value =
            serde_json::from_str(response.0.get()).expect("science extension returned JSON");
        assert_eq!(result["run"]["state"], "succeeded", "result: {result}");
        assert!(
            result["artifacts"]
                .as_array()
                .is_some_and(|items| items.len() == 2)
        );
        assert!(store_root.exists(), "durable store was not created");
        let store = xai_grok_science::ScienceStore::new(&store_root);
        let run_id = xai_grok_science::RunId::new(
            result["run"]["context"]["run_id"]
                .as_str()
                .expect("response must include durable run id"),
        );
        let run = store.load_run(&run_id).expect("reopen durable run");
        assert_eq!(run.state, xai_grok_science::RunState::Succeeded);
        let events = store.events_after(&run_id, 0, 100).expect("replay events");
        assert!(events.len() >= 4, "events: {events:?}");
        assert_eq!(events[0].seq, 1);
        assert!(
            events
                .windows(2)
                .all(|items| items[0].seq + 1 == items[1].seq),
            "event sequence is not monotonic: {events:?}"
        );
        let reopened = xai_grok_science::ScienceStore::new(&store_root);
        assert_eq!(
            events,
            reopened
                .events_after(&run_id, 0, 100)
                .expect("replay after reopen"),
            "restart replay must preserve every event field"
        );
        let premature = client
            .ext_method(
                "x.ai/science/goal_host_verify",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "storeRoot": store_root,
                    "runId": run_id.0,
                }),
            )
            .await;
        assert!(
            premature.is_err(),
            "durable Science success without an active bound Goal/Expert must not complete"
        );
    })
    .await;
}

/// Science GC1 product proof: a spawned `lumen agent stdio` process imports
/// CSV and FASTA fixtures through the SessionActor product path (begin →
/// production permission bridge → formal execute-tool transit → kernel
/// verification). Each run persists an artifact with a content-sniffed MIME,
/// a structured preview record bound to the artifact hash, and evidence.
#[tokio::test]
#[ignore]
async fn test_stdio_science_import_csv_fasta_product_path() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        // The product enforces store/artifact roots inside the session cwd.
        let store_root = workdir.path().join("science-store");
        let artifact_root = workdir.path().join("science-artifacts");
        for name in ["micro.csv", "micro.fasta"] {
            std::fs::copy(
                format!(
                    "{}/../xai-grok-science/fixtures/{name}",
                    env!("CARGO_MANIFEST_DIR")
                ),
                workdir.path().join(name),
            )
            .expect("copy fixed science fixture");
        }

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        for (name, expected_mime) in [("micro.csv", "text/csv"), ("micro.fasta", "text/x-fasta")] {
            let response = tokio::time::timeout(
                Duration::from_secs(30),
                client.ext_method(
                    "x.ai/science/import_preview",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "projectId": "science-product-import",
                        "ownerId": "science-owner",
                        "storeRoot": store_root,
                        "artifactRoot": artifact_root,
                        "sourcePath": workdir.path().join(name),
                        "approvalTimeoutMs": 5_000,
                    }),
                ),
            )
            .await
            .expect("science import timed out")
            .unwrap_or_else(|error| {
                panic!(
                    "science import failed: {error:?}\nstderr:\n{}",
                    client.stderr()
                )
            });
            let result: serde_json::Value =
                serde_json::from_str(response.0.get()).expect("science import returned JSON");
            assert_eq!(result["run"]["state"], "succeeded", "result: {result}");
            let artifacts = result["artifacts"].as_array().expect("artifacts array");
            assert_eq!(artifacts.len(), 1, "result: {result}");
            assert_eq!(artifacts[0]["mime"].as_str(), Some(expected_mime));
            let previews = result["previews"].as_array().expect("previews array");
            assert_eq!(previews.len(), 1, "result: {result}");
            assert_eq!(
                previews[0]["artifact_sha256"].as_str(),
                artifacts[0]["sha256"].as_str(),
                "preview must bind the artifact hash"
            );
            let evidence_items = result["evidence"].as_array().expect("evidence array");
            assert_eq!(evidence_items.len(), 1, "result: {result}");
            assert_eq!(
                evidence_items[0]["artifact_sha256"].as_str(),
                artifacts[0]["sha256"].as_str(),
                "evidence must cite the artifact hash"
            );

            // Durable reopen: the artifact/preview/evidence chain survives.
            let store = xai_grok_science::ScienceStore::new(&store_root);
            let run_id = xai_grok_science::RunId::new(
                result["run"]["context"]["run_id"]
                    .as_str()
                    .expect("response must include durable run id"),
            );
            let run = store.load_run(&run_id).expect("reopen durable run");
            assert_eq!(run.state, xai_grok_science::RunState::Succeeded);
            let previews = store.previews(&run_id).expect("reopen previews");
            assert_eq!(previews.len(), 1);
            assert_eq!(previews[0].preview.mime, expected_mime);
        }
    })
    .await;
}

/// `seq_analyze` must cross the real SessionActor permission seam and commit
/// only store-owned, hash-addressed artifacts. This test specifically rejects
/// the former ACP-task `std::fs::write(artifactRoot/project/seqbench/...)`
/// implementation: that loose directory must not exist.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_seq_analyze_is_actor_gated_and_store_owned() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let artifact_root = workdir.path().join("science-seq-store");
        let source = workdir.path().join("micro.fasta");
        let motif_orf = format!("ATG{}AGA{}TAA", "AAA".repeat(29), "AAA".repeat(2));
        std::fs::write(
            &source,
            format!(
                ">seq1 circular restriction control\nAATTCCCCCG\n\
                 >seq2 Motif vertebrate-mitochondrial stop\n{motif_orf}\n"
            ),
        )
        .expect("write fixed sequence fixture");

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            client.ext_method(
                "x.ai/science/seq_analyze",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                    "translationTableId": 2,
                    "topology": "circular",
                    "restrictionDigestEnzymes": ["EcoRI"],
                    "approvalTimeoutMs": 5_000,
                }),
            ),
        )
        .await
        .expect("seq_analyze timed out")
        .unwrap_or_else(|error| {
            panic!(
                "seq_analyze failed: {error:?}\nstderr:\n{}",
                client.stderr()
            )
        });
        let result: serde_json::Value =
            serde_json::from_str(response.0.get()).expect("seq_analyze returned JSON");

        assert_eq!(result["runtimeAuthority"], "SessionActor-gated ACP adapter");
        assert_eq!(result["run"]["state"], "succeeded", "result: {result}");
        assert_eq!(result["recordCount"], 2, "result: {result}");
        let artifacts = result["artifacts"].as_array().expect("artifacts array");
        assert_eq!(artifacts.len(), 2, "result: {result}");
        assert_eq!(
            result["approvals"][0]["decision"], "allow",
            "result: {result}"
        );
        assert_eq!(
            result["evidence"].as_array().map(Vec::len),
            Some(1),
            "result: {result}"
        );
        assert_eq!(
            result["provenance"].as_array().map(Vec::len),
            Some(1),
            "result: {result}"
        );

        let run_id = xai_grok_science::RunId::new(
            result["run"]["context"]["run_id"]
                .as_str()
                .expect("durable run id"),
        );
        let project_id = xai_grok_science::ProjectId::new("science-seq-project");
        let store = xai_grok_science::ScienceStore::new(&artifact_root);
        assert_eq!(
            store.load_run(&run_id).expect("reopen durable run").state,
            xai_grok_science::RunState::Succeeded
        );
        let durable_artifacts = store.artifacts(&run_id).expect("reopen artifacts");
        for artifact in &durable_artifacts {
            let bytes = store
                .artifact_bytes(
                    &project_id,
                    &run_id,
                    "science-owner",
                    &artifact.relative_path,
                )
                .expect("read registered artifact");
            assert_eq!(format!("{:x}", Sha256::digest(bytes)), artifact.sha256);
        }
        let analysis_artifact = durable_artifacts
            .iter()
            .find(|artifact| artifact.relative_path == Path::new("analysis.json"))
            .expect("durable analysis.json artifact");
        let analysis_bytes = store
            .artifact_bytes(
                &project_id,
                &run_id,
                "science-owner",
                &analysis_artifact.relative_path,
            )
            .expect("read durable analysis.json");
        let analysis: serde_json::Value =
            serde_json::from_slice(&analysis_bytes).expect("parse durable analysis.json");
        assert_eq!(analysis["schema_version"], 6, "analysis: {analysis}");
        assert_eq!(analysis["tool_version"], "1.5.0", "analysis: {analysis}");
        assert_eq!(
            analysis["algorithm_sources"][0]["commit"],
            xai_grok_science::seqbench::MOTIF_COMMIT,
            "analysis: {analysis}"
        );
        assert_eq!(
            analysis["records"][0]["nucleotide_composition"]["A"], 2,
            "analysis: {analysis}"
        );
        assert_eq!(
            analysis["translation_table"]["id"], 2,
            "analysis: {analysis}"
        );
        assert_eq!(
            analysis["restriction_topology"], "circular",
            "analysis: {analysis}"
        );
        assert_eq!(
            analysis["restriction_enzyme_count"], 30,
            "analysis: {analysis}"
        );
        assert_eq!(
            analysis["restriction_digest_enzymes"],
            serde_json::json!(["EcoRI"]),
            "analysis: {analysis}"
        );
        let circular_eco_ri = analysis["records"][0]["restriction_hits"]
            .as_array()
            .expect("seq1 restriction hits")
            .iter()
            .find(|hit| hit["enzyme"] == "EcoRI")
            .expect("origin-spanning EcoRI hit");
        assert_eq!(circular_eco_ri["position"], 9, "analysis: {analysis}");
        assert_eq!(circular_eco_ri["cut_position"], 0, "analysis: {analysis}");
        assert_eq!(circular_eco_ri["strand"], 1, "analysis: {analysis}");
        let circular_digest = analysis["records"][0]["restriction_digest_fragments"]
            .as_array()
            .expect("seq1 digest fragments");
        assert_eq!(circular_digest.len(), 1, "analysis: {analysis}");
        assert_eq!(
            circular_digest[0]["sequence"], "AATTCCCCCG",
            "analysis: {analysis}"
        );
        assert_eq!(
            circular_digest[0]["left_enzyme"], "EcoRI",
            "analysis: {analysis}"
        );
        assert_eq!(
            circular_digest[0]["right_enzyme"], "EcoRI",
            "analysis: {analysis}"
        );
        assert_eq!(
            circular_digest[0]["overhang5"], "AATT",
            "analysis: {analysis}"
        );
        assert_eq!(
            circular_digest[0]["overhang3"], "AATT",
            "analysis: {analysis}"
        );
        assert_eq!(
            analysis["translation_table"]["name"], "Vertebrate Mitochondrial",
            "analysis: {analysis}"
        );
        let table_two_orf = analysis["records"][1]["orfs"]
            .as_array()
            .expect("seq2 ORF array")
            .iter()
            .find(|orf| {
                orf["strand"] == 1
                    && orf["start"] == 0
                    && orf["start_codon"] == "ATG"
                    && orf["stop_codon"] == "AGA"
            })
            .expect("table 2 forward ORF terminated by AGA");
        assert_eq!(table_two_orf["amino_acids"], 30, "analysis: {analysis}");
        assert_eq!(
            result["provenance"][0]["environment"]["algorithm_source_commit"],
            xai_grok_science::seqbench::MOTIF_COMMIT,
            "result: {result}"
        );
        assert_eq!(
            result["provenance"][0]["environment"]["translation_table_id"], "2",
            "result: {result}"
        );
        assert_eq!(
            result["provenance"][0]["environment"]["restriction_topology"], "circular",
            "result: {result}"
        );
        assert_eq!(
            result["provenance"][0]["environment"]["restriction_digest_enzymes"], "EcoRI",
            "result: {result}"
        );
        assert_eq!(
            result["run"]["context"]["environment"]["translation_table_id"], "2",
            "result: {result}"
        );
        assert_eq!(
            result["run"]["context"]["environment"]["restriction_topology"], "circular",
            "result: {result}"
        );
        assert_eq!(
            result["run"]["context"]["environment"]["restriction_digest_enzymes"], "EcoRI",
            "result: {result}"
        );
        assert!(
            !artifact_root
                .join("science-seq-project")
                .join("seqbench")
                .exists(),
            "legacy ACP-task loose artifact directory was written"
        );

        // AUTH-7: the desktop can seed previews from the same Rust engine
        // without adding a Go/HTTP authority. Listing is a read, so it must not
        // ask for another permission after the one that admitted seq_analyze.
        let permission_count = client.permission_request_count();
        assert_eq!(permission_count, 1, "analysis must ask exactly once");
        let list_params =
            |session_id: &str, owner_id: &str, project_id: &str, store_root: &Path| {
                serde_json::json!({
                    "sessionId": session_id,
                    "ownerId": owner_id,
                    "projectId": project_id,
                    "runId": run_id.0.as_str(),
                    "storeRoot": store_root,
                })
            };
        let response = client
            .ext_method(
                "x.ai/science/artifact_list",
                list_params(
                    session_id.0.as_ref(),
                    "science-owner",
                    "science-seq-project",
                    &artifact_root,
                ),
            )
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "artifact_list failed: {error:?}\nstderr:\n{}",
                    client.stderr()
                )
            });
        let listed: serde_json::Value =
            serde_json::from_str(response.0.get()).expect("artifact_list returned JSON");
        let listed = listed.as_array().expect("artifact_list array");
        assert_eq!(listed.len(), durable_artifacts.len());
        let expected_run_root = std::fs::canonicalize(artifact_root.join("runs").join(&run_id.0))
            .expect("canonical run root");
        for item in listed {
            let sha = item["sha256"].as_str().expect("listed sha256");
            assert_eq!(item["artifact_id"], sha);
            assert!(
                durable_artifacts
                    .iter()
                    .any(|artifact| artifact.sha256 == sha),
                "list returned an unregistered digest: {item}"
            );
            let path = Path::new(item["path"].as_str().expect("listed path"));
            assert!(path.is_absolute());
            assert!(path.starts_with(&expected_run_root));
        }
        assert_eq!(
            client.permission_request_count(),
            permission_count,
            "read-only artifact listing asked for permission"
        );

        // Every identity dimension fails closed, including another real
        // session in the same workspace. None may receive a preview path.
        for (label, params) in [
            (
                "wrong owner",
                list_params(
                    session_id.0.as_ref(),
                    "other-owner",
                    "science-seq-project",
                    &artifact_root,
                ),
            ),
            (
                "wrong project",
                list_params(
                    session_id.0.as_ref(),
                    "science-owner",
                    "other-project",
                    &artifact_root,
                ),
            ),
        ] {
            assert!(
                client
                    .ext_method("x.ai/science/artifact_list", params)
                    .await
                    .is_err(),
                "{label} listed artifacts"
            );
        }
        let other_session = client.create_session_with_timeout(workdir.path()).await;
        assert!(
            client
                .ext_method(
                    "x.ai/science/artifact_list",
                    list_params(
                        other_session.0.as_ref(),
                        "science-owner",
                        "science-seq-project",
                        &artifact_root,
                    ),
                )
                .await
                .is_err(),
            "a different real session listed the run"
        );
        let outside = tempfile::tempdir().expect("outside workspace");
        let absent_outside = outside.path().join("must-not-be-created");
        assert!(
            client
                .ext_method(
                    "x.ai/science/artifact_list",
                    list_params(
                        session_id.0.as_ref(),
                        "science-owner",
                        "science-seq-project",
                        &absent_outside,
                    ),
                )
                .await
                .is_err(),
            "an outside store root was accepted"
        );
        assert!(
            !absent_outside.exists(),
            "rejected read-only listing created its store root"
        );

        // A store record is not enough: list must reopen the bytes and reject
        // digest drift before returning even a partial result.
        let tampered = PathBuf::from(
            listed[0]["path"]
                .as_str()
                .expect("first listed artifact path"),
        );
        std::fs::write(&tampered, b"tampered after commit\n").expect("tamper registered artifact");
        assert!(
            client
                .ext_method(
                    "x.ai/science/artifact_list",
                    list_params(
                        session_id.0.as_ref(),
                        "science-owner",
                        "science-seq-project",
                        &artifact_root,
                    ),
                )
                .await
                .is_err(),
            "hash-drifted artifact was listed"
        );
        assert_eq!(
            client.permission_request_count(),
            permission_count,
            "rejected listing asked for permission"
        );
    })
    .await;
}

/// Owner/session/workspace checks must fail before a sequence run is opened.
/// An endpoint that merely checks the source in the UI layer would accept at
/// least one of these forged requests.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_seq_analyze_boundaries_fail_closed() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let artifact_root = workdir.path().join("science-seq-store");
        let source = workdir.path().join("inside.fasta");
        std::fs::write(&source, b">inside\nACGT\n").expect("write inside fixture");
        let outside = tempfile::NamedTempFile::new().expect("outside fixture");
        std::fs::write(outside.path(), b">outside\nACGT\n").expect("write outside fixture");

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        for (label, params) in [
            (
                "empty owner",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                }),
            ),
            (
                "unknown session",
                serde_json::json!({
                    "sessionId": "forged-session",
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                }),
            ),
            (
                "outside workspace",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": outside.path(),
                }),
            ),
            (
                "unsupported translation table",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                    "translationTableId": 27,
                }),
            ),
            (
                "unsupported topology",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                    "topology": "toroidal",
                }),
            ),
            (
                "unsupported digest enzyme",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                    "restrictionDigestEnzymes": ["UnknownI"],
                }),
            ),
        ] {
            assert!(
                client
                    .ext_method("x.ai/science/seq_analyze", params)
                    .await
                    .is_err(),
                "{label} request was accepted"
            );
        }
        assert!(
            !artifact_root.join("runs").exists(),
            "rejected boundary requests opened durable runs"
        );
    })
    .await;
}

/// A production permission refusal closes the durable run but must not create
/// an artifact, evidence, provenance, or the legacy loose output.
///
/// The stdio harness' `PermissionResponse::Reject` becomes
/// `RequestPermissionOutcome::Cancelled` in the ACP bridge, so this product
/// seam exercises the Cancelled terminal. The protocol tests separately prove
/// the Denied terminal has the same no-output invariant.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_seq_analyze_denied_writes_nothing() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let artifact_root = workdir.path().join("science-seq-store");
        let source = workdir.path().join("micro.fasta");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../xai-grok-science/fixtures/micro.fasta"
            ),
            &source,
        )
        .expect("copy fixed sequence fixture");

        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            workdir.path(),
            PermissionResponse::Reject,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let denied = client
            .ext_method(
                "x.ai/science/seq_analyze",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        assert!(
            denied.is_err(),
            "denied analysis returned success: {denied:?}"
        );

        let runs = std::fs::read_dir(artifact_root.join("runs"))
            .expect("denied request must leave a durable run")
            .collect::<Result<Vec<_>, _>>()
            .expect("read durable runs");
        assert_eq!(runs.len(), 1, "denied request opened multiple runs");
        let run_id = xai_grok_science::RunId::new(
            runs[0]
                .file_name()
                .to_str()
                .expect("UTF-8 run id")
                .to_owned(),
        );
        let store = xai_grok_science::ScienceStore::new(&artifact_root);
        assert_eq!(
            store.load_run(&run_id).expect("reopen denied run").state,
            xai_grok_science::RunState::Cancelled
        );
        assert!(store.artifacts(&run_id).expect("artifacts").is_empty());
        assert!(store.evidence(&run_id).expect("evidence").is_empty());
        assert!(store.provenance(&run_id).expect("provenance").is_empty());
        assert!(
            client
                .ext_method(
                    "x.ai/science/artifact_list",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "science-owner",
                        "projectId": "science-seq-project",
                        "runId": run_id.0.as_str(),
                        "storeRoot": artifact_root,
                    }),
                )
                .await
                .is_err(),
            "a denied run exposed an artifact listing"
        );
        assert!(
            !artifact_root
                .join("science-seq-project")
                .join("seqbench")
                .exists(),
            "denied request wrote the legacy loose artifact directory"
        );
    })
    .await;
}

/// If the production permission request is never answered, the actor must
/// persist TimedOut and must not start sequence analysis or publish outputs.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_seq_analyze_permission_timeout_writes_nothing() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let artifact_root = workdir.path().join("science-seq-timeout-store");
        let source = workdir.path().join("micro.fasta");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../xai-grok-science/fixtures/micro.fasta"
            ),
            &source,
        )
        .expect("copy fixed sequence fixture");

        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            workdir.path(),
            PermissionResponse::NeverRespond,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let timed_out = client
            .ext_method(
                "x.ai/science/seq_analyze",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                    "approvalTimeoutMs": 100,
                }),
            )
            .await;
        assert!(
            timed_out.is_err(),
            "timed-out analysis returned success: {timed_out:?}"
        );
        assert_eq!(client.permission_request_count(), 1);

        let runs = std::fs::read_dir(artifact_root.join("runs"))
            .expect("timeout must leave a durable run")
            .collect::<Result<Vec<_>, _>>()
            .expect("read durable runs");
        assert_eq!(runs.len(), 1);
        let run_id = xai_grok_science::RunId::new(
            runs[0]
                .file_name()
                .to_str()
                .expect("UTF-8 run id")
                .to_owned(),
        );
        let store = xai_grok_science::ScienceStore::new(&artifact_root);
        assert_eq!(
            store.load_run(&run_id).expect("reopen timed-out run").state,
            xai_grok_science::RunState::TimedOut
        );
        assert!(store.artifacts(&run_id).expect("artifacts").is_empty());
        assert!(store.evidence(&run_id).expect("evidence").is_empty());
        assert!(store.provenance(&run_id).expect("provenance").is_empty());
        assert!(
            !artifact_root
                .join("science-seq-project")
                .join("seqbench")
                .exists(),
            "timed-out request wrote the legacy loose artifact directory"
        );
    })
    .await;
}

/// Input parsing happens only after Allow. A malformed source therefore must
/// close the already-durable run as Failed without publishing partial output.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_seq_analyze_malformed_input_fails_without_outputs() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let artifact_root = workdir.path().join("science-seq-malformed-store");
        let source = workdir.path().join("empty.fasta");
        std::fs::write(&source, b"").expect("write malformed sequence fixture");

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let failed = client
            .ext_method(
                "x.ai/science/seq_analyze",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "projectId": "science-seq-project",
                    "ownerId": "science-owner",
                    "artifactRoot": artifact_root,
                    "sourcePath": source,
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        assert!(
            failed.is_err(),
            "malformed analysis returned success: {failed:?}"
        );
        assert_eq!(client.permission_request_count(), 1);

        let runs = std::fs::read_dir(artifact_root.join("runs"))
            .expect("parse failure must leave a durable run")
            .collect::<Result<Vec<_>, _>>()
            .expect("read durable runs");
        assert_eq!(runs.len(), 1);
        let run_id = xai_grok_science::RunId::new(
            runs[0]
                .file_name()
                .to_str()
                .expect("UTF-8 run id")
                .to_owned(),
        );
        let store = xai_grok_science::ScienceStore::new(&artifact_root);
        assert_eq!(
            store.load_run(&run_id).expect("reopen failed run").state,
            xai_grok_science::RunState::Failed
        );
        assert_eq!(
            store.approvals(&run_id).expect("approval")[0].decision,
            xai_grok_science::ApprovalDecision::Allow
        );
        assert!(store.artifacts(&run_id).expect("artifacts").is_empty());
        assert!(store.evidence(&run_id).expect("evidence").is_empty());
        assert!(store.provenance(&run_id).expect("provenance").is_empty());
        assert!(
            !artifact_root
                .join("science-seq-project")
                .join("seqbench")
                .exists(),
            "parse failure wrote the legacy loose artifact directory"
        );
    })
    .await;
}

/// Science GC2 product proof: PubMed (two-exchange protocol), ChEMBL,
/// Crossref, UniProt, Europe PMC, and OpenAlex (single-exchange) fetches run through the SessionActor
/// product path with offline fixtures as mock transport. Each run persists raw response
/// artifacts, a redacted per-exchange audit, citation-bearing evidence, and
/// provenance naming the connector TOS.
#[tokio::test]
#[ignore]
async fn test_stdio_science_connector_fetch_product_path() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-store");
        let artifact_root = workdir.path().join("science-artifacts");
        for name in [
            "connector_pubmed_esearch.json",
            "connector_pubmed_esummary.json",
            "connector_chembl_search.json",
            "connector_crossref_works.json",
            "connector_uniprot_search.json",
            "connector_europepmc_search.json",
            "connector_openalex_search.json",
            "connector_semantic_scholar_search.json",
            "connector_arxiv_search.xml",
        ] {
            std::fs::copy(
                format!(
                    "{}/../xai-grok-science/fixtures/{name}",
                    env!("CARGO_MANIFEST_DIR")
                ),
                workdir.path().join(name),
            )
            .expect("copy connector fixture");
        }

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let cases: [(&str, &str, Vec<&str>, usize, &str); 8] = [
            (
                "pubmed",
                "crispr",
                vec![
                    "connector_pubmed_esearch.json",
                    "connector_pubmed_esummary.json",
                ],
                2,
                "Base editing advances",
            ),
            (
                "chembl",
                "aspirin",
                vec!["connector_chembl_search.json"],
                1,
                "ASPIRIN",
            ),
            (
                "crossref",
                "reproducible science",
                vec!["connector_crossref_works.json"],
                1,
                "Reproducible science workflows",
            ),
            (
                "uniprot",
                "human insulin",
                vec!["connector_uniprot_search.json"],
                1,
                "Insulin",
            ),
            (
                "europepmc",
                "single cell RNA",
                vec!["connector_europepmc_search.json"],
                1,
                "Reproducible single-cell analysis",
            ),
            (
                "openalex",
                "single cell RNA",
                vec!["connector_openalex_search.json"],
                1,
                "Reproducible scholarly graphs",
            ),
            (
                "semantic-scholar",
                "machine learning",
                vec!["connector_semantic_scholar_search.json"],
                1,
                "Attention Is All You Need",
            ),
            (
                "arxiv",
                "transformer",
                vec!["connector_arxiv_search.xml"],
                1,
                "Attention Is All You Need",
            ),
        ];
        for (connector, query, fixtures, exchange_count, first_title) in cases {
            let fixture_paths: Vec<_> = fixtures
                .iter()
                .map(|name| workdir.path().join(name))
                .collect();
            let response = tokio::time::timeout(
                Duration::from_secs(30),
                client.ext_method(
                    "x.ai/science/connector_fetch",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "projectId": "science-product-connector",
                        "ownerId": "science-owner",
                        "storeRoot": store_root,
                        "artifactRoot": artifact_root,
                        "connectorId": connector,
                        "query": query,
                        "maxResults": 5,
                        "fixturePaths": fixture_paths,
                        "approvalTimeoutMs": 5_000,
                    }),
                ),
            )
            .await
            .expect("connector fetch timed out")
            .unwrap_or_else(|error| {
                panic!(
                    "connector fetch failed: {error:?}\nstderr:\n{}",
                    client.stderr()
                )
            });
            let result: serde_json::Value =
                serde_json::from_str(response.0.get()).expect("connector fetch returned JSON");
            assert_eq!(result["run"]["state"], "succeeded", "result: {result}");
            assert_eq!(
                result["artifacts"].as_array().map(Vec::len),
                Some(exchange_count),
                "result: {result}"
            );
            assert_eq!(
                result["parsed"]["records"][0]["title"].as_str(),
                Some(first_title),
                "result: {result}"
            );
            let notice = result["user_notice"].as_str().unwrap_or_default();
            assert!(
                !notice.is_empty(),
                "connector notice must reach the product response"
            );
            if connector == "pubmed" {
                assert!(notice.contains("NCBI disclaimer"), "notice: {notice}");
            }
            if connector == "uniprot" {
                assert!(notice.contains("CC BY 4.0"), "notice: {notice}");
            }
            if connector == "europepmc" {
                assert!(notice.contains("article-level license"), "notice: {notice}");
            }
            if connector == "openalex" {
                assert!(notice.contains("CC0"), "notice: {notice}");
                assert!(notice.contains("runtime key"), "notice: {notice}");
            }
            if connector == "semantic-scholar" {
                assert!(notice.contains("ODC-BY"), "notice: {notice}");
            }
            // Evidence carries the scientific citation; the audit is redacted.
            let claim = result["evidence"][0]["claim"].as_str().unwrap_or_default();
            assert!(claim.contains(query), "claim: {claim}");
            assert!(claim.contains(first_title), "claim: {claim}");
            let audits = result["audits"].as_array().expect("audits array");
            assert_eq!(audits.len(), exchange_count);
            for audit in audits {
                let hash = audit["request_sha256"].as_str().unwrap_or_default();
                assert_eq!(hash.len(), 64, "audit: {audit}");
                assert!(!hash.contains(query), "audit must not leak query terms");
            }
            assert!(
                result["provenance"][0]["license"]
                    .as_str()
                    .is_some_and(|tos| tos.starts_with("https://")),
                "result: {result}"
            );

            // Durable reopen: records survive a store restart.
            let store = xai_grok_science::ScienceStore::new(&store_root);
            let run_id = xai_grok_science::RunId::new(
                result["run"]["context"]["run_id"]
                    .as_str()
                    .expect("response must include durable run id"),
            );
            let run = store.load_run(&run_id).expect("reopen durable run");
            assert_eq!(run.state, xai_grok_science::RunState::Succeeded);
            assert_eq!(
                store.artifacts(&run_id).expect("reopen artifacts").len(),
                exchange_count
            );
        }
    })
    .await;
}

/// S3 L4 proof: a debug-built product binary drives approval and the sole
/// SessionActor into a real, isolated local sshd. Both directions preserve
/// bytes and durable records retain only redacted target correlation data.
#[tokio::test]
#[ignore]
async fn test_stdio_science_ssh_put_get_product_path() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let fixture = LocalSshdFixture::start(workdir.path());
        let probe = Command::new("/usr/bin/ssh")
            .args(["-F"])
            .arg(&fixture.ssh_config_file)
            .arg("fixture.lumen.test")
            .arg("true")
            .output()
            .expect("run fixture SSH probe");
        assert!(
            probe.status.success(),
            "fixture SSH probe failed: {}",
            String::from_utf8_lossy(&probe.stderr)
        );
        let source = workdir.path().join("ssh-source.bin");
        let downloaded = workdir.path().join("ssh-downloaded.bin");
        let bytes = b"lumen science ssh fixture bytes\n";
        std::fs::write(&source, bytes).expect("write source");
        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let common = serde_json::json!({
            "sessionId": session_id.0.as_ref(), "projectId": "science-ssh-fixture",
            "ownerId": "science-owner", "storeRoot": workdir.path().join("science-store"),
            "artifactRoot": workdir.path().join("science-artifacts"), "port": fixture.port,
            "hostKeySha256": fixture.host_key_sha256, "user": std::env::var("USER").unwrap(),
            "identityFile": fixture.identity_file, "knownHostsFile": fixture.known_hosts_file,
            "sshConfigFile": fixture.ssh_config_file, "approvalTimeoutMs": 5_000,
            "transportTimeoutMs": 5_000,
        });
        let mut put = common.clone();
        put["direction"] = serde_json::json!("put");
        put["localPath"] = serde_json::json!(source);
        put["remotePath"] = serde_json::json!("lumen-science-fixture.bin");
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            client.ext_method("x.ai/science/ssh_scp_fixture", put),
        )
        .await
        .expect("put extension timeout")
        .expect("put product response");
        let put_result: serde_json::Value =
            serde_json::from_str(response.0.get()).expect("put JSON");
        assert_eq!(put_result["outcome"], "complete", "{put_result}");
        let mut get = common;
        get["direction"] = serde_json::json!("get");
        get["localPath"] = serde_json::json!(downloaded.clone());
        get["remotePath"] = serde_json::json!("lumen-science-fixture.bin");
        let response = tokio::time::timeout(
            Duration::from_secs(30),
            client.ext_method("x.ai/science/ssh_scp_fixture", get),
        )
        .await
        .expect("get extension timeout")
        .expect("get product response");
        let get_result: serde_json::Value =
            serde_json::from_str(response.0.get()).expect("get JSON");
        assert_eq!(get_result["outcome"], "complete", "{get_result}");
        assert_eq!(std::fs::read(downloaded).expect("read downloaded"), bytes);
    })
    .await;
}

/// S3 L4 terminal paths: both timeout and cancellation kill/reap the SCP
/// child through SessionActor and leave no transfer artifact.
#[tokio::test]
#[ignore]
async fn test_stdio_science_ssh_timeout_cancel_product_path() {
    with_local_set(|| async {
        let server = MockInferenceServer::start().await.expect("start mock server");
        let workdir = git_workdir(); let fixture = LocalSshdFixture::start(workdir.path());
        let store_root = workdir.path().join("science-store");
        let source = workdir.path().join("ssh-source.bin"); std::fs::write(&source, b"fixture bytes").unwrap();
        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await; let session_id = client.create_session_with_timeout(workdir.path()).await;
        let base = serde_json::json!({ "sessionId": session_id.0.as_ref(), "projectId": "science-ssh-terminal",
            "ownerId": "science-owner", "storeRoot": store_root, "artifactRoot": workdir.path().join("science-artifacts"),
            "port": fixture.port, "hostKeySha256": fixture.host_key_sha256, "user": std::env::var("USER").unwrap(),
            "identityFile": fixture.identity_file, "knownHostsFile": fixture.known_hosts_file, "sshConfigFile": fixture.ssh_config_file,
            "direction": "put", "localPath": source, "remotePath": "lumen-science-terminal.bin", "approvalTimeoutMs": 5_000 });
        let mut timeout = base.clone(); timeout["transportTimeoutMs"] = serde_json::json!(1);
        let response = client.ext_method("x.ai/science/ssh_scp_fixture", timeout).await.expect("timeout response");
        let result: serde_json::Value = serde_json::from_str(response.0.get()).unwrap();
        assert_eq!(result["outcome"], "timed_out", "{result}");
        let timeout_run_id = xai_grok_science::RunId::new(result["run_id"].as_str().expect("timeout run_id"));
        let store = xai_grok_science::ScienceStore::new(&store_root);
        assert_eq!(store.load_run(&timeout_run_id).unwrap().state, xai_grok_science::RunState::TimedOut);
        assert!(store.artifacts(&timeout_run_id).unwrap().is_empty(), "timeout must not register artifacts");
        let mut cancel = base; cancel["transportTimeoutMs"] = serde_json::json!(5_000); cancel["cancelAfterMs"] = serde_json::json!(1);
        let response = client.ext_method("x.ai/science/ssh_scp_fixture", cancel).await.expect("cancel response");
        let result: serde_json::Value = serde_json::from_str(response.0.get()).unwrap();
        assert_eq!(result["outcome"], "cancelled", "{result}");
        let cancel_run_id = xai_grok_science::RunId::new(result["run_id"].as_str().expect("cancel run_id"));
        assert_eq!(store.load_run(&cancel_run_id).unwrap().state, xai_grok_science::RunState::Cancelled);
        assert!(store.artifacts(&cancel_run_id).unwrap().is_empty(), "cancellation must not register artifacts");
    }).await;
}

/// A real ACP client cancellation of the permission prompt must durably
/// record the terminal Cancel decision: no artifacts, no tool-start event.
/// (The harness expresses rejection as the ACP `Cancelled` outcome, which the
/// product maps to ApprovalDecision::Cancel; a policy-side Deny is covered by
/// kernel unit tests.)
#[tokio::test]
#[ignore]
async fn test_stdio_science_csv_deny_product_path() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        // The product enforces store/artifact roots inside the session cwd.
        let store_root = workdir.path().join("science-store");
        let fixture = workdir.path().join("micro.csv");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../xai-grok-science/fixtures/micro.csv"
            ),
            &fixture,
        )
        .expect("copy fixed science fixture");
        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            workdir.path(),
            PermissionResponse::Reject,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let response = client
            .ext_method(
                "x.ai/science/run_csv",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(), "projectId": "science-product-deny",
                    "ownerId": "science-owner", "storeRoot": store_root,
                    "artifactRoot": workdir.path().join("science-artifacts"), "fixturePath": fixture,
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        assert!(
            response.is_err(),
            "deny must not report success: {response:?}"
        );
        let run_id = std::fs::read_dir(store_root.join("runs"))
            .expect("durable denied run directory")
            .next()
            .expect("one denied run")
            .expect("run directory entry")
            .file_name()
            .to_string_lossy()
            .to_string();
        let store = xai_grok_science::ScienceStore::new(&store_root);
        let run = store
            .load_run(&xai_grok_science::RunId::new(run_id))
            .expect("load denied run");
        assert_eq!(run.state, xai_grok_science::RunState::Cancelled);
        let events = store
            .events_after(&run.context.run_id, 0, 100)
            .expect("load events");
        assert!(!events.iter().any(|event| event.kind == "tool.started"));
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        assert_eq!(
            store.approvals(&run.context.run_id).unwrap()[0].decision,
            xai_grok_science::ApprovalDecision::Cancel
        );
    })
    .await;
}

/// A client that never resolves the production permission prompt must leave a
/// durable timeout record, not execute after the request has expired.
#[tokio::test]
#[ignore]
async fn test_stdio_science_csv_timeout_product_path() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        // The product enforces store/artifact roots inside the session cwd.
        let store_root = workdir.path().join("science-store");
        let fixture = workdir.path().join("micro.csv");
        std::fs::copy(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../xai-grok-science/fixtures/micro.csv"
            ),
            &fixture,
        )
        .expect("copy fixed science fixture");
        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            workdir.path(),
            PermissionResponse::NeverRespond,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let response = client
            .ext_method(
                "x.ai/science/run_csv",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(), "projectId": "science-product-timeout",
                    "ownerId": "science-owner", "storeRoot": store_root,
                    "artifactRoot": workdir.path().join("science-artifacts"), "fixturePath": fixture,
                    "approvalTimeoutMs": 100,
                }),
            )
            .await;
        assert!(
            response.is_err(),
            "timeout must not report success: {response:?}"
        );
        let run_id = std::fs::read_dir(store_root.join("runs"))
            .expect("durable timed-out run directory")
            .next()
            .expect("one timed-out run")
            .expect("run directory entry")
            .file_name()
            .to_string_lossy()
            .to_string();
        let store = xai_grok_science::ScienceStore::new(&store_root);
        let run = store
            .load_run(&xai_grok_science::RunId::new(run_id))
            .expect("load timed-out run");
        assert_eq!(run.state, xai_grok_science::RunState::TimedOut);
        assert!(
            !store
                .events_after(&run.context.run_id, 0, 100)
                .unwrap()
                .iter()
                .any(|event| event.kind == "tool.started")
        );
        assert!(store.artifacts(&run.context.run_id).unwrap().is_empty());
        assert_eq!(
            store.approvals(&run.context.run_id).unwrap()[0].decision,
            xai_grok_science::ApprovalDecision::Timeout
        );
    })
    .await;
}

/// Verify that x.ai/session/close frees the session.
/// Creates a session, closes it via ext_method, then verifies session/info
/// returns an empty response (session no longer exists).
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_stdio_session_close() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let client = GrokStdioClient::spawn(&server, workdir.path()).await;

        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        // Session should be alive — session/info returns data with sessionId
        let info_resp = client
            .ext_method(
                "x.ai/session/info",
                serde_json::json!({ "sessionId": session_id.0.as_ref() }),
            )
            .await;
        assert!(
            info_resp.is_ok(),
            "session/info should succeed before close"
        );
        let info: serde_json::Value =
            serde_json::from_str(info_resp.unwrap().0.get()).expect("parse info");
        assert_eq!(
            info["result"]["sessionId"].as_str(),
            Some(session_id.0.as_ref()),
            "session/info should return the session we created, got: {info}"
        );

        // Close the session
        let close_resp = client
            .ext_method(
                "x.ai/session/close",
                serde_json::json!({ "sessionId": session_id.0.as_ref() }),
            )
            .await;
        assert!(
            close_resp.is_ok(),
            "session/close failed: {:?}\nstderr:\n{}",
            close_resp.err(),
            stderr_tail(&client.stderr(), 1200)
        );

        // Session should be gone — session/info returns empty result (no sessionId)
        let info_after = client
            .ext_method(
                "x.ai/session/info",
                serde_json::json!({ "sessionId": session_id.0.as_ref() }),
            )
            .await;
        assert!(info_after.is_ok(), "session/info should still succeed");
        let info_val: serde_json::Value =
            serde_json::from_str(info_after.unwrap().0.get()).expect("parse info after close");
        assert!(
            info_val["result"].get("sessionId").is_none(),
            "session/info should not contain sessionId after close, got: {info_val}"
        );
    })
    .await;
}

#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_stdio_prompt_then_immediate_load_session() {
    with_local_set(|| async {
        let server = MockInferenceServer::start().await.expect("start mock server");
        let workdir = git_workdir();
        let mut writer = GrokStdioClient::spawn(&server, workdir.path()).await;

        let init_resp = writer.initialize_with_timeout().await;
        assert!(
            !init_resp.auth_methods.is_empty(),
            "agent should return at least one auth method"
        );

        let session_id = writer.create_session_with_timeout(workdir.path()).await;
        let result = writer.prompt_with_timeout(&session_id, "say hello").await;
        assert!(
            result.is_ok(),
            "prompt failed before load_session: {:?}\nrequest log:\n{}\nstderr:\n{}",
            result.err(),
            server.request_log_summary(),
            stderr_tail(&writer.stderr(), 1200)
        );

        let shared_home = writer.take_home();
        drop(writer);

        let reader = GrokStdioClient::spawn_with_home(&server, workdir.path(), shared_home).await;
        reader.initialize_with_timeout().await;
        let _ = reader
            .load_session_with_timeout(&session_id, workdir.path())
            .await;
        assert!(
            reader.notification_count() > 0,
            "reloaded session should emit replay notifications\nstderr:\n{}",
            stderr_tail(&reader.stderr(), 1200)
        );
        assert!(
            reader.captured_text().contains("Echo:") && reader.captured_text().contains("say hello"),
            "reloaded session should replay the expected assistant text\ncaptured:\n{}\nstderr:\n{}",
            reader.captured_text(),
            stderr_tail(&reader.stderr(), 1200)
        );
    })
    .await;
}

// ── Raw-wire stdio driving (Xcode / Foundation shape) ───────────────────────

/// Serialize `req` compactly, then rewrite its method to the Foundation-escaped
/// form (`"session/new"` → `"session\/new"`) by string surgery, asserting the
/// escape really is in the wire bytes — so a serde_json formatting change can
/// never silently downgrade this test to the unescaped path.
fn line_with_escaped_method(req: &serde_json::Value, method: &str) -> String {
    let plain = format!(r#""method":"{method}""#);
    let escaped = format!(r#""method":"{}""#, method.replace('/', r"\/"));
    let line = req.to_string().replacen(&plain, &escaped, 1);
    assert!(
        line.contains(&escaped),
        "escaped method must be on the wire: {line}"
    );
    // One replacement only: a params value carrying the same substring must
    // fail here rather than get silently double-mangled.
    assert!(
        !line.contains(&plain),
        "plain method form must be gone from the wire: {line}"
    );
    line
}

/// Xcode 27 beta's ACP client (Swift/Foundation `JSONEncoder`) escapes forward
/// slashes in the JSON-RPC `method` field (`"session\/new"` — spec-legal JSON)
/// and uses uppercase string UUID request ids. acp 0.6 parses `method` as a
/// borrowed str, so an escaped method used to fail the envelope parse and the
/// line was silently dropped: `initialize` (no slash) worked, every `session/*`
/// request hung forever. Drives the built binary with the raw wire bytes and
/// asserts every escaped-method request gets a response.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_stdio_xcode_escaped_slash_methods_get_responses() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = git_workdir();
    let mut agent = RawStdioClient::spawn(&server, workdir.path()).await;

    // initialize/authenticate carry no slash (they work from Xcode too), but
    // ride string UUID ids and minimal capabilities like Xcode's client.
    let init_id = "9B25E574-2F0C-4C8A-8C7E-2E9B3A4A0F01";
    agent
        .send_line(&format!(
            r#"{{"jsonrpc":"2.0","id":"{init_id}","method":"initialize","params":{{"protocolVersion":1,"clientCapabilities":{{"fs":{{"readTextFile":false,"writeTextFile":false}},"terminal":false}},"_meta":{{"startupHints":{{"nonInteractive":true,"skipGitStatus":true,"skipProjectLayout":true}},"clientType":"xcode-test","clientVersion":"27.0"}}}}}}"#
        ))
        .await;
    let init_resp = agent
        .response_for_id(init_id, "initialize", Duration::from_secs(20))
        .await;
    assert!(
        init_resp.get("result").is_some(),
        "initialize must respond with a result, got: {init_resp}"
    );

    let auth_id = "3C41A7D9-6B58-4E2F-A0D3-5F8C1B7E0A02";
    agent
        .send_line(&format!(
            r#"{{"jsonrpc":"2.0","id":"{auth_id}","method":"authenticate","params":{{"methodId":"xai.api_key","_meta":{{"headless":true}}}}}}"#
        ))
        .await;
    let auth_resp = agent
        .response_for_id(auth_id, "authenticate", Duration::from_secs(20))
        .await;
    assert!(
        auth_resp.get("error").is_none(),
        "authenticate failed: {auth_resp}\nstderr:\n{}",
        stderr_tail(&agent.stderr(), 1200)
    );

    // session/new with the escaped method literally on the wire.
    let new_id = "5DE7EA60-0B0C-4A43-9650-2B72CDF6A44B";
    let line = line_with_escaped_method(
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": new_id,
            "method": "session/new",
            "params": { "cwd": workdir.path(), "mcpServers": [] },
        }),
        "session/new",
    );
    agent.send_line(&line).await;
    // Returning at all asserts the exact-string-UUID id echo: response_for_id
    // only matches on it.
    let new_resp = agent
        .response_for_id(new_id, "escaped session/new", Duration::from_secs(20))
        .await;
    let session_id = new_resp["result"]["sessionId"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "escaped session/new must return a sessionId, got: {new_resp}\nstderr:\n{}",
                stderr_tail(&agent.stderr(), 1200)
            )
        })
        .to_string();

    // session/prompt with the escaped method: must produce a response (result
    // or error) rather than silence; against the echo mock it completes.
    let prompt_id = "A1F3C9B2-7D64-4E85-B9A0-8C2D5E6F1A03";
    let line = line_with_escaped_method(
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": prompt_id,
            "method": "session/prompt",
            "params": {
                "sessionId": session_id,
                "prompt": [{ "type": "text", "text": "say hello" }],
            },
        }),
        "session/prompt",
    );
    agent.send_line(&line).await;
    let prompt_resp = agent
        .response_for_id(prompt_id, "escaped session/prompt", Duration::from_secs(30))
        .await;
    assert!(
        prompt_resp.get("error").is_none(),
        "escaped session/prompt must complete: {prompt_resp}\nrequest log:\n{}\nstderr:\n{}",
        server.request_log_summary(),
        stderr_tail(&agent.stderr(), 1200)
    );
    assert!(
        prompt_resp["result"]["stopReason"].is_string(),
        "prompt response should carry a stopReason, got: {prompt_resp}"
    );
    assert!(
        server.request_count() > 0,
        "mock server received no inference requests\nrequest log:\n{}\nstderr:\n{}",
        server.request_log_summary(),
        stderr_tail(&agent.stderr(), 1200)
    );
}

// ── Config test harness ─────────────────────────────────────────────────────

/// Isolated headless run with a custom `~/.grok/`. Clean env (no leaked
/// host credentials). Write config files into `grok_dir()` before `run()`.
struct ConfigTestHarness {
    home: tempfile::TempDir,
    workdir: tempfile::TempDir,
    env: Vec<(String, String)>,
}

impl ConfigTestHarness {
    fn new(server: &MockInferenceServer) -> Self {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".grok")).unwrap();
        Self {
            home,
            workdir: git_workdir(),
            env: vec![
                ("GROK_CLI_CHAT_PROXY_BASE_URL".into(), server.url()),
                ("GROK_TELEMETRY_ENABLED".into(), "false".into()),
                ("GROK_FEEDBACK_ENABLED".into(), "false".into()),
                ("GROK_TRACE_UPLOAD".into(), "false".into()),
                ("GROK_INSTRUMENTATION".into(), "disabled".into()),
                ("GROK_DISABLE_AUTOUPDATER".into(), "1".into()),
            ],
        }
    }

    fn grok_dir(&self) -> std::path::PathBuf {
        self.home.path().join(".grok")
    }

    fn env(&mut self, key: &str, value: &str) -> &mut Self {
        self.env.push((key.into(), value.into()));
        self
    }

    async fn run(&self) -> HeadlessResult {
        let mut cmd = tokio::process::Command::new(grok_binary());
        cmd.args(["-p", "say hello", "--yolo"])
            .current_dir(self.workdir.path())
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .env_clear()
            .env("HOME", self.home.path())
            // Windows resolves `~` via USERPROFILE, not HOME — pin the grok
            // home explicitly so the sandbox holds on all platforms (see
            // `test_env_cmd_tokio`).
            .env("GROK_HOME", self.grok_dir())
            .env("PATH", std::env::var("PATH").unwrap_or_default());
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        run_headless_with_cmd(cmd).await
    }
}

// ── Enterprise managed config tests ────────────────────────────────────────

/// Enterprise BYOK: managed_config.toml overrides grok-build with a custom
/// endpoint + env_key. Mock rejects unauthenticated requests with 401.
/// Regression guard for the 0.1.220 authentication regression.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_managed_config_byok_sends_authorized_requests() {
    let server = MockInferenceServer::start_with_required_auth(
        vec![MockModelEntry::new("grok-4.5")],
        "test-byok-secret-token",
    )
    .await
    .expect("start mock server");

    let mut h = ConfigTestHarness::new(&server);
    std::fs::write(
        h.grok_dir().join("managed_config.toml"),
        format!(
            r#"
[endpoints]
deployment_key = "test-deployment-key"
xai_api_base_url = "{url}"

[model.grok-build]
api_backend = "responses"
base_url = "{url}"
context_window = 500000
env_key = "GROK_TEST_BYOK_TOKEN"
model = "grok-4.5"

[models]
default = "grok-4.5"
"#,
            url = server.url()
        ),
    )
    .unwrap();
    h.env("GROK_TEST_BYOK_TOKEN", "test-byok-secret-token");

    let result = h.run().await;
    assert_headless_success(&result, "managed config BYOK", Some(&server));
    assert_no_crashes(&result.stderr);
    assert!(
        server.has_responses_request(),
        "mock server received no /v1/responses request\n{}",
        server.request_log_summary()
    );
}

/// New-server payload — a `reasoning_efforts` menu plus the legacy
/// `supportsReasoningEffort`/`reasoningEffort` (exactly what CCP emits) — parses
/// without error and the legacy effort scalar still rides the wire. Proves the
/// backwards-compat contract end-to-end through the built binary: the unknown
/// `reasoningEfforts` field never breaks the `/v1/models` parse. On the headless
/// path the wire effort comes from the legacy scalar, not from the list; the
/// list→default derivation is unit-tested in `acp_model_meta_*`.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn headless_reasoning_efforts_payload_parses_and_legacy_effort_rides_wire() {
    let server = MockInferenceServer::start_with_models(vec![
        MockModelEntry::new("grok-4.5")
            .with_api_backend("chat_completions")
            .with_supports_reasoning_effort(true)
            .with_reasoning_effort("xhigh")
            .with_reasoning_efforts(vec![
                serde_json::json!({ "id": "deep", "value": "xhigh", "label": "Deep", "default": true }),
                serde_json::json!({ "id": "balanced", "value": "medium", "label": "Balanced" }),
            ]),
    ])
    .await
    .expect("start mock server");
    server.set_response("done");

    let workdir = git_workdir();
    let result = run_headless(
        &server,
        &[
            "-p",
            "hi",
            "--yolo",
            "--model",
            "grok-4.5",
            "--max-turns",
            "1",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "grok -p reasoning_efforts list", Some(&server));
    assert_no_crashes(&result.stderr);

    // The legacy effort scalar rides the chat-completions request unchanged.
    let effort_on_wire = server.requests().iter().any(|r| {
        r.body.as_ref().is_some_and(|body| {
            body.pointer("/reasoning_effort").and_then(|v| v.as_str()) == Some("xhigh")
        })
    });
    assert!(
        effort_on_wire,
        "legacy reasoning_effort=xhigh must reach the wire\n{}",
        server.request_log_summary()
    );
}

// ============================================================================
// Background-task reaping at headless exit
// ============================================================================

#[cfg(unix)]
use xai_grok_test_support::sse::{
    chat_completions_reasoning_then_tool_call_events, responses_api_reasoning_then_tool_call_events,
};

/// Poll `kill -0 <pid>` until the process is gone or the deadline passes.
#[cfg(unix)]
fn process_dead_within(pid: u32, deadline: Duration) -> bool {
    let start = std::time::Instant::now();
    loop {
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !alive {
            return true;
        }
        if start.elapsed() > deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Read the PID a scripted background task wrote to `pid_file`, waiting for
/// the file to exist (the task writes it as its first action).
#[cfg(unix)]
fn read_task_pid(pid_file: &std::path::Path) -> u32 {
    let start = std::time::Instant::now();
    while !pid_file.exists() && start.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let contents = std::fs::read_to_string(pid_file).unwrap_or_else(|e| {
        panic!(
            "background task never ran: pid file {} unreadable: {e}",
            pid_file.display()
        )
    });
    contents
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("pid file {} held {contents:?}: {e}", pid_file.display()))
}

/// Script one turn that starts an `is_background: true` shell task recording
/// its PID and sleeping far longer than any timeout, followed by a plain-text
/// answer for the post-tool turn.
#[cfg(unix)]
fn enqueue_background_task_turn(server: &MockInferenceServer, pid_file: &std::path::Path) {
    let command = format!("echo $$ > {} && exec /bin/sleep 300", pid_file.display());
    let args = serde_json::json!({
        "command": command,
        "description": "start long-lived background process",
        "is_background": true,
    })
    .to_string();
    server.enqueue_response(
        "/v1/responses",
        ScriptedResponse::sse(responses_api_reasoning_then_tool_call_events(
            "starting a background task",
            "call_bg",
            "run_terminal_command",
            &args,
            "test-model",
        )),
    );
    server.enqueue_response(
        "/v1/chat/completions",
        ScriptedResponse::sse(chat_completions_reasoning_then_tool_call_events(
            "starting a background task",
            "call_bg",
            "run_terminal_command",
            &args,
            "test-model",
        )),
    );
    server.set_response("done");
}

/// Timeout path: a background task outlives `--background-wait-timeout`, so
/// headless exits via the timeout valve — and must kill the task instead of
/// orphaning it.
#[cfg(unix)]
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_timeout_exit_kills_pending_background_task() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = git_workdir();
    let pid_file = workdir.path().join("task_pid.txt");
    enqueue_background_task_turn(&server, &pid_file);

    let result = run_headless(
        &server,
        &[
            "-p",
            "start the server",
            "--yolo",
            "--background-wait-timeout",
            "1",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(
        &result,
        "grok -p with pending background task",
        Some(&server),
    );
    assert_no_crashes(&result.stderr);

    let pid = read_task_pid(&pid_file);
    assert!(
        process_dead_within(pid, Duration::from_secs(5)),
        "background task (pid {pid}) survived headless exit on the timeout path\nstderr:\n{}",
        stderr_tail(&result.stderr, 2000)
    );
}

/// `--no-wait-for-background` path: exit is immediate after the turn, and the
/// task — tracked despite the flag — must still be killed, not leaked.
#[cfg(unix)]
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_no_wait_exit_kills_background_task() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = git_workdir();
    let pid_file = workdir.path().join("task_pid.txt");
    enqueue_background_task_turn(&server, &pid_file);

    let result = run_headless(
        &server,
        &[
            "-p",
            "start the server",
            "--yolo",
            "--no-wait-for-background",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "grok -p --no-wait-for-background", Some(&server));
    assert_no_crashes(&result.stderr);

    let pid = read_task_pid(&pid_file);
    assert!(
        process_dead_within(pid, Duration::from_secs(5)),
        "background task (pid {pid}) survived --no-wait-for-background exit\nstderr:\n{}",
        stderr_tail(&result.stderr, 2000)
    );
}

/// Quiescent path regression guard: a background task that completes on its
/// own is waited for (intended behavior) and the run exits cleanly with
/// nothing reaped.
#[cfg(unix)]
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn test_headless_waits_for_short_background_task_and_exits_clean() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = git_workdir();
    let marker = workdir.path().join("finished.txt");
    let command = format!("/bin/sleep 1 && echo ok > {}", marker.display());
    let args = serde_json::json!({
        "command": command,
        "description": "short background task",
        "is_background": true,
    })
    .to_string();
    server.enqueue_response(
        "/v1/responses",
        ScriptedResponse::sse(responses_api_reasoning_then_tool_call_events(
            "starting a short background task",
            "call_bg_short",
            "run_terminal_command",
            &args,
            "test-model",
        )),
    );
    server.enqueue_response(
        "/v1/chat/completions",
        ScriptedResponse::sse(chat_completions_reasoning_then_tool_call_events(
            "starting a short background task",
            "call_bg_short",
            "run_terminal_command",
            &args,
            "test-model",
        )),
    );
    server.set_response("done");

    let result = run_headless(
        &server,
        &[
            "-p",
            "start it",
            "--yolo",
            "--background-wait-timeout",
            "30",
        ],
        workdir.path(),
    )
    .await;

    assert_headless_success(&result, "grok -p with short background task", Some(&server));
    assert_no_crashes(&result.stderr);
    assert!(
        marker.exists(),
        "short background task did not finish before exit — the intended wait \
         was skipped\nstderr:\n{}",
        stderr_tail(&result.stderr, 2000)
    );
}

/// DS-38: 40 active runtime connectors + 2 rejected inventory-only.
/// Rejected (biogrid, kegg) MUST NOT appear in the runtime fetch registry.
#[test]
fn test_connector_registry_active_40_inventory_42() {
    let registry = xai_grok_science::connectors::registry();
    let rejected = xai_grok_science::connectors::rejected_registry();
    let inventory = xai_grok_science::connectors::inventory();

    assert_eq!(registry.len(), 40, "active runtime registry must be 40");
    assert_eq!(rejected.len(), 2, "rejected inventory must be 2");
    assert_eq!(inventory.len(), 42, "full disposition inventory must be 42");

    let expected_active: std::collections::BTreeSet<&str> = [
        "pubmed",
        "chembl",
        "crossref",
        "uniprot",
        "europepmc",
        "openalex",
        "semantic-scholar",
        "arxiv",
        "biorxiv",
        "rcsb-pdb",
        "pdbe",
        "alphafold",
        "interpro",
        "sifts",
        "pubchem",
        "bindingdb",
        "gtopdb",
        "surechembl",
        "chebi",
        "ensembl",
        "ncbi-gene",
        "dbsnp",
        "clinvar",
        "gnomad",
        "ucsc",
        "mygene",
        "myvariant",
        "reactome",
        "string-db",
        "intact",
        "wikipathways",
        "opentargets",
        "geo",
        "arrayexpress",
        "gtex",
        "hpa",
        "expression-atlas",
        "single-cell-atlas",
        "depmap",
        "eutils",
    ]
    .into_iter()
    .collect();

    let actual_active: std::collections::BTreeSet<&str> = registry.iter().map(|d| d.id).collect();
    assert_eq!(expected_active, actual_active);

    let rejected_ids: std::collections::BTreeSet<&str> = rejected.iter().map(|d| d.id).collect();
    assert_eq!(
        rejected_ids,
        ["biogrid", "kegg"].into_iter().collect(),
        "rejected inventory must be exactly biogrid + kegg"
    );

    // Rejected must not be runtime-resolvable
    assert!(xai_grok_science::connectors::descriptor("biogrid").is_none());
    assert!(xai_grok_science::connectors::descriptor("kegg").is_none());
}

// ============================================================================
// WP-2 project mutations over stdio ACP (LS5-P2-03 E4 proof)
//
// LS5-P2-03 routed project_create / project_transition / claim_propose /
// evidence_attach through the SessionActor, but nothing exercised that route:
// the science-crate unit tests cover the mutation semantics without the actor,
// and every ACP-level science test needs a pre-built binary. Routing a mutation
// through the actor and PROVING it goes through the actor are different claims,
// so the status file deliberately stayed at E2.
//
// These close that gap. They drive the rebuilt binary over the real protocol,
// so they cannot pass unless the shell wiring, the permission bridge and the
// durable ledger all actually work.
// ============================================================================

/// A project mutation is actor-gated end to end, and replaying one operation
/// id returns the first outcome instead of applying it twice.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_project_mutation_is_actor_gated_and_idempotent() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-store");

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let create = |operation_id: &str| {
            serde_json::json!({
                "sessionId": session_id.0.as_ref(),
                "ownerId": "science-owner",
                "storeRoot": store_root,
                "title": "Restriction mapping",
                "researchQuestion": "Where does EcoRI cut?",
                "operationId": operation_id,
                "approvalTimeoutMs": 5_000,
            })
        };

        let first: serde_json::Value = serde_json::from_str(
            tokio::time::timeout(
                Duration::from_secs(30),
                client.ext_method("x.ai/science/project_create", create("op-create-1")),
            )
            .await
            .expect("project_create timed out")
            .unwrap_or_else(|error| {
                panic!(
                    "project_create failed: {error:?}\nstderr:\n{}",
                    client.stderr()
                )
            })
            .0
            .get(),
        )
        .expect("project_create returned JSON");

        // The authority claim is only true because the mutation took the actor
        // route; asserting it here is what stops the string drifting back into
        // decoration.
        assert_eq!(
            first["runtimeAuthority"], "SessionActor-gated ACP adapter",
            "response: {first}"
        );
        assert_eq!(first["replayed"], false, "first apply must not be a replay");
        let project_id = first["projectId"].as_str().expect("projectId").to_owned();
        assert!(!project_id.is_empty());

        // Read authority is owner-scoped too. The creator can reopen and list
        // the project without another permission prompt; a different owner
        // receives neither the bundle nor a project-list disclosure.
        let permissions_after_create = client.permission_request_count();
        let owned_project: serde_json::Value = serde_json::from_str(
            client
                .ext_method(
                    "x.ai/science/project_get",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "science-owner",
                        "storeRoot": store_root,
                        "projectId": project_id.as_str(),
                    }),
                )
                .await
                .expect("owner could not reopen project")
                .0
                .get(),
        )
        .expect("project_get returned JSON");
        assert_eq!(
            owned_project["project"]["project_id"], project_id,
            "project_get returned the wrong project: {owned_project}"
        );
        assert!(
            client
                .ext_method(
                    "x.ai/science/project_get",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "other-owner",
                        "storeRoot": store_root,
                        "projectId": project_id.as_str(),
                    }),
                )
                .await
                .is_err(),
            "foreign owner reopened the project"
        );
        let owned_projects: serde_json::Value = serde_json::from_str(
            client
                .ext_method(
                    "x.ai/science/project_list",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "science-owner",
                        "storeRoot": store_root,
                    }),
                )
                .await
                .expect("owner could not list projects")
                .0
                .get(),
        )
        .expect("project_list returned JSON");
        assert!(
            owned_projects
                .as_array()
                .is_some_and(|projects| projects.iter().any(|project| {
                    project["project_id"] == serde_json::Value::String(project_id.clone())
                })),
            "owned project missing from project_list: {owned_projects}"
        );
        let foreign_projects: serde_json::Value = serde_json::from_str(
            client
                .ext_method(
                    "x.ai/science/project_list",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "other-owner",
                        "storeRoot": store_root,
                    }),
                )
                .await
                .expect("foreign project_list should return an undisclosing empty list")
                .0
                .get(),
        )
        .expect("foreign project_list returned JSON");
        assert_eq!(foreign_projects, serde_json::json!([]));
        assert_eq!(
            client.permission_request_count(),
            permissions_after_create,
            "read-only project queries asked for permission"
        );

        // A newly-created project has no run yet. The desktop still asks for
        // its configured default run when opening it; the Rust list query must
        // return an honest empty list after verifying project ownership, not
        // turn every new project into a seed error.
        let empty: serde_json::Value = serde_json::from_str(
            client
                .ext_method(
                    "x.ai/science/artifact_list",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "science-owner",
                        "projectId": project_id.as_str(),
                        "runId": "default",
                        "storeRoot": store_root,
                    }),
                )
                .await
                .expect("owned project with no run should list empty")
                .0
                .get(),
        )
        .expect("empty artifact list returned JSON");
        assert_eq!(empty, serde_json::json!([]));
        assert_eq!(
            client.permission_request_count(),
            permissions_after_create,
            "read-only empty listing asked for permission"
        );
        assert!(
            client
                .ext_method(
                    "x.ai/science/artifact_list",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "other-owner",
                        "projectId": project_id.as_str(),
                        "runId": "default",
                        "storeRoot": store_root,
                    }),
                )
                .await
                .is_err(),
            "wrong owner received an empty-list grant"
        );
        assert!(
            client
                .ext_method(
                    "x.ai/science/artifact_list",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "science-owner",
                        "projectId": project_id.as_str(),
                        "runId": "forged-missing-run",
                        "storeRoot": store_root,
                    }),
                )
                .await
                .is_err(),
            "arbitrary missing run was treated as the new-project default"
        );
        assert_eq!(
            client.permission_request_count(),
            permissions_after_create,
            "read-only rejected listings asked for permission"
        );

        // Replaying the same operation id must return the SAME project rather
        // than creating a second one. Without the durable operation ledger a
        // retried request silently forks the store.
        let replay: serde_json::Value = serde_json::from_str(
            client
                .ext_method("x.ai/science/project_create", create("op-create-1"))
                .await
                .expect("replay failed")
                .0
                .get(),
        )
        .expect("replay returned JSON");
        assert_eq!(replay["replayed"], true, "replay: {replay}");
        assert_eq!(
            replay["projectId"], project_id,
            "replay created a new project"
        );

        // A different operation id is a different mutation and must create a
        // second project — otherwise the check above would pass trivially.
        let second: serde_json::Value = serde_json::from_str(
            client
                .ext_method("x.ai/science/project_create", create("op-create-2"))
                .await
                .expect("second create failed")
                .0
                .get(),
        )
        .expect("second returned JSON");
        assert_ne!(second["projectId"], serde_json::Value::String(project_id));

        // Reopen the durable store from the test process and confirm exactly
        // two projects exist — the replay must not have left a third.
        let store = xai_grok_science::project::ProjectStore::new(&store_root);
        let projects = store.list_projects().expect("list projects");
        assert_eq!(
            projects.len(),
            2,
            "expected 2 projects after create+replay+create, got {}",
            projects.len()
        );
    })
    .await;
}

/// `operationId` is required, and a denied permission leaves the store empty.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_project_mutation_fails_closed() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-store");

        // 1. Missing operationId must be rejected. Without an idempotency key
        //    a retry cannot be distinguished from a second intentional
        //    mutation, so the field is mandatory rather than defaulted.
        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let missing_op = client
            .ext_method(
                "x.ai/science/project_create",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "title": "No operation id",
                    "researchQuestion": "?",
                }),
            )
            .await;
        assert!(
            missing_op.is_err(),
            "project_create without operationId was accepted: {missing_op:?}"
        );

        // 2. A mutation into a session that does not exist must not reach the
        //    store, even with a well-formed request.
        let wrong_session = client
            .ext_method(
                "x.ai/science/project_create",
                serde_json::json!({
                    "sessionId": "not-a-real-session",
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "title": "Forged session",
                    "researchQuestion": "?",
                    "operationId": "op-forged",
                }),
            )
            .await;
        assert!(
            wrong_session.is_err(),
            "mutation with an unknown session was accepted: {wrong_session:?}"
        );

        // 3. Confinement must happen before directory creation. The old helper
        //    called create_dir_all first and only then noticed the path was
        //    outside the session workspace.
        let outside = tempfile::tempdir().expect("outside root");
        let outside_store = outside.path().join("must-not-be-created");
        let outside_request = client
            .ext_method(
                "x.ai/science/project_create",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": outside_store,
                    "title": "Outside workspace",
                    "researchQuestion": "?",
                    "operationId": "op-outside-root",
                }),
            )
            .await;
        assert!(
            outside_request.is_err(),
            "mutation with an outside store root was accepted: {outside_request:?}"
        );
        assert!(
            !outside_store.exists(),
            "outside store root was created before the request failed"
        );

        // 4. Project mutations have one actual durable run root:
        //    storeRoot/runs. A different artifactRoot would make RunContext
        //    claim one location while ScienceStore writes another.
        let unrelated_artifacts = workdir.path().join("unrelated-artifacts");
        let mismatched_artifact_root = client
            .ext_method(
                "x.ai/science/project_create",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "artifactRoot": unrelated_artifacts,
                    "title": "Mismatched roots",
                    "researchQuestion": "?",
                    "operationId": "op-mismatched-roots",
                }),
            )
            .await;
        assert!(
            mismatched_artifact_root.is_err(),
            "mutation with mismatched store/artifact roots was accepted: {mismatched_artifact_root:?}"
        );
        assert!(
            !unrelated_artifacts.exists(),
            "rejected artifact root was created"
        );
        assert_eq!(
            std::fs::read_dir(store_root.join("runs"))
                .expect("empty canonical run root")
                .count(),
            0,
            "root validation opened a durable run"
        );

        assert!(
            !store_root.join("projects").exists(),
            "a rejected mutation created durable state"
        );
    })
    .await;
}

/// The desktop intentionally sends `storeRoot: "science-store"`. A safe
/// relative path binds to the session workspace; rejecting every relative path
/// would make source-level confinement green while breaking the real product.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_project_mutation_relative_store_is_workspace_bound() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let response: serde_json::Value = serde_json::from_str(
            client
                .ext_method(
                    "x.ai/science/project_create",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "science-owner",
                        "storeRoot": "science-store",
                        "title": "Relative desktop store",
                        "researchQuestion": "Is this bound to the session workspace?",
                        "operationId": "op-relative-store",
                        "approvalTimeoutMs": 5_000,
                    }),
                )
                .await
                .expect("safe relative project mutation failed")
                .0
                .get(),
        )
        .expect("relative project mutation returned JSON");
        assert_eq!(
            response["runtimeAuthority"],
            "SessionActor-gated ACP adapter"
        );
        let store =
            xai_grok_science::project::ProjectStore::new(workdir.path().join("science-store"));
        assert_eq!(
            store
                .list_projects()
                .expect("relative store projects")
                .len(),
            1
        );
    })
    .await;
}

/// A denied permission must abort the mutation and leave no project behind.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_project_mutation_denied_writes_nothing() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-store");

        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            workdir.path(),
            PermissionResponse::Reject,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let denied = client
            .ext_method(
                "x.ai/science/project_create",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "title": "Denied",
                    "researchQuestion": "?",
                    "operationId": "op-denied",
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;

        assert!(
            denied.is_err(),
            "a denied permission still returned success: {denied:?}"
        );
        // The point of routing through the actor is that the permission
        // decision gates the WRITE, not just the response.
        let store = xai_grok_science::project::ProjectStore::new(&store_root);
        let projects = store.list_projects().unwrap_or_default();
        assert!(
            projects.is_empty(),
            "denied mutation persisted {} project(s)",
            projects.len()
        );
    })
    .await;
}

/// Operator feature gates are captured once per session and enforced on both
/// read routes and actor-owned mutations. Re-enabling the feature on disk
/// after session creation must not widen that already-running session.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_operator_gate_snapshot_denies_read_and_mutation_before_admission() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-disabled-store");
        let home = tempfile::TempDir::new().expect("create temp home");
        let lumen_dir = home.path().join(".lumen");
        std::fs::create_dir_all(&lumen_dir).expect("create isolated Lumen home");
        std::fs::write(
            lumen_dir.join("config.toml"),
            concat!(
                "[science_features]\n",
                "research_project = \"disabled\"\n",
                "workflow_dag = \"disabled\"\n",
            ),
        )
        .expect("write disabled science feature config");

        let client = GrokStdioClient::spawn_with_home(&server, workdir.path(), home).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let artifact_read = client
            .ext_method(
                "x.ai/science/artifact_list",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "projectId": "disabled-project",
                    "runId": "default",
                    "storeRoot": store_root,
                }),
            )
            .await;
        let artifact_error = format!(
            "{:?}",
            artifact_read.expect_err("disabled artifact_list was accepted")
        );
        assert!(
            artifact_error.contains("feature disabled: research_project"),
            "artifact_list failed for the wrong reason: {artifact_error}"
        );
        assert!(
            !store_root.exists(),
            "disabled read-only artifact_list created its store root"
        );

        let read = client
            .ext_method(
                "x.ai/science/project_list",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                }),
            )
            .await;
        let read_error = format!(
            "{:?}",
            read.expect_err("disabled project_list was accepted")
        );
        assert!(
            read_error.contains("feature disabled: research_project"),
            "project_list failed for the wrong reason: {read_error}"
        );

        // A config watcher may observe this change, but the existing session
        // must retain the disabled snapshot it was born with.
        std::fs::write(
            client.home_path().join(".lumen/config.toml"),
            concat!(
                "[science_features]\n",
                "research_project = \"preview\"\n",
                "workflow_dag = \"disabled\"\n",
            ),
        )
        .expect("re-enable feature on disk");

        let mutation = client
            .ext_method(
                "x.ai/science/project_create",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "title": "Must stay disabled",
                    "researchQuestion": "Can a config reload widen this session?",
                    "operationId": "op-disabled-gate",
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        let mutation_error = format!(
            "{:?}",
            mutation.expect_err("disabled project_create was accepted")
        );
        assert!(
            mutation_error.contains("feature disabled: research_project"),
            "project_create failed for the wrong reason: {mutation_error}"
        );

        let workflow = client
            .ext_method(
                "x.ai/science/workflow_execute",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "operationId": "op-disabled-workflow",
                    "workflowSpec": workflow_spec("wf-disabled-gate", WORKFLOW_CELL),
                    // The actor must reject on its gate snapshot before this
                    // executable is probed or run.
                    "interpreterPath": std::env::current_exe()
                        .expect("resolve inert absolute executable"),
                    "allowKernelSteps": true,
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        let workflow_error = format!(
            "{:?}",
            workflow.expect_err("disabled workflow_execute was accepted")
        );
        assert!(
            workflow_error.contains("feature disabled: workflow_dag"),
            "workflow_execute failed for the wrong reason: {workflow_error}"
        );
        assert_eq!(
            client.permission_request_count(),
            0,
            "disabled feature reached the permission broker"
        );
        let run_root = store_root.join("runs");
        let run_count = if run_root.is_dir() {
            std::fs::read_dir(&run_root)
                .expect("read empty run root")
                .count()
        } else {
            0
        };
        assert_eq!(
            run_count, 0,
            "disabled feature opened a durable run before rejection"
        );
        assert!(
            !store_root.join("projects").exists(),
            "disabled feature created a project before rejection"
        );
        for name in [
            "workflow-runs",
            "workflow-operations",
            "workflow-commits",
            "workflow-cells",
            "workflow-outputs",
            "workflow-runtime",
        ] {
            let path = store_root.join(name);
            let count = std::fs::read_dir(&path).map(Iterator::count).unwrap_or(0);
            assert_eq!(
                count, 0,
                "disabled workflow left {count} durable entries in {name}"
            );
        }
    })
    .await;
}

/// The legacy migration endpoint must use the typed project-mutation seam:
/// one permission, one durable run, one idempotent project-store mutation.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_project_migrate_is_actor_gated_and_idempotent() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-migration-store");

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let params = || {
            serde_json::json!({
                "sessionId": session_id.0.as_ref(),
                "ownerId": "science-owner",
                "storeRoot": store_root,
                "runId": "legacy-run-42",
                "title": "Migrated restriction study",
                "question": "Which V1 evidence remains valid?",
                "operationId": "op-migrate-0001",
                "approvalTimeoutMs": 5_000,
            })
        };

        let first: serde_json::Value = serde_json::from_str(
            client
                .ext_method("x.ai/science/project_migrate", params())
                .await
                .expect("project_migrate failed")
                .0
                .get(),
        )
        .expect("project_migrate returned JSON");
        assert_eq!(
            first["runtimeAuthority"], "SessionActor-gated ACP adapter",
            "response: {first}"
        );
        assert_eq!(first["source_run_id"], "legacy-run-42");
        assert_eq!(first["replayed"], false);
        let project_id = xai_grok_science::project::ProjectId(
            first["target_project_id"]
                .as_str()
                .expect("target project id")
                .to_owned(),
        );

        let project_store = xai_grok_science::project::ProjectStore::new(&store_root);
        let project = project_store
            .load_project(&project_id)
            .expect("reopen migrated project");
        assert_eq!(project.owner_id.0, "science-owner");
        assert!(project.sessions.contains(&"legacy-run-42".to_string()));
        assert_eq!(project_store.list_projects().unwrap().len(), 1);

        let runs = std::fs::read_dir(store_root.join("runs"))
            .expect("durable migration run")
            .collect::<Result<Vec<_>, _>>()
            .expect("read migration runs");
        assert_eq!(runs.len(), 1);
        let run_id = xai_grok_science::RunId::new(
            runs[0]
                .file_name()
                .to_str()
                .expect("UTF-8 run id")
                .to_owned(),
        );
        let science_store = xai_grok_science::ScienceStore::new(&store_root);
        assert_eq!(
            science_store.load_run(&run_id).expect("load run").state,
            xai_grok_science::RunState::Succeeded
        );
        assert_eq!(
            science_store.approvals(&run_id).expect("approval")[0].decision,
            xai_grok_science::ApprovalDecision::Allow
        );

        let replay: serde_json::Value = serde_json::from_str(
            client
                .ext_method("x.ai/science/project_migrate", params())
                .await
                .expect("migration replay failed")
                .0
                .get(),
        )
        .expect("migration replay returned JSON");
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["target_project_id"], first["target_project_id"]);
        assert_eq!(project_store.list_projects().unwrap().len(), 1);
        assert_eq!(
            std::fs::read_dir(store_root.join("runs"))
                .expect("migration runs after replay")
                .count(),
            1,
            "idempotent replay opened a second durable run"
        );
    })
    .await;
}

/// A refused migration may record the refusal, but it cannot create a project
/// or burn the operation id.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_project_migrate_refusal_writes_no_project() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-migration-store");
        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            workdir.path(),
            PermissionResponse::Reject,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let missing_operation = client
            .ext_method(
                "x.ai/science/project_migrate",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "runId": "legacy-run-unkeyed",
                    "title": "Must not exist",
                    "question": "Where is the idempotency key?",
                }),
            )
            .await;
        assert!(
            missing_operation.is_err(),
            "migration without operationId succeeded: {missing_operation:?}"
        );
        assert!(
            !store_root.join("runs").exists(),
            "unkeyed migration opened a durable run"
        );

        let refused = client
            .ext_method(
                "x.ai/science/project_migrate",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "runId": "legacy-run-denied",
                    "title": "Must not exist",
                    "question": "Was this denied?",
                    "operationId": "op-migrate-denied",
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        assert!(refused.is_err(), "refused migration succeeded: {refused:?}");

        let project_store = xai_grok_science::project::ProjectStore::new(&store_root);
        assert!(project_store.list_projects().unwrap_or_default().is_empty());
        assert!(
            !store_root.join("operations").exists(),
            "refused migration burned its operation id"
        );
        let runs = std::fs::read_dir(store_root.join("runs"))
            .expect("refusal must be durable")
            .collect::<Result<Vec<_>, _>>()
            .expect("read refused migration runs");
        assert_eq!(runs.len(), 1);
        let run_id = xai_grok_science::RunId::new(
            runs[0]
                .file_name()
                .to_str()
                .expect("UTF-8 run id")
                .to_owned(),
        );
        let science_store = xai_grok_science::ScienceStore::new(&store_root);
        assert_eq!(
            science_store
                .load_run(&run_id)
                .expect("load refused run")
                .state,
            xai_grok_science::RunState::Cancelled
        );
    })
    .await;
}

/// Disabling only `migration_chain` must stop the legacy endpoint before a
/// durable authority run, permission prompt, project, or operation is opened.
/// `research_project` intentionally remains at its compiled preview default.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_project_migrate_requires_migration_chain_gate() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-migration-disabled-store");
        let home = tempfile::TempDir::new().expect("create temp home");
        let lumen_dir = home.path().join(".lumen");
        std::fs::create_dir_all(&lumen_dir).expect("create isolated Lumen home");
        std::fs::write(
            lumen_dir.join("config.toml"),
            "[science_features]\nmigration_chain = \"disabled\"\n",
        )
        .expect("write migration-only disabled config");

        let client = GrokStdioClient::spawn_with_home(&server, workdir.path(), home).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;
        let rejected = client
            .ext_method(
                "x.ai/science/project_migrate",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "runId": "legacy-run-disabled",
                    "title": "Must not exist",
                    "question": "Did the migration-only gate stop this?",
                    "operationId": "op-migrate-disabled",
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await
            .expect_err("migration_chain-disabled request was accepted");
        let error = format!("{rejected:?}");
        assert!(
            error.contains("feature disabled: migration_chain"),
            "migration was rejected for the wrong reason: {error}"
        );
        assert_eq!(
            client.permission_request_count(),
            0,
            "disabled migration reached the permission seam"
        );
        assert!(
            !store_root.join("runs").exists(),
            "disabled migration opened a durable run"
        );
        assert!(
            !store_root.join("projects").exists(),
            "disabled migration created a project"
        );
        assert!(
            !store_root.join("operations").exists(),
            "disabled migration burned its operation id"
        );
    })
    .await;
}

/// The shipped review endpoint must not be a preview echo. It reopens a
/// succeeded source run after permission, hashes the registered bytes, writes
/// one durable project review, and commits a manifest/evidence/provenance
/// chain in the actor's own run. Replaying the operation prompts and writes
/// nothing a second time.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_review_record_is_actor_gated_artifact_bound_and_idempotent() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let workspace = std::fs::canonicalize(workdir.path()).expect("canonical workspace");
        let store_root = workspace.join("science-review-store");
        let client = GrokStdioClient::spawn(&server, &workspace).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(&workspace).await;
        let created: serde_json::Value = serde_json::from_str(
            client
                .ext_method(
                    "x.ai/science/project_create",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "review-owner",
                        "storeRoot": store_root,
                        "title": "Artifact-bound review",
                        "researchQuestion": "Do these exact bytes support the result?",
                        "operationId": "op-review-project-create",
                        "approvalTimeoutMs": 5_000,
                    }),
                )
                .await
                .expect("create review project")
                .0
                .get(),
        )
        .expect("project create JSON");
        let project_id = created["projectId"]
            .as_str()
            .expect("project id")
            .to_owned();

        // Seed a genuine succeeded ScienceStore run. The review request cites
        // only its run id + digest; Rust must resolve and rehash the bytes.
        let science_store = xai_grok_science::ScienceStore::new(&store_root);
        let source_run = xai_grok_science::RunId::new("review-source-run-1");
        science_store
            .create_run(xai_grok_science::RunContext {
                run_id: source_run.clone(),
                project_id: xai_grok_science::ProjectId::new(project_id.clone()),
                session_id: session_id.0.to_string(),
                owner_id: "review-owner".into(),
                workspace_root: workspace.clone(),
                provider: "offline-test".into(),
                approval_policy: "fixture".into(),
                tool_profile: "review-source-fixture".into(),
                artifact_root: store_root.join("runs"),
                environment: std::collections::BTreeMap::new(),
            })
            .expect("create source run");
        let source_artifact = science_store
            .put_artifact(
                &xai_grok_science::ProjectId::new(project_id.clone()),
                &source_run,
                "review-owner",
                xai_grok_science::CallId::new("source-call"),
                Path::new("result.json"),
                br#"{"answer":"supported"}"#,
                "application/json",
                "review source",
            )
            .expect("put source artifact");
        science_store
            .transition(&source_run, xai_grok_science::RunState::Succeeded, None)
            .expect("finish source run");

        let params = || {
            serde_json::json!({
                "sessionId": session_id.0.as_ref(),
                "ownerId": "review-owner",
                "storeRoot": store_root,
                "projectId": project_id,
                "reviewerId": "review-owner",
                "verdict": "pass",
                "summary": "The exact source bytes support the recorded fixture conclusion.",
                "runId": source_run.0,
                "artifactSha256s": [source_artifact.sha256],
                "operationId": "op-review-record-0001",
                "approvalTimeoutMs": 5_000,
            })
        };
        let run_names = || {
            std::fs::read_dir(store_root.join("runs"))
                .expect("read runs")
                .map(|entry| {
                    entry
                        .expect("run entry")
                        .file_name()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<std::collections::BTreeSet<_>>()
        };

        let runs_before_forged_reviewer = run_names();
        let permissions_before_forged_reviewer = client.permission_request_count();
        let mut forged_reviewer = params();
        forged_reviewer["reviewerId"] = serde_json::json!("Nature-Reviewer-2");
        assert!(
            client
                .ext_method("x.ai/science/review_record", forged_reviewer)
                .await
                .is_err(),
            "untrusted reviewer attribution was accepted"
        );
        assert_eq!(
            client.permission_request_count(),
            permissions_before_forged_reviewer,
            "an impossible reviewer identity reached the permission prompt"
        );
        assert_eq!(
            run_names(),
            runs_before_forged_reviewer,
            "forged reviewer identity opened an authority run"
        );

        let before = run_names();
        let permissions_before = client.permission_request_count();
        let response: serde_json::Value = serde_json::from_str(
            client
                .ext_method("x.ai/science/review_record", params())
                .await
                .expect("review_record failed")
                .0
                .get(),
        )
        .expect("review response JSON");
        assert_eq!(
            response["runtimeAuthority"],
            "SessionActor-gated ACP adapter"
        );
        assert_eq!(response["kind"], "review_record");
        assert_eq!(response["replayed"], false);
        assert_eq!(response["result"]["project_id"], project_id);
        assert_eq!(response["result"]["source_run_id"], source_run.0);
        assert_eq!(
            response["result"]["artifacts"][0]["sha256"],
            source_artifact.sha256
        );
        assert_eq!(
            client.permission_request_count(),
            permissions_before + 1,
            "review must ask exactly once"
        );

        let project_store = xai_grok_science::project::ProjectStore::new(&store_root);
        let project_id_typed = xai_grok_science::project::ProjectId(project_id.clone());
        let reviews = project_store
            .list_reviews(&project_id_typed)
            .expect("reopen reviews");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].operation_id, "op-review-record-0001");

        let after = run_names();
        let review_runs: Vec<_> = after.difference(&before).cloned().collect();
        assert_eq!(review_runs.len(), 1, "review must open one authority run");
        let authority_run = xai_grok_science::RunId::new(review_runs[0].clone());
        assert_eq!(
            response["result"]["authority_run_id"], authority_run.0,
            "review record must bind the actor run that authorized it"
        );
        assert_eq!(
            reviews[0].authority_run_id, authority_run.0,
            "reopened review lost its authority-run binding"
        );
        assert_eq!(
            science_store
                .load_run(&authority_run)
                .expect("load authority run")
                .state,
            xai_grok_science::RunState::Succeeded
        );
        assert_eq!(
            science_store.approvals(&authority_run).expect("approval")[0].decision,
            xai_grok_science::ApprovalDecision::Allow
        );
        let manifests = science_store.artifacts(&authority_run).expect("manifest");
        assert_eq!(manifests.len(), 1);
        assert_eq!(manifests[0].relative_path, Path::new("review_record.json"));
        assert_eq!(science_store.evidence(&authority_run).unwrap().len(), 1);
        assert_eq!(science_store.provenance(&authority_run).unwrap().len(), 1);
        let verified =
            xai_grok_science::review::verify_for_goal_completion(&science_store, &authority_run)
                .expect("host-verify review authority run");
        assert_eq!(verified.artifact_count, 1);
        assert_eq!(verified.evidence_count, 1);
        assert_eq!(verified.provenance_count, 1);
        let manifest_bytes = science_store
            .artifact_bytes(
                &xai_grok_science::ProjectId::new(project_id.clone()),
                &authority_run,
                "review-owner",
                Path::new("review_record.json"),
            )
            .expect("reopen manifest bytes");
        assert_eq!(
            format!("{:x}", Sha256::digest(&manifest_bytes)),
            manifests[0].sha256
        );

        let permissions_after = client.permission_request_count();
        let runs_after = run_names();
        let replay: serde_json::Value = serde_json::from_str(
            client
                .ext_method("x.ai/science/review_record", params())
                .await
                .expect("review replay failed")
                .0
                .get(),
        )
        .expect("review replay JSON");
        assert_eq!(replay["replayed"], true);
        assert_eq!(client.permission_request_count(), permissions_after);
        assert_eq!(run_names(), runs_after, "review replay opened another run");
        assert_eq!(
            project_store.list_reviews(&project_id_typed).unwrap().len(),
            1,
            "review replay duplicated the ledger"
        );

        // A new operation against tampered source bytes reaches Allow but must
        // fail the actor run and leave the review ledger unchanged.
        std::fs::write(
            store_root
                .join("runs")
                .join(&source_run.0)
                .join("artifacts/result.json"),
            b"tampered",
        )
        .expect("tamper source bytes");
        let mut tampered = params();
        tampered["operationId"] = serde_json::json!("op-review-record-tampered");
        let before_failed = run_names();
        assert!(
            client
                .ext_method("x.ai/science/review_record", tampered)
                .await
                .is_err(),
            "tampered source bytes produced a review"
        );
        assert!(
            project_store.list_reviews(&project_id_typed).is_err(),
            "a review over tampered source bytes remained readable as valid"
        );
        let review_files = std::fs::read_dir(
            store_root
                .join("projects")
                .join(&project_id)
                .join("reviews"),
        )
        .expect("read review ledger")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|ext| ext.to_str()) == Some("json"))
        .count();
        assert_eq!(
            review_files, 1,
            "tampered retry wrote a duplicate project review"
        );
        let after_failed = run_names();
        let failed_runs: Vec<_> = after_failed.difference(&before_failed).cloned().collect();
        assert_eq!(failed_runs.len(), 1);
        let failed_run = xai_grok_science::RunId::new(failed_runs[0].clone());
        assert_eq!(
            science_store.load_run(&failed_run).unwrap().state,
            xai_grok_science::RunState::Failed
        );
        assert!(science_store.artifacts(&failed_run).unwrap().is_empty());
    })
    .await;
}

/// A production refusal is durable, but it must not create a review,
/// operation ledger entry, manifest, evidence, or provenance.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_review_record_denied_writes_no_review_or_artifact() {
    with_local_set(|| async {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let workspace = std::fs::canonicalize(workdir.path()).expect("canonical workspace");
        let store_root = workspace.join("science-review-denied-store");
        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            &workspace,
            PermissionResponse::Reject,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(&workspace).await;

        let project_store = xai_grok_science::project::ProjectStore::new(&store_root);
        let project = project_store
            .create_project("review-owner", "Denied review", "Must it write nothing?")
            .expect("seed project");
        let science_store = xai_grok_science::ScienceStore::new(&store_root);
        let source_run = xai_grok_science::RunId::new("review-denied-source");
        science_store
            .create_run(xai_grok_science::RunContext {
                run_id: source_run.clone(),
                project_id: xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                session_id: session_id.0.to_string(),
                owner_id: "review-owner".into(),
                workspace_root: workspace.clone(),
                provider: "offline-test".into(),
                approval_policy: "fixture".into(),
                tool_profile: "review-source-fixture".into(),
                artifact_root: store_root.join("runs"),
                environment: std::collections::BTreeMap::new(),
            })
            .unwrap();
        let artifact = science_store
            .put_artifact(
                &xai_grok_science::ProjectId::new(project.project_id.0.clone()),
                &source_run,
                "review-owner",
                xai_grok_science::CallId::new("source-call"),
                Path::new("result.txt"),
                b"source bytes",
                "text/plain",
                "source",
            )
            .unwrap();
        science_store
            .transition(&source_run, xai_grok_science::RunState::Succeeded, None)
            .unwrap();

        let denied = client
            .ext_method(
                "x.ai/science/review_record",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "review-owner",
                    "storeRoot": store_root,
                    "projectId": project.project_id.0,
                    "reviewerId": "review-owner",
                    "verdict": "pass",
                    "summary": "This request must be denied before any review evidence is written.",
                    "runId": source_run.0,
                    "artifactSha256s": [artifact.sha256],
                    "operationId": "op-review-denied-0001",
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        assert!(
            denied.is_err(),
            "denied review returned success: {denied:?}"
        );
        assert!(
            project_store
                .list_reviews(&project.project_id)
                .unwrap()
                .is_empty()
        );
        assert!(
            project_store
                .lookup_operation("op-review-denied-0001")
                .unwrap()
                .is_none()
        );
        assert_eq!(client.permission_request_count(), 1);

        let run_ids: Vec<_> = std::fs::read_dir(store_root.join("runs"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|id| id != &source_run.0)
            .collect();
        assert_eq!(
            run_ids.len(),
            1,
            "denial must have one durable authority run"
        );
        let denied_run = xai_grok_science::RunId::new(run_ids[0].clone());
        let denied_record = science_store.load_run(&denied_run).unwrap();
        assert!(matches!(
            denied_record.state,
            xai_grok_science::RunState::Denied | xai_grok_science::RunState::Cancelled
        ));
        assert!(science_store.artifacts(&denied_run).unwrap().is_empty());
        assert!(science_store.evidence(&denied_run).unwrap().is_empty());
        assert!(science_store.provenance(&denied_run).unwrap().is_empty());
    })
    .await;
}

/// Kernel identity probing is execution: the rebuilt product must route it
/// through the SessionActor and production permission seam before hashing or
/// spawning the interpreter, then commit its assessment to the Science store.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_kernel_admission_is_actor_gated_and_store_owned() {
    let Some(python) = workflow_python3() else {
        panic!("no python3 on PATH: kernel admission must probe a real interpreter");
    };
    with_local_set(|| async move {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let workspace = std::fs::canonicalize(workdir.path()).expect("canonical workspace");
        let store_root = workspace.join("science-kernel-store");
        let client = GrokStdioClient::spawn(&server, &workspace).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(&workspace).await;
        let created: serde_json::Value = serde_json::from_str(
            client
                .ext_method(
                    "x.ai/science/project_create",
                    serde_json::json!({
                        "sessionId": session_id.0.as_ref(),
                        "ownerId": "science-owner",
                        "storeRoot": store_root,
                        "title": "Kernel admission project",
                        "researchQuestion": "Can the actor admit this local interpreter?",
                        "operationId": "op-kernel-project-create",
                        "approvalTimeoutMs": 5_000,
                    }),
                )
                .await
                .expect("actor-gated project_create failed")
                .0
                .get(),
        )
        .expect("project_create returned JSON");
        let project_id = created["projectId"]
            .as_str()
            .expect("project_create returned projectId")
            .to_owned();
        assert_eq!(client.permission_request_count(), 1);
        let runs_before_boundaries = std::fs::read_dir(store_root.join("runs"))
            .expect("project create must leave an authority run")
            .count();

        // Syntax and session boundaries must fail before a durable run or
        // permission prompt. This rejects an ACP-only probe bypass.
        for (label, params) in [
            (
                "relative interpreter",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "projectId": project_id.clone(),
                    "storeRoot": store_root,
                    "kernelId": "python-relative",
                    "kind": "python",
                    "interpreterPath": "python3",
                }),
            ),
            (
                "unknown session",
                serde_json::json!({
                    "sessionId": "forged-session",
                    "ownerId": "science-owner",
                    "projectId": project_id.clone(),
                    "storeRoot": store_root,
                    "kernelId": "python-forged",
                    "kind": "python",
                    "interpreterPath": python,
                }),
            ),
        ] {
            let error = client
                .ext_method("x.ai/science/kernel_admission", params)
                .await
                .expect_err("invalid kernel admission was accepted");
            assert_ne!(
                error.code,
                acp::ErrorCode::MethodNotFound,
                "{label} reached an unwired endpoint"
            );
        }
        assert_eq!(client.permission_request_count(), 1);
        assert_eq!(
            std::fs::read_dir(store_root.join("runs")).unwrap().count(),
            runs_before_boundaries,
            "invalid kernel admission opened another durable run"
        );

        let response = tokio::time::timeout(
            Duration::from_secs(60),
            client.ext_method(
                "x.ai/science/kernel_admission",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "projectId": project_id.clone(),
                    "storeRoot": store_root,
                    "kernelId": "python-product",
                    "kind": "python",
                    "interpreterPath": python,
                    "probeTimeoutMs": 30_000,
                    "approvalTimeoutMs": 5_000,
                }),
            ),
        )
        .await
        .expect("kernel admission timed out")
        .unwrap_or_else(|error| {
            panic!(
                "kernel admission failed: {error:?}\nstderr:\n{}",
                client.stderr()
            )
        });
        let result: serde_json::Value =
            serde_json::from_str(response.0.get()).expect("kernel admission returned JSON");
        assert_eq!(result["runtimeAuthority"], "SessionActor-gated ACP adapter");
        assert_eq!(result["state"], "succeeded", "result: {result}");
        assert_eq!(
            result["admission"]["admission_status"], "Admitted",
            "result: {result}"
        );
        assert!(
            result["admission"]["executable_hash"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64),
            "missing probed executable hash: {result}"
        );
        assert_eq!(result["artifacts"].as_array().map(Vec::len), Some(1));
        assert_eq!(result["evidence"].as_array().map(Vec::len), Some(1));
        assert_eq!(result["provenance"].as_array().map(Vec::len), Some(1));
        assert_eq!(result["approvals"][0]["decision"], "allow");
        assert_eq!(client.permission_request_count(), 2);

        let run_id = xai_grok_science::RunId::new(
            result["runId"].as_str().expect("durable authority run id"),
        );
        let store = xai_grok_science::ScienceStore::new(&store_root);
        assert_eq!(
            store.load_run(&run_id).expect("reopen kernel run").state,
            xai_grok_science::RunState::Succeeded
        );
        let artifacts = store.artifacts(&run_id).expect("reopen artifacts");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(
            artifacts[0].relative_path,
            Path::new("kernel-admission.json")
        );
        let bytes = store
            .artifact_bytes(
                &xai_grok_science::ProjectId::new(project_id),
                &run_id,
                "science-owner",
                &artifacts[0].relative_path,
            )
            .expect("reopen registered kernel artifact");
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), artifacts[0].sha256);
        assert!(
            !store_root.join("kernel-admission.json").exists(),
            "kernel admission wrote a loose artifact outside the run store"
        );
    })
    .await;
}

/// Cancelling the real permission prompt must close the durable authority run
/// without probing the interpreter or registering any scientific output.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_kernel_admission_denied_writes_nothing() {
    let Some(python) = workflow_python3() else {
        panic!("no python3 on PATH: kernel admission must name a real interpreter");
    };
    with_local_set(|| async move {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let workspace = std::fs::canonicalize(workdir.path()).expect("canonical workspace");
        let store_root = workspace.join("science-kernel-denied-store");
        let creator = GrokStdioClient::spawn(&server, &workspace).await;
        creator.initialize_with_timeout().await;
        let creator_session = creator.create_session_with_timeout(&workspace).await;
        let created: serde_json::Value = serde_json::from_str(
            creator
                .ext_method(
                    "x.ai/science/project_create",
                    serde_json::json!({
                        "sessionId": creator_session.0.as_ref(),
                        "ownerId": "science-owner",
                        "storeRoot": store_root,
                        "title": "Denied kernel admission project",
                        "researchQuestion": "Must denial leave no kernel output?",
                        "operationId": "op-kernel-denied-project-create",
                        "approvalTimeoutMs": 5_000,
                    }),
                )
                .await
                .expect("actor-gated project_create failed")
                .0
                .get(),
        )
        .expect("project_create returned JSON");
        let project_id = created["projectId"]
            .as_str()
            .expect("project_create returned projectId")
            .to_owned();
        assert_eq!(creator.permission_request_count(), 1);
        drop(creator);

        let runs_before: std::collections::BTreeSet<_> = std::fs::read_dir(store_root.join("runs"))
            .expect("project create must leave an authority run")
            .map(|entry| entry.unwrap().file_name())
            .collect();
        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            &workspace,
            PermissionResponse::Reject,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(&workspace).await;
        let denied = client
            .ext_method(
                "x.ai/science/kernel_admission",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "projectId": project_id,
                    "storeRoot": store_root,
                    "kernelId": "python-denied",
                    "kind": "python",
                    "interpreterPath": python,
                    "probeTimeoutMs": 30_000,
                    "approvalTimeoutMs": 5_000,
                }),
            )
            .await;
        assert!(
            denied.is_err(),
            "denied kernel admission returned success: {denied:?}"
        );
        assert_eq!(client.permission_request_count(), 1);

        let runs_after: std::collections::BTreeSet<_> = std::fs::read_dir(store_root.join("runs"))
            .expect("denial must leave a durable run")
            .map(|entry| entry.unwrap().file_name())
            .collect();
        let denied_runs: Vec<_> = runs_after.difference(&runs_before).collect();
        assert_eq!(denied_runs.len(), 1);
        let run_id =
            xai_grok_science::RunId::new(denied_runs[0].to_str().expect("UTF-8 run id").to_owned());
        let store = xai_grok_science::ScienceStore::new(&store_root);
        assert_eq!(
            store.load_run(&run_id).expect("reopen denied run").state,
            xai_grok_science::RunState::Cancelled
        );
        assert!(store.artifacts(&run_id).expect("artifacts").is_empty());
        assert!(store.evidence(&run_id).expect("evidence").is_empty());
        assert!(store.provenance(&run_id).expect("provenance").is_empty());
        assert!(
            !store_root.join("kernel-admission.json").exists(),
            "denied kernel admission wrote a loose artifact"
        );
    })
    .await;
}

// ============================================================================
// LS5-K8: workflow execution over stdio ACP
//
// A previous attempt at this endpoint was declined, and correctly: with no
// `StepRunner` implementation every step would have failed with
// `NoStepRunnerBound`, and an endpoint that cannot succeed is worse than none.
// `PythonLoopRunner` closed that gap, so the endpoint can now be proven rather
// than asserted.
//
// These drive the REAL binary over the real protocol against a REAL python3.
// They cannot pass unless the ACP adapter, the SessionCommand seam, the
// permission bridge, the kernel probe, the exec-loop driver and the durable
// ledger all work together — which is the whole claim.
// ============================================================================

/// A real python3, or the test does not run. A stub interpreter would prove
/// only that a stub was called.
fn workflow_python3() -> Option<PathBuf> {
    let out = Command::new("sh")
        .args(["-c", "command -v python3"])
        .output()
        .ok()?;
    let path = PathBuf::from(String::from_utf8(out.stdout).ok()?.trim());
    path.is_absolute().then_some(path)
}

const WORKFLOW_CELL: &str = "import os\n\
     p = os.path.join(os.environ['LUMEN_KERNEL_OUTPUT_DIR'], 'result.json')\n\
     open(p, 'w').write('{\"mean\": 1.5}')\n\
     print('acp-computed')\n";

fn workflow_spec(workflow_id: &str, cell: &str) -> Value {
    serde_json::json!({
        "workflow_id": workflow_id,
        "project_id": "proj-acp-workflow",
        "name": "acp workflow execution",
        "steps": [{
            "step_id": "compute",
            "kind": "NotebookCell",
            "connector_id": null,
            "notebook_cell": cell,
            "inputs": [],
            "parameters": {},
            "timeout_secs": 120,
            "retry_policy": null,
            "cache_policy": "NoCache",
            "acceptance_rules": []
        }],
        "parameters": {},
        "permissions": [],
        "resources": {
            "max_concurrent_steps": 1,
            "max_total_duration_secs": 3600,
            "max_memory_mb": 1024,
            "max_disk_mb": 1024
        },
        "schema_version": 1
    })
}

/// Per-attempt output directories under `<outputs>/<runId>/<stepId>`. One
/// directory means the kernel ran once — the most direct physical evidence
/// there is that a replay did not execute a second time.
fn attempt_dirs(output_root: &Path, run_id: &str, step_id: &str) -> Vec<String> {
    let dir = output_root.join(run_id).join(step_id);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A workflow executes through the SessionActor, and replaying its operation
/// id returns the recorded outcome without running the kernel a second time.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_workflow_execute_is_actor_gated_and_idempotent() {
    let Some(python) = workflow_python3() else {
        panic!(
            "no python3 on PATH: this test proves a real interpreter runs, so it cannot be skipped into a pass"
        );
    };
    with_local_set(|| async move {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-store");

        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let params = |operation_id: &str| {
            serde_json::json!({
                "sessionId": session_id.0.as_ref(),
                "ownerId": "science-owner",
                "storeRoot": store_root,
                "operationId": operation_id,
                "workflowSpec": workflow_spec("wf-acp-exec", WORKFLOW_CELL),
                "interpreterPath": python,
                "kernelId": "py-acp-exec",
                // The opt-in that `ExecutionPolicy::default()` deliberately
                // withholds. Visible in the request, exactly as intended.
                "allowKernelSteps": true,
                "probeTimeoutMs": 60_000,
                "approvalTimeoutMs": 30_000,
            })
        };

        let first: Value = serde_json::from_str(
            tokio::time::timeout(
                Duration::from_secs(120),
                client.ext_method("x.ai/science/workflow_execute", params("op-wf-exec-1")),
            )
            .await
            .expect("workflow_execute timed out")
            .unwrap_or_else(|error| {
                panic!(
                    "workflow_execute failed: {error:?}\nstderr:\n{}",
                    client.stderr()
                )
            })
            .0
            .get(),
        )
        .expect("workflow_execute returned JSON");

        // 1. It succeeded, and it says by whose authority.
        assert_eq!(
            first["runtimeAuthority"], "SessionActor-gated ACP adapter",
            "response: {first}"
        );
        assert_eq!(
            first["state"], "succeeded",
            "workflow did not succeed: {first}"
        );
        assert_eq!(first["replayed"], false, "first run must not be a replay");
        assert_eq!(
            first["artifactsCommitted"], 1,
            "expected one first-time commit: {first}"
        );

        // The committed manifest must contain the bytes the cell actually
        // wrote. Recomputed here, not read back from the record claiming it.
        let expected_stdout = format!("{:x}", Sha256::digest(b"acp-computed\n"));
        let manifest = first["commits"][0]["outputManifest"]
            .as_object()
            .unwrap_or_else(|| panic!("no commit manifest: {first}"));
        assert!(
            manifest
                .values()
                .any(|d| d == &Value::String(expected_stdout.clone())),
            "stdout not committed with its true digest: {manifest:?}"
        );
        assert!(
            manifest.keys().any(|k| k.ends_with("result.json")),
            "the file the cell wrote is missing: {manifest:?}"
        );

        let run_id = first["runId"].as_str().expect("runId").to_owned();
        let attempt_id = first["attempts"][0]["attemptId"]
            .as_str()
            .expect("attemptId")
            .to_owned();
        let output_root = store_root.join("workflow-outputs");
        assert_eq!(
            attempt_dirs(&output_root, &run_id, "compute"),
            vec![attempt_id.clone()],
            "expected exactly one kernel attempt on disk"
        );

        // 2. Replay: same operation id, recorded outcome, no second execution.
        let replay: Value = serde_json::from_str(
            tokio::time::timeout(
                Duration::from_secs(120),
                client.ext_method("x.ai/science/workflow_execute", params("op-wf-exec-1")),
            )
            .await
            .expect("replay timed out")
            .expect("replay failed")
            .0
            .get(),
        )
        .expect("replay returned JSON");

        assert_eq!(replay["replayed"], true, "replay: {replay}");
        assert_eq!(replay["runId"], run_id, "replay reported a different run");
        assert_eq!(replay["state"], "succeeded", "replay: {replay}");
        assert_eq!(
            replay["attempts"].as_array().map(Vec::len),
            Some(1),
            "replay recorded a second attempt: {replay}"
        );
        assert_eq!(
            replay["attempts"][0]["attemptId"], attempt_id,
            "replay produced a new attempt id: {replay}"
        );
        // The decisive one: the kernel writes a fresh directory per attempt, so
        // a second execution could not be hidden.
        assert_eq!(
            attempt_dirs(&output_root, &run_id, "compute"),
            vec![attempt_id],
            "replay executed the kernel a second time"
        );

        // A different operation id IS a second execution — without this the
        // replay assertions above could pass on an endpoint that never runs
        // anything at all.
        let second: Value = serde_json::from_str(
            tokio::time::timeout(
                Duration::from_secs(120),
                client.ext_method("x.ai/science/workflow_execute", params("op-wf-exec-2")),
            )
            .await
            .expect("second execution timed out")
            .expect("second execution failed")
            .0
            .get(),
        )
        .expect("second returned JSON");
        assert_eq!(second["replayed"], false, "second: {second}");
        assert_eq!(second["state"], "succeeded", "second: {second}");
        assert_ne!(
            second["runId"],
            Value::String(run_id),
            "second run reused the first run id"
        );
        // Content-addressed commits: the same cell with the same inputs commits
        // once, so the second run REUSES rather than duplicating the artifact.
        assert_eq!(
            second["artifactsCommitted"], 0,
            "an identical step committed a second artifact: {second}"
        );
    })
    .await;
}

/// `operationId` is required, and a denied permission executes nothing.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_workflow_execute_fails_closed() {
    let Some(python) = workflow_python3() else {
        panic!(
            "no python3 on PATH: this test proves a real interpreter runs, so it cannot be skipped into a pass"
        );
    };
    with_local_set(|| async move {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-store");

        // 3. Missing operationId. Without an idempotency key a retry cannot be
        //    told apart from a second intentional execution, so the field is
        //    mandatory rather than defaulted.
        let client = GrokStdioClient::spawn(&server, workdir.path()).await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let missing_op = client
            .ext_method(
                "x.ai/science/workflow_execute",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "workflowSpec": workflow_spec("wf-no-op", WORKFLOW_CELL),
                    "interpreterPath": python,
                    "allowKernelSteps": true,
                }),
            )
            .await;
        let missing_op = missing_op.expect_err("workflow_execute without operationId was accepted");
        // Not merely "an error": a binary with no `workflow_execute` at all
        // would also error, and this test would then pass while proving
        // nothing. The refusal must come from parameter validation.
        assert_ne!(
            missing_op.code,
            acp::ErrorCode::MethodNotFound,
            "workflow_execute is not wired into this binary: {missing_op:?}"
        );

        // A forged session must not reach an executor either.
        let wrong_session = client
            .ext_method(
                "x.ai/science/workflow_execute",
                serde_json::json!({
                    "sessionId": "not-a-real-session",
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "operationId": "op-wf-forged",
                    "workflowSpec": workflow_spec("wf-forged", WORKFLOW_CELL),
                    "interpreterPath": python,
                    "allowKernelSteps": true,
                }),
            )
            .await;
        let wrong_session =
            wrong_session.expect_err("workflow_execute with an unknown session was accepted");
        assert_ne!(
            wrong_session.code,
            acp::ErrorCode::MethodNotFound,
            "workflow_execute is not wired into this binary: {wrong_session:?}"
        );

        assert!(
            !store_root.join("workflow-runs").exists(),
            "a rejected request created a durable workflow run"
        );
        assert!(
            !store_root.join("workflow-commits").exists(),
            "a rejected request committed an artifact"
        );
    })
    .await;
}

/// A denied permission must abort before anything runs: no kernel process, no
/// attempt, no commit, and no operation id burned.
#[tokio::test]
#[ignore] // requires pre-built binary
async fn test_stdio_science_workflow_execute_denied_runs_nothing() {
    let Some(python) = workflow_python3() else {
        panic!(
            "no python3 on PATH: this test proves a real interpreter runs, so it cannot be skipped into a pass"
        );
    };
    with_local_set(|| async move {
        let server = MockInferenceServer::start()
            .await
            .expect("start mock server");
        let workdir = git_workdir();
        let store_root = workdir.path().join("science-store");

        let client = GrokStdioClient::spawn_with_permission_response(
            &server,
            workdir.path(),
            PermissionResponse::Reject,
        )
        .await;
        client.initialize_with_timeout().await;
        let session_id = client.create_session_with_timeout(workdir.path()).await;

        let denied = tokio::time::timeout(
            Duration::from_secs(120),
            client.ext_method(
                "x.ai/science/workflow_execute",
                serde_json::json!({
                    "sessionId": session_id.0.as_ref(),
                    "ownerId": "science-owner",
                    "storeRoot": store_root,
                    "operationId": "op-wf-denied",
                    "workflowSpec": workflow_spec("wf-denied", WORKFLOW_CELL),
                    "interpreterPath": python,
                    "allowKernelSteps": true,
                    "probeTimeoutMs": 60_000,
                    "approvalTimeoutMs": 30_000,
                }),
            ),
        )
        .await
        .expect("denied workflow_execute timed out");

        assert!(
            denied.is_err(),
            "a denied permission still returned success: {denied:?}"
        );

        // 4. The decision gates the EXECUTION, not just the response.
        assert!(
            !store_root.join("workflow-commits").exists(),
            "denied execution committed an artifact"
        );
        assert!(
            !store_root.join("workflow-runs").exists(),
            "denied execution wrote a workflow run record"
        );
        assert!(
            !store_root.join("workflow-operations").exists(),
            "denied execution burned the operation id"
        );
        // Cell staging, driver materialisation and per-attempt output all live
        // behind the gate, so their directories exist (the adapter created
        // them) and are empty.
        for name in ["workflow-cells", "workflow-runtime", "workflow-outputs"] {
            let dir = store_root.join(name);
            let count = std::fs::read_dir(&dir).map(Iterator::count).unwrap_or(0);
            assert_eq!(
                count, 0,
                "denied execution left {count} entry/entries in {name}"
            );
        }

        // The durable Science run exists and records the refusal — a request
        // that was not granted is evidence, not silence.
        //
        // NOTE on which refusal: this harness answers a permission request with
        // `RequestPermissionOutcome::Cancelled`, which the bridge maps to
        // `ApprovalDecision::Cancel` and `RunState::Cancelled`. A user-pressed
        // "reject" would land on `Denied` instead. Both take the same
        // not-Allow branch in `finish_science_workflow_execution`, so what is
        // proven here is that permission-not-granted executes nothing; the
        // `Denied` spelling specifically is not what this harness produces.
        let run_id = std::fs::read_dir(store_root.join("runs"))
            .expect("durable refused run directory")
            .next()
            .expect("one refused run")
            .expect("run directory entry")
            .file_name()
            .to_string_lossy()
            .to_string();
        let store = xai_grok_science::ScienceStore::new(&store_root);
        let run = store
            .load_run(&xai_grok_science::RunId::new(run_id))
            .expect("load refused run");
        assert_eq!(run.state, xai_grok_science::RunState::Cancelled);
        assert_eq!(
            store.approvals(&run.context.run_id).unwrap()[0].decision,
            xai_grok_science::ApprovalDecision::Cancel
        );
    })
    .await;
}
