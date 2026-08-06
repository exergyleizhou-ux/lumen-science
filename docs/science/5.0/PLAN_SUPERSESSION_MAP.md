# NextGen Plan Supersession Map

**Status:** 2026-08-06 canonical map (dialectical revision against canonical Lumen 2.2); it records document roles, not feature completion.

| Document | Role from 2026-08-06 | May it be deleted or treated as obsolete evidence? |
|---|---|---|
| `LUMEN_SCIENCE_NEXTGEN_CANONICAL_EXECUTION_BOOK_2026-08-06.md` | **Canonical Lumen Science execution order** (current): 2026-08-06 facts freeze (Lumen 2.2 released, kernel absorbed into canonical main, ACP seam exists), gate re-map with receipts, re-oriented strangler (X-M1 push-up), card table, milestones, final DoD | No. This is the only document allowed to change global ordering or status semantics. |
| `LUMEN_SCIENCE_NEXTGEN_CANONICAL_EXECUTION_BOOK_2026-08-02.md` | Previous canonical plan: 2026-08-02 facts freeze and full detail oracle (nine-source 14-step, W waves, scientific validity, G1, STOP matrix, card DoD templates). Superseded for ordering and current status only. | No. Retain as historical evidence and detail oracle. |
| `NEXTGEN_CANONICAL_EXECUTION_BOOK.lock.json` and `verify-nextgen-canonical-book.py` | Hash, required-contract, pointer and acyclic-DAG guard for the canonical book; enforces receipt-backed pin_eligible and PASS/PASS_UPSTREAM evidence | No. It proves plan integrity only and must keep top-level pass count at zero until separate runtime gates pass. |
| `NEXTGEN_GATE_REGISTRY.json` (schema-1) and `NEXTGEN_GATE_REGISTRY_V2.json` (granular) with `verify-nextgen-baseline.py` / `verify-nextgen-gates-v2.py` | Machine gate truth. PASS_UPSTREAM = upstream delivered with receipts (not Science completion); PASS = Science-side verified receipt | No. A non-PASS status is an explicit truth; PASS without receipt is rejected by the verifiers. |
| `NEXTGEN_BASELINE.json` | 2026-08-06 auditable starting snapshot (Science HEAD, plan inputs, canonical Lumen observation with R0 receipt) | No. It records the starting point only. |
| `LUMEN_SCIENCE_NEXTGEN_FINAL_EXECUTION_BOOK_2026-08-01.md` | Previous canonical plan and detailed historical oracle | No. It is superseded for ordering and current status; retain its evidence and detail. |
| `EXTREME_ADOPTION_SINGLE_BASE_EXECUTION_PLAN_2026-08-01.md` | Nine-source rights/asset intake and single-Rust-base migration detail | No. It remains the source-intake and migration reference; direction and order per 08-06 book X-M1. |
| `NEXT_GENERATION_AUTONOMY_CONTROL_PLANE_EXECUTION_PLAN_2026-08-01.md` | Authority, delegation, memory, Advisor, Kairos, and anti-pattern semantics | No. It remains the detailed safety reference; prerequisite status per 08-06 book §4.1. |
| `ECOSYSTEM_ABSORPTION_PLAN.md` | Historical ledger of already admitted/adapted source material and provenance | No. It is historical evidence, not an admission of every upstream asset. |
| `ecosystem-admission.lock.json` and `verify-ecosystem-admission.py` | Existing four-source authority/admission guard | No. v2 extends it; v1 remains a regression oracle until v2 is independently accepted. |
| `upstream-lock.v2.json` and `verify-upstream-lock-v2.py` | Nine-source immutable intake ledger | Not yet active. The draft must report `BLOCKED`, never `PASS`. |
| canonical Lumen's own docs (`LUMEN-NEXTGEN-EXECUTION-BOOK-2026-08-01.md`, v2.x release chain) | Read-only Core design/delivery-contract input and, since 2026-08-06, a release-backed source (v2.0.0/v2.1.0/v2.2.0 tuples) | Never a Science dependency until consumed via V0 verification + X-C1 contract; never edited by Science. |

## Non-negotiable ordering (2026-08-06)

1. N0: S0-A (PR #28 Linux red) and S0-B (Desktop skill mutation fail-close) stay the first executable cards; neither is done.
2. F0 (this map, baseline, gate registries, verifiers) is delivered with this book; machine gates must stay green.
3. V0 independently re-verifies every PASS_UPSTREAM receipt from canonical Lumen 2.2 before Science consumes it; PASS_UPSTREAM is never a Science completion claim.
4. X-C1 formalizes the existing `x.ai/science/*` seam (versioned method catalog + compat manifest + consumer fixture) — this is the re-scoped PLATFORM_API work, no longer "wait for Lumen owner".
5. X-M1 pushes the six Science-only modules (seqbench, primer_thermo, dossier, skill_quarantine, capability, features) into canonical `xai-grok-science` via draft PRs, then deletes the copied Core; no new authority may be added to the 0.1.222 copy (M1-A0).
6. Nine-source intake (I1-A completeness now, I1-B active after completeness + per-source gates) keeps `draft` semantics.
7. S2a shadow-only corpus and W0/W1/W3 product waves follow V0/X-C1; G1 macOS waits for SINGLE_BASE + E4 + lumen NG10/UPDATER receipts.
8. No document, CI result, local binary, release tag, or provider claim substitutes for a different evidence layer; PASS without receipt is rejected by the verifiers.
