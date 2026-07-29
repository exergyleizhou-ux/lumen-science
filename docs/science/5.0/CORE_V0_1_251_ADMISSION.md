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

**Single Rust base is not complete.** Science still identifies the embedded
Core line as `0.1.222`. The 135-file drift against audited Lumen main head
`dc563b1e0db9eaca7e970d56d7816e1522511723` is expected and machine-visible; it
must not grow or change identity silently, and it must not be erased by
rewriting VERSION to `0.1.251`. The `v0.1.251` tag commit
`88a204cfe468a325b53272dddf70afc8036d24ad` is the selective-admission target,
not the checkout used for this drift count.

Verify version honesty, offline drift fixtures (including an equal-count
substitution counterexample), the audited-head drift manifest, the admission
lock, and the optional exact binary without network access:

```bash
python3 scripts/release_version.py --root . check
python3 scripts/check-core-drift.py --self-test
python3 scripts/check-core-drift.py \
  --science-root . \
  --upstream-root /path/to/lumen@dc563b1e \
  --lock docs/science/5.0/core-v0.1.251-admission.lock.json
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

The Science authority integration commit is:

- `75fa66b70a8795c582b18b9e38b8d0486f6488d4`

The sealed `seq_analyze` follow-up commits are:

- `d38f948d491069ae6002b7c63adc515a145e45a9`
- `c54bb88ada7fbbc55a5a3a0e9c47ca8f29b07f20`
- `63f27da0d74ed050a7691c349805c69f26f2623d`

These commits are not Lumen version admissions. They close durable Science
SessionActor paths and intentionally make
`xai-grok-shell/src/session/science_goal.rs` one newly diverged shared path,
then change bytes in seven paths already classified as shared-diverged for the
sealed sequence authority route, cross-process serialization, and product
counterexamples. The machine lock records the exact resulting path set and
byte manifest instead of treating a count or byte change as implicit or
harmless.

## Current evidence

At Science code head `63f27da`:

- `cargo check -p xai-grok-science --all-targets --locked`: exit 0.
- `cargo test -p xai-grok-science --locked`: 589 passed, 0 failed, 8 explicit
  live network probes ignored; doc-tests 0.
- `cargo clippy -p xai-grok-science --all-targets --locked -- -D warnings`:
  exit 0 with 0 warnings.
- `cargo check -p xai-grok-shell --lib --locked`: exit 0.
- Exact-head `lumen` binary SHA-256:
  `8d7bd563028b24c73ad8aff7a67b93b373ce410ba5505f17748ccad54d36b545`.
- Exact-head built-binary `seq_analyze` ACP product tests: 10 passed, 0 failed.
  Other product families were not rerun for this admission update.
- The core drift reporter finds all 8 tracked security markers present.

This is source, focused/offline test, and local built-binary evidence. It is not
CI, live-provider, release, or deployment evidence.

## What is not admitted

The current comparison against Lumen `dc563b1e` still reports:

| Kind | Count |
|---|---:|
| Diverged shared-core Rust files | 130 |
| Missing in Science | 5 |
| Total drift | 135 |

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
