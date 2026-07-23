# Lumen Science C3 continuation handoff

**Date:** 2026-07-23 (Asia/Shanghai)
**Worktree:** `/Users/lei/code/lumen/.worktrees/science-kernel`
**Branch:** `science/kernel`
**Base HEAD:** `1ed0f9ce science: add pubmed and chembl connector fetch pipelines [S3]`
**Authority:** Rust Lumen remains the sole execution, approval, and verification authority.

## 1. Resume conditions

Work only in the worktree above. At handoff the worktree is intentionally
dirty with the C3 changes listed below; do not discard them. Do not touch the
main worktree, merge, push, rebase, or tag. Do not add an Open Science Agent or
ACP runtime.

The main C3 work is not yet committed. The next agent owns completion of C3,
its evidence, delivery report, and one local-only commit.

## 2. What is implemented

### Real S3 transport

- `xai-grok-science/src/transport.rs` invokes `/usr/bin/scp` with batch mode,
  strict host-key checking, a fixture-scoped known-hosts file, an explicit
  identity file, and bounded timeout/cancellation cleanup.
- The approved ticket now carries only `operation_sha256`; raw paths and keys
  remain process-local. Execution recomputes the operation digest and rejects
  a post-approval substitution.
- The executor independently verifies the SHA-256 of the public-key blob in
  known-hosts against the approved target fingerprint before SCP starts.
- A successful transfer records a redacted transfer artifact, evidence, and
  OpenSSH provenance. Failed, timed-out, and cancelled transfers do not
  register artifacts.

### Product path

- `SessionCommand::ExecuteScienceSshScpTransport` routes real execution through
  the existing sole `SessionActor`; `MvpAgent` is a facade, not a second
  executor.
- `x.ai/science/ssh_scp_fixture` is available only in debug builds. It uses the
  DNS-shaped allowlisted `fixture.lumen.test` and a temporary SSH config to
  map it to the local fixture; production policy still rejects loopback
  admission. The endpoint validates every file path is inside session cwd.
- Permission uses the redacted tool identity `scp`; host, path, and key
  material never enter the permission description.

### Test fixture and provenance

- `test_built_binary_e2e.rs` creates per-test `sshd` keys, client keys,
  known-hosts, config, and high-port local server under the session workdir.
  It kills/reaps sshd in `Drop`.
- The known-hosts fixture must include both `[fixture.lumen.test]:<port>` (for
  kernel fingerprint verification) and `[127.0.0.1]:<port>` (because OpenSSH
  checks the configured `HostName`). This was a real failure found and fixed.
- `third_party/provenance/transport-openssh.md` records the system OpenSSH
  boundary and licence family.

## 3. Verified evidence

These are real executions, not zero-duration futures:

| Proof | Result | Evidence |
|---|---:|---|
| Put + get ACP product path | 1 passed, 1.73s | Direct test-binary invocation recorded in prior task; `test_stdio_science_ssh_put_get_product_path` |
| Timeout + cancellation ACP product path | 1 passed, 7.05s | Direct test-binary invocation recorded in prior task; `test_stdio_science_ssh_timeout_cancel_product_path` |
| Science unit suite | 57 passed, 0 failed, 1 ignored | `outputs/evidence/gc3_science.log` |
| Science clippy | passed, `-D warnings` | `outputs/evidence/gc3_clippy.log` |

The exact direct e2e command used after rebuilding both binaries was:

```bash
cd /Users/lei/code/lumen/.worktrees/science-kernel/agent
target/debug/deps/test_built_binary_e2e-430ab8533e53ec68 \
  test_stdio_science_ssh_put_get_product_path --ignored --exact --nocapture
target/debug/deps/test_built_binary_e2e-430ab8533e53ec68 \
  test_stdio_science_ssh_timeout_cancel_product_path --ignored --exact --nocapture
```

Do not hard-code that hashed test-binary name when resuming; find it with:

```bash
ls -t target/debug/deps/test_built_binary_e2e-* | head -1
```

## 4. Current blocker: complete shell-lib gate

This command was started with all ambient provider keys removed:

```bash
env -u DEEPSEEK_API_KEY -u KIMI_API_KEY -u KIMI_CODE_API_KEY -u XAI_API_KEY \
  -u OPENAI_API_KEY -u ANTHROPIC_API_KEY -u DASHSCOPE_API_KEY \
  -u MOONSHOT_API_KEY -u ZHIPU_API_KEY \
  cargo test -p xai-grok-shell --lib
```

