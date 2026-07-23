# Lumen Science M0 baseline

Date: 2026-07-23

## Scope and protection

Product scope is the Rust Lumen TUI checkout at
`/Users/lei/code/lumen`, `main@8bd51b51ff874ecf035f52398898c2fbd40e9390`.
The checkout has these pre-existing protected changes, which M0 must not edit,
stage, revert, or format:

- `agent/crates/codegen/lumen-guard/src/bash.rs`
- `agent/crates/codegen/xai-grok-shell/src/agent/config.rs`
- `outputs/LUMEN_SCIENCE_FUSION_PLAN.md` (untracked)

No Cargo or rustc process was active at M0 start. Other registered worktrees
are out of scope. The residual aipoch checkout is not a product baseline and
is not to be recovered or modified.

## Authority invariant

Rust Lumen TUI is the sole execution, approval, verification, artifact,
evidence, provenance, and Run/Goal state authority. Upstream Open Science,
synsci, and Motif assets are capability references only. They cannot introduce
a second runtime authority.

## Source admission record

`fusion-sources.lock.json` records the observed upstream identities and the
default-deny admission policy. A root repository license is not enough to admit
an individual skill, renderer, connector, model, data source, or dependency.
Each admitted asset must add its exact upstream path, source SHA, SPDX status,
dependency licenses, data terms, runtime requirements, permissions, fixture,
and evidence level.

## Historical evidence to re-run, not current-green claims

The following is user-provided historical evidence and a revalidation target.
M0 must capture fresh commands, logs, current HEAD, and results before treating
any gate as current:

| Gate | Historical target |
|---|---|
| G1 | `xai-grok-science`: 59 passed, 0 failed, 2 ignored |
| G2 | `xai-grok-shell --lib --quiet`: 5674 passed, 0 failed, 13 ignored |
| G3 | strict science clippy clean |
| G4 | `xai-grok-pager-bin` builds the `lumen` binary |
| G5 | built-binary science ACP product path: 7 passed, 0 failed |

The G5 test names are the seven ignored product tests in
`xai-grok-shell/tests/test_built_binary_e2e.rs`; revalidation requires a
freshly built `target/debug/lumen` supplied via `GROK_BINARY`, then runs the
science tests with `--ignored`. A passing fixture path is L4, not L5 live
provider proof.

## M0 completion conditions

1. Preserve the protected state above and record final `git status`.
2. Complete the source lock and mark every candidate `approved`, `rejected`,
   or `pending`; no implicit admission.
3. Re-run G1--G5 on this checkout without credentials or billable provider
   calls, retaining exact logs and exit status.
4. Compare the new results with the historical targets; report regressions,
   timeouts, or inability to reproduce without calling them green.
5. Do not start M1A until M0 has an explicit pass/block decision.
