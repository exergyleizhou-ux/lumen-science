# Lumen core v0.1.251 admission

**Status:** selective safety admission, not version parity

**Source of truth:** `exergyleizhou-ux/lumen`

**Target:** `v0.1.251` (`88a204cfe468a325b53272dddf70afc8036d24ad`)

**Audited core head:** `dc563b1e0db9eaca7e970d56d7816e1522511723`

## Decision

Lumen Science will use the newer Lumen core as the source of truth, but it will
admit changes in reviewed security/correctness slices until a real shared-core
boundary exists. It will not merge or overlay the complete core tree.

This preserves three invariants:

1. Rust `SessionActor` remains the sole Science execution, approval, artifact,
   evidence, provenance, and replay authority.
2. Science-specific features and protected Lumen customizations are not erased
   by a bulk sync.
3. A version or release claim is made only after source, package, product, CI,
   and release evidence all refer to the same exact head.

The machine-readable authority for this admission is
`core-v0.1.251-admission.lock.json`.

Verify the lock, both local Git histories, and the optional exact binary without
network access:

```bash
python3 scripts/verify-core-admission.py \
  --lumen-repo /path/to/lumen \
  --binary agent/target/debug/lumen
```

## What is admitted

The earlier `926fa620` Science commit admitted eight tracked core security
fixes. The v0.1.251 sync then admitted:

- torn UTF-8 JSONL recovery without weakening strict rewrite paths;
- zero-test verification as inconclusive, never proof of success;
- isolated-stack cancellation fixtures;
- typed verify-after-edit outcomes;
- SessionActor delivery only after an explicit typed pass;
- dead ChatState recovery as an ACP error instead of a process panic;
- hermetic unit-test discovery that cannot silently import the developer's real
  global Claude permission settings.

The v0.1.251 Science commits are:

- `188a88a45b89866f44b9dc95aacc02504a0ef4a0`
- `58783fc25ec7297e50f40a3f8ed653e4f82d05f4`
- `75d7251b625ddff19745e9cde6910b36a7df5c89`

## Current evidence

At Science code head `58783fc`:

- `cargo test -p lumen-verify`: 29 passed, 0 failed.
- `cargo test -p xai-grok-science`: 378 passed, 0 failed, 8 explicit live
  network probes ignored.
- `xai-grok-workspace` test binary: 1,414 passed, 0 failed.
- Exact-head `lumen` binary SHA-256:
  `33dc60ac77fa13e18b908c1c0a7c78ae5c1eea840949aef9c5b9cee8526a1f64`.
- Built-binary OSF9: 17 steps and 5 assertions passed, 0 failures.
- The core drift reporter finds all 8 tracked security markers present.

This is source, focused/offline test, and local built-binary evidence. It is not
CI, live-provider, release, or deployment evidence.

## What is not admitted

The current comparison against Lumen `dc563b1e` still reports:

| Kind | Count |
|---|---:|
| Diverged shared-core Rust files | 129 |
| Missing in Science | 5 |
| Total drift | 134 |

Therefore the Science product still identifies its embedded Lumen line as
`0.1.222`. Changing package versions, the SBOM, or release ledgers to
`0.1.251` now would be a false equivalence claim.

Release metadata commits and generated current-state ledgers are also not
portable evidence: they describe the core repository and its exact release
head, not this Science repository.

## Migration route

The safe route to “use new Lumen” has two horizons:

1. **Now:** continually admit high-risk core fixes in small, independently
   tested slices. Prioritize permission/workspace boundaries, SessionActor
   recovery, verification truth, and provider/session truth.
2. **Platform migration:** define a stable core API for permission,
   execution, verification, and evidence signing; then make Science depend on
   one pinned core instead of copying it.

The exit criterion for the second horizon is not “the diff is smaller.” It is
that Science no longer owns a second mutable implementation of the authority
layer.
