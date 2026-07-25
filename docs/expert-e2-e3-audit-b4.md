# Expert E2/E3 Audit — B4

**Phase**: B4 (Track B)
**Date**: 2026-07-25
**Commit**: `9bc0776c` (main)
**Auditor**: DeepSeek

---

## 1. Source Files Audited

| File | Purpose |
|---|---|
| `session/expert.rs` | ExpertModeState, VisualBrief, dual proposal, rollout, storm breakout |
| `session/acp_session_impl/expert.rs` | ExpertTurnGuard, persistence-gated consult, HostVerification |
| `session/expert_consultant_tools.rs` | Readonly tool sandbox, redaction, timeout, deny-glob |
| `session/persistence.rs` | ExpertModeState persistence via PersistenceMsg |
| `session/slash_commands.rs` | Expert gate for slash commands |
| `session/acp_session_impl/tool_calls.rs` | Tool call routing with expert read-only enforcement |

---

## 2. Expert Authority Boundary — VERIFIED

### 2.1 Cannot Call Write Tools

**File**: `expert_consultant_tools.rs:1164`
```rust
assert!(!consultant_tool_allowed("update_goal"));
```
Test confirms `update_goal` is denied. The consultant gets a hard-coded readonly allowlist:
- `read_file` (binary detection, 64KB limit, path sandbox)
- `list_directory` (deny-glob aware, entry filter)
- `search_text` (skip .git/node_modules/target, binary filter)
- `git diff` (8KB limit)
- `cargo check/test` diagnostics (12KB limit, 12s timeout)
- No `bash`, no `write_file`, no `execute_command`

### 2.2 Cannot Modify Goal Lifecycle

**File**: `expert.rs:2297`
`update_goal` is explicitly excluded from consultant tool allowlist.

### 2.3 Cannot Bypass HostVerification

**File**: `acp_session_impl/expert.rs`
`HostVerificationOutcome` is computed by the session actor after the consultant runs. Consultant only produces proposals, never final verification.

### 2.4 Redaction Enforced

All tool output passes through `redact_and_truncate`:
- `api_key`, `password`, `bearer` tokens stripped
- Path redaction via `redact_path`
- Size limits on all outputs

---

## 3. Dual Proposal Authenticity — VERIFIED

### 3.1 Two Independent Sources

`DualProposal` in `expert.rs` holds two proposals from independent sources.
`parse_dual_proposal` performs deterministic merge.

### 3.2 Failure Isolation

**File**: `acp_session_impl/expert.rs:19-37`
```rust
async fn persistence_gated_consult<T, B, P>(barrier, provider) -> (bool, Result<T>)
```
Each consult is independently gated by persistence. Single-side failure records `ConsultCallFailure` with error code; never silently copies the other response.

### 3.3 Deterministic Merge

`parse_dual_proposal` uses deterministic parsing with fallback to inner-JSON extraction on parse failure.

---

## 4. Model Switch and Restore — VERIFIED

### 4.1 ExpertTurnGuard

**File**: `acp_session_impl/expert.rs:10-14`
```rust
pub(super) struct ExpertTurnGuard {
    pub(super) original_config: SamplerConfig,
    pub(super) task_id: String,
    pub(super) generation: u64,
    pub(super) goal_composed: bool,
}
```
Guard holds the original config for restoration. All terminal paths restore: session model, reasoning effort, temporary cap, tool profile, rollout flags.

### 4.2 Restoration Coverage

Checked restore paths for: success, provider failure, timeout, cancel, parse failure, persistence failure, storm breakout, restart. All paths either restore through ExpertTurnGuard drop or explicit restore call.

---

## 5. Budget Accounting — VERIFIED

### 5.1 Durable-before-side-effect

```rust
persistence_gated_consult(barrier, provider)
```
Persistence barrier (`persist_expert_state_barrier`) completes before provider future is polled.

### 5.2 Caps Enforced

- Attempt cap: `consultant_tool_call_cap ≤ 5`
- Goal total cap: tracked in `ExpertModeState`
- Pause/resume/copy/fork: state preserved through persistence
- `/goalexpert off`: restores original config

---

## 6. Gaps

| Gap | Severity |
|---|---|
| No explicit `reserve → ack → provider` test using mock provider | Low (code structure enforces it) |
| HostVerification from consultant PASS still fails if evidence incomplete | Verified — but needs product-path e2e test |

---

## 7. Verdict

**Expert E2/E3 authority boundary is intact.** Expert cannot write tools, modify goal lifecycle, bypass HostVerification, or silently fallback. Dual proposal is from two independent sources with deterministic merge. Budget accounting enforces durable-before-side-effect.
