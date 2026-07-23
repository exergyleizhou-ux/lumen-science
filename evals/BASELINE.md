# Coding Eval Baseline — Lumen M4

> Harness: `./scripts/eval-coding.sh`
> Rules: 20+ tasks, each with `prompt.txt` + broken `workspace/` + deterministic test.
> Parity 12 and security smoke are separate — NOT counted here.

## Task Index (20)

| ID | Slug | Language | Category |
|----|------|----------|----------|
| T01 | 01-average-empty | Go | Empty slice / div zero |
| T02 | 02-stack-lifo | Go | Data structure |
| T03 | 03-reverse-runes | Go | String handling |
| T04 | 04-binary-search | Go | Algorithm |
| T05 | 05-counter-race | Go | Concurrency |
| T06 | 06-stringer-impl | Go | Interface |
| T07 | 07-nilmap-write | Go | Nil map |
| T08 | 08-multifile-shapes | Go | Multi-file |
| T09 | 09-py-divzero | Python | Division by zero |
| T10 | 10-py-json-merge | Python | Dict merge |
| T11 | 11-ts-optional-chain | TypeScript | Null handling |
| T12 | 12-ts-async-race | TypeScript | Async timeout |
| T13 | 13-go-context-cancel | Go | Context cancellation |
| T14 | 14-go-error-wrap | Go | Error wrapping |
| T15 | 15-py-path-traversal-fix | Python | Security fix |
| T16 | 16-go-http-timeout | Go | HTTP timeout |
| T17 | 17-multi-pkg-go | Go | Multi-package |
| T18 | 18-fix-only-regression | Go | Regression safety |
| T19 | 19-readme-driven | Go | README-driven impl |
| T20 | 20-flaky-to-stable | Go | Flaky test fix |

## Baseline Runs

| Model | Date | Pass | Total | Notes |
|-------|------|------|-------|-------|
| harness | 2026-07-16 | 0/20 (broken OK) | 20 | Harness-only: all 20 workspaces intentionally broken |
| deepseek-chat live | 2026-07-16 | **20/20** | 20 | `./scripts/eval-coding-live.sh` → `artifacts/readiness/eval-live.json`; silent_corruption=0; gate ≥18/20 |

## Harness Verification

```bash
./scripts/eval-coding.sh
```

All 20 broken workspaces should report BROKEN (test failure detected), proving the harness correctly identifies unfixed tasks.

## Live agent Verification

```bash
export DEEPSEEK_API_KEY=…
./scripts/eval-coding-live.sh   # writes artifacts/readiness/eval-live.json
```

Gate: `pass_count >= 18` and `silent_corruption == 0` (PASS requires deterministic tests after agent edit — not self-report).
