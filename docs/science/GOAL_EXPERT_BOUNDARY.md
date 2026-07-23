# Lumen Science Goal and Expert boundary

Seam contract: **S5**. Status: implemented in the Rust `SessionActor` on
`science/kernel`. The completion entry is
`x.ai/science/goal_host_verify`; it is routed through `MvpAgent` and the
owning actor, never through a consultant callback or a second runtime.

## Authority and completion

Goal owns Science task lifecycle. Expert/Reviewer outputs are untrusted,
bounded-quality advice: they may identify missing evidence, propose checks, or
request a repair attempt, but cannot change a Science run state, approve a
connector, bypass a sandbox, or mark work completed.

Only the existing Lumen HostVerification path may promote a run to a completed
user-visible outcome. For a Science result, HostVerification must inspect the
durable run record, approval terminal state, required artifacts/evidence, and
provenance. Consultant `PASS`, parsed natural language, or a model callback is
never sufficient evidence.

## Callback fencing

Every Expert-originated asynchronous result must carry the active Goal/task
identity and expected phase/revision. The SessionActor must reject it when the
Goal is terminal, replaced, cancelled, restored from a different revision, or
in a different phase. Rejection is a safe no-op plus a redacted audit event;
it must not mutate a new run or re-open an old approval.

Restart behavior is fail-closed: no in-flight consultant request resumes into a
new Goal generation, and no stale `PASS` can satisfy HostVerification after a
restart. Goal state and Expert state may be persisted for recovery, but recovery
must require a fresh host-side verification before any completion claim.

## Required proof

The following proof obligations are covered by the focused Rust tests in
`xai-grok-shell::session::science_goal` and
`xai-grok-science::review`:

1. consultant `PASS` without HostVerification does not transition a Science
   run to complete;
2. a stale callback after cancel/restart/new Goal is rejected and cannot mutate
   a different run;
3. consultant attempts cannot invoke connector approval, transport, or
   `update_goal` directly;
4. a valid HostVerification with matching durable evidence can complete the
   intended Goal only; and
5. all audit/error surfaces redact provider payloads and credentials.
