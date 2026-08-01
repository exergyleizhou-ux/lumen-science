# NextGen Plan Supersession Map

**Status:** F0 baseline map; it records document roles, not feature completion.

| Document | Role after F0 | May it be deleted or treated as obsolete evidence? |
|---|---|---|
| `LUMEN_SCIENCE_NEXTGEN_FINAL_EXECUTION_BOOK_2026-08-01.md` | Canonical execution order, cross-repo gates, owners, negative tests, and completion definition | No. Amend it only with an explicit evidence-backed revision. |
| `EXTREME_ADOPTION_SINGLE_BASE_EXECUTION_PLAN_2026-08-01.md` | Nine-source rights/asset intake, single-Rust-base migration detail, and capability waves | No. It remains the source-intake and migration reference. |
| `NEXT_GENERATION_AUTONOMY_CONTROL_PLANE_EXECUTION_PLAN_2026-08-01.md` | Authority, delegation, memory, Advisor, Kairos, and anti-pattern semantics | No. It remains the detailed safety reference. |
| `ECOSYSTEM_ABSORPTION_PLAN.md` | Historical ledger of already admitted/adapted source material and provenance | No. It is historical evidence, not an admission of every upstream asset. |
| `ecosystem-admission.lock.json` and `verify-ecosystem-admission.py` | Existing four-source authority/admission guard | No. v2 extends it; v1 remains a regression oracle until v2 is independently accepted. |
| `upstream-lock.v2.json` and `verify-upstream-lock-v2.py` | Planned nine-source immutable intake ledger | Not yet active. The draft must report `BLOCKED`, never `PASS`. |
| Lumen's local `LUMEN-NEXTGEN-EXECUTION-BOOK-2026-08-01.md` | Read-only design input from another working session | Never a Science dependency until it becomes an immutable Lumen commit with the required gates. |

## Non-negotiable ordering

1. `LUMEN_R0_SOURCE_GATE` makes a clean canonical Core source consumable.
2. C1 produces `PLATFORM_API_GATE`; R0 alone never implies a public Science extension port.
3. Science source intake may proceed in parallel, but a source catalog is not a runnable capability.
4. Existing actor-gated `seq_analyze`, Motif, Desktop and provenance work remain fixtures and parity oracles; they are not discarded.
5. No document, CI result, local binary, release tag, or provider claim substitutes for a different evidence layer.
