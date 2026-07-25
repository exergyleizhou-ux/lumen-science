# Lumen Science Architecture Decision Records

## ADR-0501: ResearchProject Aggregate

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V1 operates on individual `Run` instances. V2 needs a higher-level aggregate
that groups runs, evidence, and claims under a single research project.

### Decision
`ResearchProject` is the aggregate root. It owns:
- `project_id`, `owner_id`, `title`, `research_question`, `hypotheses[]`
- `sessions[]`, `datasets[]`, `workflows[]`
- `evidence_graph_id`, `review_policy`, `retention_policy`
- Status: Draft → Planned → Active → ReviewPending → Accepted|Rejected|Inconclusive → Archived

All mutations go through `SessionActor` as the sole authority. No external
agent, MCP server, or Python kernel may create or modify a ResearchProject.

### Consequences
- Every V1 Run must be migratable to a minimal ResearchProject
- Replay must reproduce the exact project state from events
- Unknown fields must be preserved during migration


## ADR-0502: Workflow Execution Semantics

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V1 runs are linear sequences of exchanges. V3 needs DAG-based workflows
with branching, retry, and approval steps.

### Decision
WorkflowSpec is a declarative DAG of Steps:
- Step types: ConnectorFetch, ArtifactTransform, NotebookCell, Renderer,
  Reviewer, HumanApproval, Export
- No arbitrary shell step (security invariant)
- Exactly-once artifact commit, at-least-once execution with dedup
- Deterministic replay uses stored fixtures, never re-fetches live

### Consequences
- Workflow must validate DAG acyclicity before execution
- Each step must produce a verifiable artifact or evidence
- Cancelled workflows must leave no partial state


## ADR-0503: EvidenceGraph Authority

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V1 has flat evidence lists. V2 needs a graph structure connecting claims,
artifacts, observations, and reviewer verdicts.

### Decision
EvidenceGraph is a directed graph with typed nodes and edges:
- Nodes: SourceArtifact, DerivedArtifact, Observation, Claim, Method,
  ParameterSet, ComputeEnvironment, ReviewerVerdict, DeviceReading,
  DeviceCommand, ExternalCitation
- Edges: derived_from, supports, contradicts, measured_by, executed_with,
  reviewed_by, supersedes, reproduces, fails_to_reproduce
- Every edge binds: source, target, relation, actor, timestamp, run_id,
  supporting_artifact_sha256, confidence_kind

The graph must reject: dangling nodes, self-referencing claims, cycles,
cross-project edges, and edges without artifact citations.

### Consequences
- EvidenceGraph is stored as durable events in SessionActor's event log
- Replay must reconstruct the exact graph state
- Graph queries must verify owner/project/session authorization


## ADR-0504: ComputeEnvironment Identity

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
Reproducible science requires exact environment identity.

### Decision
Every ComputeEnvironment manifest records:
- OS/architecture, Lumen binary hash, Rust crate lock
- Python/R/Julia executable hash, dependency lock hash
- Locale/timezone, environment allowlist, CPU/GPU identity
- Deterministic flags, network policy, container/VM identity

Compute environments are immutable. Any change produces a new environment ID.

### Consequences
- Workflow replay must verify environment hash matches
- Unknown interpreter versions must fail closed
- Container identity must include digest, not just tag


## ADR-0505: Collaboration and Ownership

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V1 is single-user. V3 needs multi-user collaboration with clear ownership.

### Decision
- Every ResearchProject has exactly one owner
- Collaborators are explicitly invited with bounded permissions
- Permission model: Read, Comment, Propose, Approve, Admin
- Cross-project references require explicit allowlisting
- Evidence graph edges cannot cross project boundaries

### Consequences
- Ownership changes require explicit transfer with audit trail
- Removed collaborators lose access to all project artifacts
- Collaboration packages are self-contained with manifest


## ADR-0506: Remote Compute Boundary

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V1 has basic SSH/SCP. V3 needs governed remote compute.

### Decision
Remote compute goes through SessionActor's permission system:
- Host must be DNS-shaped, not raw IP
- SHA-256 host key fingerprint must match
- Explicit egress permission per operation
- Operation digest binds direction, paths, timeout
- Credentials never enter logs, artifacts, or evidence

### Consequences
- SSH config override only allowed in debug fixtures
- Real host validation blocked until user authorizes
- Timeout and cancellation kill and reap child processes


## ADR-0507: Digital Twin Boundary

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V4 introduces digital twins and simulation before real device control.

### Decision
Digital twin output is derived evidence, never device observation:
- Must include: model identity, version/hash, initial state, parameters,
  assumptions, simulation clock, random seed, prediction interval, limitations
- Cannot be used as proof of real experiment success
- UI must clearly label simulation vs real results

### Consequences
- target_mode field is mandatory on all experiments (Dummy/DigitalTwin/HardwareInLoop/Real)
- Digital twin predictions that match real results validate the model,
  not the experiment


## ADR-0508: Device Command Safety

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V4/V5 introduce device control. Safety must be first principle.

### Decision
Device commands follow a strict safety ladder:
1. observe-only → recommendation-only → approved single action →
   approved bounded sequence → supervised closed loop
2. Every command has a CommandPlan with sha256 binding
3. Emergency stop is a deterministic, LLM-independent path
4. No auto-recovery from emergency stop
5. target_mode must be explicitly verified before any device action

### Consequences
- Preflight checks are mandatory before every device session
- Device identity, calibration, interlock all must verify
- Any unknown state fails closed


## ADR-0509: Reviewer Independence

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
V1 has basic reviewer. V2 needs multi-role independent review.

### Decision
- Reviewer outputs are evidence, not truth
- Multiple reviewers must operate independently
- Reviewer cannot approve their own results
- All reviewer verdicts must cite artifact SHA-256
- Review collusion must be detectable

### Consequences
- Reviewer identity is tracked per verdict
- Automated review must be labeled as such
- Human review carries different evidentiary weight


## ADR-0510: Replay and Non-determinism

**Status**: ACCEPTED
**Date**: 2026-07-25

### Context
Scientific reproducibility requires exact replay. V1 does event replay.
V2-V5 need increasingly sophisticated reproduction.

### Decision
Three reproduction levels:
- R1 replay-only: replay events and existing artifacts
- R2 deterministic rerun: same environment, fixed fixtures, recompute
- R3 independent reproduction: new session/environment, from approved inputs

Live provider calls during replay are forbidden. R2/R3 must report which
level was achieved and any deviations.

### Consequences
- Replay infrastructure must handle schema migrations
- Old artifacts must retain original hashes
- Notebook replay must not auto-execute
- Provider cache truth must be verified before citing
