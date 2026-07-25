# Cache Control Plane — Interface Freeze

**Phase**: A2 (Track A)
**Date**: 2026-07-25
**Purpose**: Freeze and document the cache truth control plane so Science/Expert can only consume truth, never manufacture it.

---

## 1. Frozen Types

### 1.1 CacheDomain — Provider Cache Identity

**Location**: `agent/crates/codegen/xai-grok-shell/src/session/cache_epoch.rs:150-160`

```rust
pub struct CacheDomain {
    pub provider: String,              // Provider slug (e.g., "grok", "deepseek")
    pub base_url: String,              // API base URL
    pub backend: String,               // Backend name
    pub model: String,                 // Model identifier
    pub effective_effort: Option<String>, // Reasoning effort if set
    pub credential_scope: Option<String>, // Account/credential slot (NOT raw key)
    pub permission_domain: String,     // Permission boundary identifier
    pub tool_manifest_fingerprint: String, // SHA-256 of ordered tool manifest
}
```

**Invariant**: All fields are non-secret identities. Credential scope is a slot/account ID, never a derived API key or bearer token.

**Fingerprint**: `CacheDomain::fingerprint()` produces a deterministic SHA-256 hex digest of all fields. This is the cache identity sent to the provider.

### 1.2 CacheEpochRecord — Durable Epoch Metadata

**Location**: `agent/crates/codegen/xai-grok-shell/src/session/cache_epoch.rs:182-191`

```rust
pub struct CacheEpochRecord {
    pub schema_version: u32,           // Current: 1
    pub epoch_id: Uuid,                // Unique epoch identifier
    pub generation: u64,               // Monotonic generation counter
    pub domain_fingerprint: String,    // SHA-256 of CacheDomain
    pub pending_mutation_reasons: Vec<WireMutationReason>, // Bounded, enum-only
}
```

**Storage**: `cache_epoch.json` in session directory. Lives beside (not inside) `chat_history.jsonl`.

**Invariant**: Contains neither prompt material nor credentials. Mutation reasons are bounded enum values, never rewritten history or request text.

### 1.3 CacheEpochDisposition — Why Epoch Changed

**Location**: `agent/crates/codegen/xai-grok-shell/src/session/cache_epoch.rs:194-200`

```rust
pub enum CacheEpochDisposition {
    Retained,            // Epoch unchanged, valid
    CreatedMissing,      // No epoch file existed — first use
    RotatedDomainChanged,// Domain fingerprint changed (model/URL/etc)
    RotatedInvalidRecord,// Epoch file corrupted or unparseable
    RotatedFork,         // Session was forked — new identity required
}
```

### 1.4 HistoryMutationAck — Committed History Boundary

**Location**: `agent/crates/codegen/xai-chat-state/src/events.rs:19-23`

```rust
pub struct HistoryMutationAck {
    pub revision: u64,                     // Monotonic revision
    pub mutation: CommittedHistoryMutation, // What changed
    pub new_len: usize,                    // Conversation length after mutation
}
```

**Invariant**: The `revision` is the only valid shell-side cache-epoch boundary. After any `HistoryMutationAck`, cache truth must be re-evaluated.

### 1.5 WireObservationContext — Per-Request Cache Context

**Location**: `agent/crates/codegen/lumen-discipline/src/wire.rs:48-53`

```rust
pub struct WireObservationContext {
    pub cache_domain_hash: String,         // Current CacheDomain fingerprint
    pub cache_epoch_id: String,            // Current epoch UUID
    pub mutation_reasons: Vec<WireMutationReason>, // Why this request differs
}
```

### 1.6 WireMutationReason — Mutation Causes

**Location**: `agent/crates/codegen/lumen-discipline/src/wire.rs:18-28`

```rust
pub enum WireMutationReason {
    RetryImageStrip,
    ImageEvicted,
    ToolResultPruned,
    MemoryChanged,
    FullCompaction,
    ModelChanged,
    BaseUrlChanged,
    PermissionProfileChanged,
}
```

### 1.7 WireRequestSnapshot — Sanitized Request Evidence

**Location**: `agent/crates/codegen/lumen-discipline/src/wire.rs:30-42`

```rust
pub struct WireRequestSnapshot {
    pub cache_domain_hash: String,
    pub cache_epoch_id: String,
    pub transport_hash: String,           // SHA-256 of transport material
    pub provider_cache_material_hash: String, // SHA-256 of provider-specific bytes
    pub body_bytes: u64,                  // Total request body size
    pub wire_common_prefix_bytes: Option<u64>, // Available only while predecessor in memory
    pub serialization_kind: WireSerializationKind,
    pub mutation_reasons: Vec<WireMutationReason>,
    pub attempt_index: u32,              // Within retry loop
}
```

