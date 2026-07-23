# Lumen Science seam contract

Rust Lumen is the sole execution, approval, verification, and truth authority. Open Science Agent, Session, ACP, Permission, and Provider runtimes are rejected.

| ID | Capability | Rust entry | Forbidden replacement |
|---|---|---|---|
| S1 | UI / science workflow | authenticated run/event/artifact API over durable Lumen records | upstream agent runtime |
| S2 | file import / preview | Lumen tool + artifact/evidence/provenance | upstream orchestration |
| S3 | MCP / SSH / HPC | Lumen approval, policy, tool dispatch | upstream permission model |
| S4 | notebook / computation | `xai-grok-shell::SessionActor` command and tool dispatch | independent executor authority |
| S5 | reviewer / quality | Goal lifecycle + Expert advice + HostVerification | reviewer PASS as completion |

Local commits on `science/kernel` must name one or more IDs. No merge or push is authorized.

## Approval terminal semantics

Approval keys are `(project_id, run_id, call_id)`. `allow`, `deny`, `timeout`, and `cancel` are terminal and idempotent only for an identical repeated decision. A conflicting second decision is rejected. Restart never executes a pending call: recovery converts pending approvals and their runs to explicit `interrupted` terminal records.

## Phase B local service and dispatch choices

- The Phase B event stream is authenticated bounded JSON polling through
  `events?after=<seq>&limit=<bounded>`; it is the plan's permitted SSE-equivalent
  and preserves every typed event field without a second event authority.
- `serve_loopback` accepts only a caller-bound loopback listener and owns the
  token guard for the complete server future. Startup replaces only a stale
  regular token file; symlink token paths fail closed.
- Science CSV uses a two-phase `SessionActor` command. Phase one persists the
  run and pending approval; phase two records the production permission result.
  Only `allow` reaches `WorkspaceOps::call_tool`; unresolved `Ask` fails closed.
