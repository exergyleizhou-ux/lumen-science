# Lumen Science M0 evidence — 2026-07-23

**Checkout:** `/Users/lei/code/lumen`  
**HEAD:** `8bd51b51ff874ecf035f52398898c2fbd40e9390`  
**Status:** M0 is **complete for its offline/fixture acceptance scope**. G1--G5
now have fresh terminal results on this checkout. This is not a live-provider
or billable-call claim.

## Protected pre-existing state

The following were present before M0 and were not modified, staged, reverted,
or formatted:

- `agent/crates/codegen/lumen-guard/src/bash.rs`
- `agent/crates/codegen/xai-grok-shell/src/agent/config.rs`
- `outputs/LUMEN_SCIENCE_FUSION_PLAN.md` (untracked)

No Cargo/rustc process was active before M0 started.

## Fresh gate results

| Gate | Exact command | Result | Evidence |
|---|---|---|---|
| G1 | `cargo test -p xai-grok-science --lib` | PASS: 59 passed, 0 failed, 2 ignored | `work/m0-evidence/g1-science-rerun.log` |
| G2 | `env -u DEEPSEEK_API_KEY -u KIMI_API_KEY -u KIMI_CODE_API_KEY -u XAI_API_KEY cargo test -p xai-grok-shell --lib --quiet` | PASS: 5674 passed, 0 failed, 13 ignored, exit 0 | `work/m0-evidence/g2-shell-lib-cleanroom.log` |
| G3 | `cargo clippy -p xai-grok-science --all-targets -- -D warnings` | PASS: exit 0 | `work/m0-evidence/g3-science-clippy-rerun.log` |
| G4 | `cargo build -p xai-grok-pager-bin --bin lumen` | PASS: exit 0; first full build 11m14s; evidence rerun 5m35s | `work/m0-evidence/g4-lumen-build-full.log` |
| G5 | `GROK_BINARY=target/debug/lumen cargo test -p xai-grok-shell --test test_built_binary_e2e science -- --ignored` | PASS: 7 passed, 0 failed, 0 ignored, exit 0 | `work/m0-evidence/g5-built-binary-science-e2e.log` |

## Completion observations

G4 compiled `xai-grok-tools-api` for more than three minutes with no new build
log and `rustc` at 0.0% CPU. G2 subsequently showed the same condition while
compiling `xai-computer-hub-sdk`. In both cases Cargo owned the target lock and
had a live rustc child; there was no competing Cargo/rustc process. M0 stopped
only the exact PIDs it had started and retained `CARGO_EXIT=143` in the logs.

A diagnostic retry with `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1` progressed
through substantially more crates, proving that the apparent idle windows are
not enough to establish a deadlock. It nevertheless did not produce a binary
or exit status within a bounded ten-minute observation window and was stopped
with exit 143. Its log is `work/m0-evidence/g4-no-incremental-diagnostic.log`.

The unbounded standard G4 build subsequently completed successfully; G5 then
used that fresh binary, rather than an older artifact.

The first un-sanitized G2 completion was not a source-green result: it produced
5665 passed, 9 failed, and 13 ignored (exit 101). The host process exposed
`DEEPSEEK_API_KEY`, `KIMI_API_KEY`, `KIMI_CODE_API_KEY`, and `XAI_API_KEY`.
The failures were all model/auth resolution expectations that observed
`deepseek-v4-*` credentials. The recorded passing G2 command removes exactly
those credential variables for its child process; it does not read, modify, or
log their values. Its 5674/0/13 result exactly matches the historical target.

## Source admission result

`fusion-sources.lock.json` was added with Rust Lumen as the only runtime
authority and all upstream sources in a pending/default-deny state. The lock
records the observed synsci source commit, its 293 `SKILL.md` files and 12
GPL-keyword matches. It contains no automatic license or capability admission.

## M1A admission decision

**M0 gate satisfied.** M1A may start only as an explicitly scoped, default-deny
admission task. The source lock remains 0 approved; this M0 completion does not
admit PubMed, any upstream skill, connector, data source, or live provider.
