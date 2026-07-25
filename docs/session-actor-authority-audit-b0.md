# SessionActor Authority Audit — B0

**Phase**: B0 (Track B)
**Date**: 2026-07-25
**Commit**: `fe1a637d` (main)
**Auditor**: DeepSeek

---

## 1. Authority Chain (Confirmed)

```
UI / ACP / command
        ↓
SessionHandle  (handle.rs)          — sole Clone+Send proxy
        ↓
SessionCommand (commands.rs)        — message protocol enum
        ↓
owning SessionActor                 — single-writer state machine
        ↓
flush_to_disk  (acp_session_impl/updates.rs:355)  — persistence
        ↓
PermissionHandle                    — gated external actions
        ↓
WorkspaceOps / approved adapter     — tool execution
        ↓
artifact + evidence + provenance
        ↓
durable replay
        ↓
HostVerification
```

## 2. Single-Writer Verification

### 2.1 SessionHandle is the Sole Entry Point

**File**: `agent/crates/codegen/xai-grok-shell/src/session/handle.rs`

`SessionHandle` is `Clone + Send`. It holds:
- `cmd_tx: UnboundedSender<SessionCommand>` — единственный канал команд
- `persistence_tx: UnboundedSender<PersistenceMsg>` — канал для persistence-операций
- Shared state (prompt_id, interactions, signals, chat_state_handle) — read-only для callers

No other type in the codebase can send `SessionCommand` without going through `SessionHandle`.

### 2.2 Persistence is Actor-Internal

**File**: `agent/crates/codegen/xai-grok-shell/src/session/acp_session_impl/updates.rs:355`

```rust
pub(super) async fn flush_to_disk(&self) { ... }
```

`pub(super)` visibility — only callable within `acp_session_impl` module. All durable writes flow through this path or its sibling methods (`persist_xai_update_only`, `persist_announcement_state`, etc.).

### 2.3 No Second Writer Found

Full-module grep for `pub.*fn.*(write|persist|save|store|commit|flush)` across all session files confirmed:
- All persistence functions are `pub(super)`, `pub(crate)`, or internally-scoped
- No external crate can directly write session state
- `file_system.rs:230` (`write_file`) is a tool-accessible path, not a session state path

### 2.4 Spawn Path

**File**: `agent/crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs:91`

```rust
pub(crate) async fn spawn_session_actor(...) -> (SessionHandle, ...)
```

Single creation point. SessionHandle is the only return value that exposes commands to the outside world.

## 3. State Inventory Audit

| State | Owner | Access Pattern | Verified |
|---|---|---|---|
| Goal state | SessionActor | Commands only | ✅ |
| Expert state | SessionActor | Commands only | ✅ |
| active model | SessionHandle (via config) | Set at spawn, changed via command | ✅ |
| reasoning effort | SessionHandle (shared Arc) | Read-only for observers | ✅ |
| cache epoch | `cache_epoch` module | Only via `DurableCacheEvidenceObserver` | ✅ |
| permission requests | `PendingInteractions` | Inserted by actor, read by roster | ✅ |
| Science runs | `science_connector.rs` + `science_goal.rs` | Routed through SessionHandle commands | ✅ |
| ResearchResult | `science_goal.rs` | Actor-internal state | ✅ |
| artifact registrations | `replay_events.rs` + `chat_persistence.rs` | Actor-internal | ✅ |
| cancellation | `current_prompt_id` (Arc<Mutex>) | Shared read, actor-write | ✅ |
| copy/fork | `fork.rs` | Returns new SessionHandle | ✅ |
| crash recovery | `restore_stub.rs` | Restores SessionHandle from disk | ✅ |
| storm/repeat guards | `expert.rs`, `goal_strategist.rs` | Actor-internal | ✅ |

## 4. Invariant Tests

### 4.1 Durable-before-side-effect

**Status**: ⚠️ Needs verification

The handover document requires: `reserve/intent durable → ack → provider/tool/device side effect`. Current code routes through `SessionCommand` which the actor processes sequentially, providing a natural ordering guarantee. However, explicit `reserve → ack → execute` tests are not yet written.

### 4.2 Terminal exactly-once

**Status**: ⚠️ Needs verification

Session lifecycle transitions (Active → Completed/Failed/Cancelled) should be terminal. The existing `SessionLiveState` enum supports this but the exhaustive terminal-transition test matrix from the handover doc is not verified.

### 4.3 No Direct Filesystem Side-State

**Status**: ✅ Verified

All session state is stored via the actor's persistence pipeline. No `fs::write` directly writes session metadata.

## 5. Gaps Found

| Gap | Severity | Action |
|---|---|---|
| No `reserve → ack → execute` unit tests | Medium | Write in B1 |
| No terminal-exactly-once test matrix | Medium | Write in B1 |
| `install_truth_snapshot()` no runtime caller | High | Fix in B2 |
| Expert cannot write tools — confirmed? | ✅ Verified — `expert_consultant_tools.rs` enforces read-only |

## 6. Verdict

**SessionActor authority chain is intact.** No second runtime, session store, permission manager, provider stack, tool executor, artifact registry, replay log, or goal completion authority exists outside the SessionActor → SessionHandle path.

**Ready for B1** (persistence/restart/cancel invariant tests).
