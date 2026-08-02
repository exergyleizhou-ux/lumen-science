# NextGen Plan Supersession Map

**Status:** 2026-08-02 canonical map; it records document roles, not feature completion.

| Document | Role from 2026-08-02 | May it be deleted or treated as obsolete evidence? |
|---|---|---|
| `LUMEN_SCIENCE_NEXTGEN_CANONICAL_EXECUTION_BOOK_2026-08-02.md` | **Canonical Lumen Science execution order**, current observation, cross-repo gates, nine-source capability waves, owners, negative tests, product/release gates, and final completion definition | No. This is the only document allowed to change global ordering or status semantics. |
| `NEXTGEN_CANONICAL_EXECUTION_BOOK.lock.json` and `verify-nextgen-canonical-book.py` | Hash, required-contract, pointer and acyclic-DAG guard for the canonical book | No. It proves plan integrity only and must keep top-level pass count at zero until separate runtime gates pass. |
| `LUMEN_SCIENCE_NEXTGEN_FINAL_EXECUTION_BOOK_2026-08-01.md` | Previous canonical plan and detailed historical oracle | No. It is superseded for ordering and current status; retain its evidence and detail. |
| `EXTREME_ADOPTION_SINGLE_BASE_EXECUTION_PLAN_2026-08-01.md` | Nine-source rights/asset intake, single-Rust-base migration detail, and capability waves | No. It remains the source-intake and migration reference. |
| `NEXT_GENERATION_AUTONOMY_CONTROL_PLANE_EXECUTION_PLAN_2026-08-01.md` | Authority, delegation, memory, Advisor, Kairos, and anti-pattern semantics | No. It remains the detailed safety reference. |
| `ECOSYSTEM_ABSORPTION_PLAN.md` | Historical ledger of already admitted/adapted source material and provenance | No. It is historical evidence, not an admission of every upstream asset. |
| `ecosystem-admission.lock.json` and `verify-ecosystem-admission.py` | Existing four-source authority/admission guard | No. v2 extends it; v1 remains a regression oracle until v2 is independently accepted. |
| `upstream-lock.v2.json` and `verify-upstream-lock-v2.py` | Planned nine-source immutable intake ledger | Not yet active. The draft must report `BLOCKED`, never `PASS`. |
| Lumen's local `LUMEN-NEXTGEN-EXECUTION-BOOK-2026-08-01.md` | Read-only Core design and delivery-contract input from another working session | Never a Science dependency until it becomes a canonical immutable source A/evidence B tuple with the required gates. |

## Non-negotiable ordering

1. Lumen `P0_NR_SAFETY_GATE` must close unsealed replay/resubmit before R0.
2. `LUMEN_R0_SOURCE_GATE` makes a clean canonical Core source A plus evidence-only suffix B consumable.
3. C1 produces `PLATFORM_API_GATE`; R0 alone never implies a public Science extension port.
4. Science source intake may proceed in parallel, but a source catalog is not a runnable capability.
5. Existing actor-gated `seq_analyze`, Motif, Desktop and provenance work remain fixtures and parity oracles; they are not discarded.
6. No document, CI result, local binary, release tag, or provider claim substitutes for a different evidence layer.
