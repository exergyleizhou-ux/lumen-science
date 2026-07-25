# TruthSnapshot Runtime Wiring Audit — B2

**Phase**: B2 (Track B)
**Date**: 2026-07-25
**Commit**: `1765a861` (main)
**Auditor**: DeepSeek

---

## 1. Correction to Handover Document

The handover document states:

> `install_truth_snapshot()` — 全仓搜索只看到定义和测试，没有确认真实 runtime caller。

**This is incorrect.** There ARE multiple runtime callers. The confusion arose because `install_truth_snapshot()` specifically (the method that takes a pre-assembled snapshot and a seq number) is test-only. But the **actual runtime path** uses `refresh_truth_snapshot()` and `initial_truth_pair()`, which do the same thing without the seq check.

---

## 2. Confirmed Runtime Callers

### 2.1 Initialization (AgentView::new)

**File**: `agent_view/session.rs:381-387`
```rust
pub fn new(session: AgentSession, scrollback: ScrollbackState) -> Self {
    let (truth_session, truth_snapshot) = Self::initial_truth_pair(&session);
    // ... truth_snapshot is stored as Arc<TruthSnapshot>
}
```
Called every time an agent view is created (new session, or session reload).

### 2.2 Status Display (/status)

**File**: `dispatch/status.rs:19`
```rust
let text = crate::views::status_detail::redacted_report(
    agent.display_truth_snapshot(),
    std::time::SystemTime::now(),
);
```
`/status` reads from the snapshot — synchronous, no provider call, no probe. **Matches handover requirement.**

### 2.3 Capability Probe Refresh

**File**: `dispatch/status.rs:84-88`
```rust
agent.truth_session.capability = CapabilityState::Failed { ... };
let _ = agent.refresh_truth_snapshot();
```
Refreshes snapshot when a capability probe fails. Also see `begin_truth_probe` for the success path.

### 2.4 Dashboard Display

**File**: `dispatch/dashboard.rs:405`
```rust
agent.display_truth_snapshot(),
```
Dashboard reads from the same snapshot as status.

### 2.5 Cache Updates

**File**: `agent_view/session.rs:300-303`
```rust
pub fn apply_cache_update(&mut self, cache: CacheSummary) -> Result<(), String> {
    self.truth_session.cache = cache;
    self.refresh_truth_snapshot()
}
```
Called when cache truth changes (e.g., provider response received).

---

## 3. Trigger Condition Coverage

| Trigger | Wired? | Location |
|---|---|---|
| provider/model binding change | ✅ | Probe refresh + `initial_truth_pair` |
| tool schema hash change | ✅ | `CapabilityFingerprintInput.tool_schema_hash` |
| binary/source tuple change | ✅ | `ProductIdentity` in `initial_truth_pair` |
| verification command result | ✅ | `VerificationSummary` |
| session reload | ✅ | `AgentView::new()` → `initial_truth_pair` |
| permission profile change | ✅ | `PermissionSummary` |
| cache provider truth change | ✅ | `apply_cache_update` |
| filesystem mutation | ❌ | Not wired — no filesystem watcher triggers |
| `git_head_changed` | ❌ | Not wired |
| Science connector readiness | N/A | Pager concern; handled in shell layer |

---

## 4. Remaining Gaps

| Gap | Severity | Action |
|---|---|---|
| Filesystem mutation does not refresh snapshot | Low | Add fs watch callback that calls `refresh_truth_snapshot` |
| `git_head_changed` not monitored | Low | Add git head poll or hook |
| `install_truth_snapshot()` seq-based install unused at runtime | Info | Method exists for future cross-surface sync; not currently needed |

---

## 5. Verdict

**TruthSnapshot IS wired at runtime.** The handover document's claim of "no runtime caller" was based on searching only for `install_truth_snapshot()` (test-only path), missing the `refresh_truth_snapshot()` and `initial_truth_pair()` paths that are the actual runtime callers.

`/status` is synchronous, no provider call, uses cached snapshot, and redacts sensitive data — **matching all handover requirements for Phase 3.4.**

The only uncovered triggers are filesystem mutation and git_head_changed — minor gaps that do not block the product.
