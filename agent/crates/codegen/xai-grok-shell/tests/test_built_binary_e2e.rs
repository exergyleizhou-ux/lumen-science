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

/// DS-38: structural proof that the science connector registry contains
/// exactly 42 entries and that every expected connector ID is present.
/// This test does NOT require a running binary — it reads the compiled-in
/// descriptor registry directly from the xai-grok-science crate.
#[test]
fn test_connector_registry_has_all_42_ids() {
    let registry = xai_grok_science::connectors::registry();
    assert_eq!(
        registry.len(),
        42,
        "descriptor registry must contain exactly 42 connectors"
    );

    let expected: std::collections::BTreeSet<&str> = [
        "pubmed", "chembl", "crossref", "uniprot", "europepmc", "openalex",
        "semantic-scholar", "arxiv", "biorxiv",
        "rcsb-pdb", "pdbe", "alphafold", "interpro", "sifts",
        "pubchem", "bindingdb", "gtopdb", "surechembl", "chebi",
        "ensembl", "ncbi-gene", "dbsnp", "clinvar", "gnomad", "ucsc", "mygene", "myvariant",
        "reactome", "string-db", "intact", "wikipathways", "opentargets",
        "geo", "arrayexpress", "gtex", "hpa", "expression-atlas", "single-cell-atlas", "depmap",
        "eutils", "biogrid", "kegg",
    ]
    .into_iter()
    .collect();

    let actual: std::collections::BTreeSet<&str> =
        registry.iter().map(|d| d.id).collect();

    assert_eq!(
        expected, actual,
        "connector registry must contain exactly these 42 IDs"
    );

    // Prove no duplicates
    let mut ids: Vec<&str> = registry.iter().map(|d| d.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 42, "connector registry contains duplicate IDs");
}
