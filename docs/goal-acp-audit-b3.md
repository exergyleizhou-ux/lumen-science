# Goal / Pager / ACP Product Loop Audit — B3

**Phase**: B3 (Track B)
**Date**: 2026-07-25
**Commit**: `9bc0776c` (main)
**Auditor**: DeepSeek

---

## 1. Source Files Audited

| File | Purpose |
|---|---|
| `session/goal_tracker.rs` | GoalTracker, GoalStatus, all lifecycle states |
| `session/acp_session_impl/goal.rs` | Goal processing, UpdateGoalAck, resume/pause/clear |
| `session/acp_session_impl/session_actor_invariants.rs` | Invariant tests for goal + expert boundaries |
| `session/acp_session_impl/spawn.rs` | GoalUpdateHandle registration |
| `session/slash_commands.rs` | Goal slash command routing |
| `session/acp_session_impl/types.rs` | GoalCompletionGate, triple-guard |

---

## 2. Goal Lifecycle States — VERIFIED

### 2.1 All States Present

**File**: `goal_tracker.rs:64`

| State | Description |
|---|---|
| `Active` | Goal in progress |
| `Complete` | Terminal — all verifications passed |
| `BudgetLimited` | Terminal — budget exhausted |
| `UserPaused` | Paused by user (`/goal pause`) |
| `BackOffPaused` | Auto-paused on consecutive failures |
| `NoProgressPaused` | Auto-paused on stall detection |
| `InfraPaused` | Auto-paused on infrastructure error |
| `Blocked` | Blocked via `update_goal(blocked_reason)` |

All states are serializable (snake_case), deserializable, and backward-compatible with legacy wire format.

### 2.2 Terminal Exactly-Once

`is_terminal()` returns true only for `Complete` and `BudgetLimited`. Once terminal, transitions to other states are rejected.

---

## 3. Goal Slash Command Path — VERIFIED

### 3.1 Pager → ACP → Shell → GoalTracker

```
pager input (/goal, /goal status, /goal pause, etc.)
  → ACP message
  → slash_commands.rs resolution
  → SessionCommand (GoalSet, GoalStatus, GoalPause, GoalResume, GoalClear)
  → acp_session_impl/goal.rs processing
  → GoalTracker state mutation
  → pager update via session/prompt response
```

### 3.2 GoalCompletionGate (Triple Guard)

**File**: `acp_session_impl/types.rs:182-212`

Three guards prevent premature completion:
1. `Pending` — `update_goal` arrived during inference; deferred
2. `Channel` — accumulated via tool channel; processed in order
3. Second `update_goal(completed: true)` while first is pending → rejected

### 3.3 Pager Does Not Hold Second Goal State

Goal state is owned by `GoalTracker` inside SessionActor. Pager reads via `SessionHandle` (display only). No independent goal state in pager.

### 3.4 Expert Cannot Complete Goal

**File**: `session_actor_invariants.rs:79`
```rust
assert!(!consultant_tool_allowed("update_goal"));
```
Expert tool allowlist explicitly rejects `update_goal`.

---

## 4. Invariant Tests — VERIFIED

**File**: `session_actor_invariants.rs`

Active invariants tested:
- Consultant cannot call `update_goal`
- GoalStatus wire format round-trips all variants
- `is_paused()` correct for all states
- Session actor single-writer property

---

## 5. Missing from Product Path

| Gap | Status |
|---|---|
| built-binary PTY/ACP e2e for `/goal` | ❌ Not verified (requires compiled binary) |
| Restart preserves Goal state | ✅ Code path exists (`restore_stub.rs`); test exists in `goal_tracker.rs` |
| Copy/fork isolates Goal state | ✅ `fork.rs` creates new session identity |
| Concurrent prompt during goal processing | ✅ Queue-based; goal commands are sequential |
| Screenshot/terminal evidence | ❌ Not captured (requires running binary) |

---

## 6. Verdict

**Goal/ACP product loop is structurally verified.** All lifecycle states exist, terminal transitions are correctly gated, Expert is barred from completing goals, and invariants are tested. Built-binary e2e evidence pending compilation.