**Invariant**: Contains no prompt bytes, credentials, or HTTP metadata. Evidence is one-way — it can prove a request happened, never what it said.

### 1.8 DurableCacheEvidenceObserver — Best-Effort Evidence Sink

**Location**: `agent/crates/codegen/xai-grok-shell/src/session/cache_epoch.rs:60-139`

Implements `xai_grok_sampler::RequestObserver`. Uses a bounded sync channel (capacity 64) and background writer thread. Key invariants:
- Provider call is **never blocked** by evidence writing (`try_send` only)
- Full queue makes evidence unavailable, **never** represented as successful write
- Worker failure makes later evidence unavailable, **never** inferred as cache hit
- Availability is tracked via `AtomicU8` (Available / UnavailableQueueFull / UnavailableWriteFailed / UnavailableWriterClosed)

### 1.9 WireSerializationKind

**Location**: `agent/crates/codegen/lumen-discipline/src/wire.rs:11-16`

```rust
pub enum WireSerializationKind {
    ChatCompletions,
    Responses,
    Messages,
}
```

---

## 2. Cross-Crate Ownership

| Type | Defining Crate | Primary Consumer |
|---|---|---|
| `CacheDomain` | `xai-grok-shell` | `xai-grok-shell`, `xai-grok-sampler` |
| `CacheEpochRecord` | `xai-grok-shell` | `xai-grok-shell` |
| `HistoryMutationAck` | `xai-chat-state` | `xai-grok-shell` |
| `WireObservationContext` | `lumen-discipline` | `xai-grok-sampler`, `xai-grok-shell` |
| `WireRequestSnapshot` | `lumen-discipline` | `xai-grok-sampler` (producer), `xai-grok-shell` (consumer) |
| `WireMutationReason` | `lumen-discipline` | All |
| `RequestObserver` | `xai-grok-sampler` | `xai-grok-shell` (implementor) |

---

## 3. Invariants (Non-Negotiable)

1. **Science/Expert can only consume cache truth, never manufacture it.**
   - `CacheDomain::fingerprint()` is read-only hash computation
   - `CacheEpochRecord` is written only by `cache_epoch` module in shell
   - No Science or Expert crate may import `cache_epoch` internals

2. **Each provider has independent cache semantics.**
   - Different providers → different `CacheDomain.fingerprint()`
   - Provider fallback refusal → never silently reuse another provider's cache truth

3. **Evidence is fail-open for providers, fail-closed for cache claims.**
   - Evidence sink failure → provider call continues (fail-open)
   - No evidence → cache hit claim is denied (fail-closed for UI display)

4. **No evidence → `Unavailable`, never inferred as `Hit`.**
   - Empty evidence file means no evidence was recorded
   - Provider reporting 0 usage means `Unavailable`, not `Hit`

5. **Epoch rotates on any domain change, never on Goal/Expert lifecycle events.**
   - Goal multi-turn → epoch unchanged if domain same
   - Expert consultation → does not pollute user session cache truth
   - Model switch → domain fingerprint changes, epoch rotates
   - Session fork → new session identity, does not inherit previous truth

---

## 4. What This Freeze Means

- The above types and their field sets are **frozen** as of main `b9dc2e43`.
- Adding fields requires a schema version bump in `CacheEpochRecord.schema_version`.
- Renaming or removing fields is a **breaking change** requiring migration.
- New consumers must go through the public API, never reach into `cache_epoch` internals.

---

## 5. Verification Status

| Check | Status |
|---|---|
| `CacheDomain` contains no secrets | ✅ Audit confirmed — all fields are identity slots |
| `CacheEpochRecord` contains no prompt material | ✅ Audit confirmed — schema_version + epoch_id + generation + fingerprint + bounded reasons |
| `WireRequestSnapshot` contains no raw prompt bytes | ✅ Audit confirmed — hashes and byte counts only |
| `DurableCacheEvidenceObserver` non-blocking | ✅ `try_send` on bounded channel, background writer |
| Evidence availability never becomes cache truth | ✅ `EvidenceAvailability` is separate type, never fed into hit/miss logic |
| Shellcheck on cache-related scripts | ✅ 45 scripts, 0 issues |
| Clippy baseline | Pending (compilation in progress) |
| Package tests | Pending (compilation in progress) |