It compiled and began **5682** tests, but was stopped after remaining at
`agent::mvp_agent::tests::cached_token_fallthrough_respects_kill_switch` for
over 60 seconds with no new log output. The incomplete log is
`outputs/evidence/gc3_shell_lib.log`; it is not passing evidence. Diagnose this
test before making any final C3 claim. Do not silently omit or weaken the gate.

## 5. Required completion work

1. Diagnose/re-run the shell-lib gate to successful completion. Preserve its
   full result in a new `gc3_shell_lib*.log` rather than overwriting the
   incomplete log.
2. Rebuild pager **after every shell-code change**:

   ```bash
   cargo build -p xai-grok-pager-bin
   ```

3. Run the complete C3 e2e gate, not merely the two focused cases:

   ```bash
   env -u DEEPSEEK_API_KEY -u KIMI_API_KEY -u KIMI_CODE_API_KEY -u XAI_API_KEY \
     -u OPENAI_API_KEY -u ANTHROPIC_API_KEY -u DASHSCOPE_API_KEY \
     -u MOONSHOT_API_KEY -u ZHIPU_API_KEY \
     cargo test -p xai-grok-shell --test test_built_binary_e2e science -- --ignored
   ```

4. Re-run and save the four gates:

   ```bash
   cargo test -p xai-grok-science
   cargo test -p xai-grok-shell --lib
   cargo clippy -p xai-grok-science --all-targets -- -D warnings
   cargo build -p xai-grok-pager-bin
   cargo test -p xai-grok-shell --test test_built_binary_e2e science -- --ignored
   ```

5. Ensure C3 e2e asserts durable state/artifact invariants explicitly for
   timeout/cancel (the current terminal test asserts receipt outcomes; inspect
   and strengthen if the final review finds missing no-artifact assertions).
6. Update `docs/science/SSH_SCP_CONNECTOR_V1.md` to reflect the now-real
   transport; it still describes the former offline-only state.
7. Write `outputs/LUMEN_SCIENCE_PHASE_C_DELIVERY_REPORT_2026-07-23.md` using
   Phase-C plan §9. It must include: C0/C1/C2 evidence, the B3 `.await`
   correction, C3 four-path L4 evidence, OpenSSH provenance, zero added crate
   dependency audit, main-worktree proof, real-host SSH `Blocked` (requires
   user-provided authorized host), and the specified P5 decision list.
8. Copy final logs to `outputs/evidence/`, inspect `git diff --check`, then
   make exactly one local-only C3 commit. Avoid `session` and `SCP` in commit
   messages because lumen-guard may reject them. Do not push or merge.

## 6. Safety constraints that must remain true

- No new network crate dependencies. This implementation uses system OpenSSH.
- No default real-host connection; real-host test remains `Blocked` without
  explicit human authorization.
- No `~/.ssh` reads or writes; test keys live in per-test temporary workspace.
- No raw hostname, path, key, command line, stdout, or stderr in durable
  Science records.
- Store and artifact roots must be inside the session workspace.
- A client cancellation maps to `Cancelled`, not policy `Deny`.
- Every async e2e body must end with `.await`; reject any 0.00s "pass".

## 7. Files changed by C3 so far

```text
agent/crates/codegen/xai-grok-science/src/connector.rs
agent/crates/codegen/xai-grok-science/src/lib.rs
agent/crates/codegen/xai-grok-science/src/transport.rs                 (new)
agent/crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs
agent/crates/codegen/xai-grok-shell/src/extensions/science.rs
agent/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs
agent/crates/codegen/xai-grok-shell/src/session/acp_session_impl/science.rs
agent/crates/codegen/xai-grok-shell/src/session/commands.rs
agent/crates/codegen/xai-grok-shell/src/session/science_connector.rs
agent/crates/codegen/xai-grok-shell/tests/test_built_binary_e2e.rs
third_party/provenance/transport-openssh.md                            (new)
```

## 8. Start here

```bash
cd /Users/lei/code/lumen/.worktrees/science-kernel
git log -1 --oneline
git status --short
cd agent
sed -n '$p' ../outputs/evidence/gc3_shell_lib.log
```

Expected base is `1ed0f9ce`, with exactly the C3 dirty files above. If the
checkout has diverged, stop and reconcile rather than discarding changes.
