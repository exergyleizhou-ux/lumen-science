# EVIDENCE: Lumen Expert E2/E3 — release tag v0.1.221

## Manifest

| Field | Value |
|-------|-------|
| release_tag | `v0.1.221` |
| release_commit | `fabceea0238654865a301a45b2bd91b09f702cda` |
| expert_e3_commit | `fd6aa2d` |
| followthrough_commit | `0b05bc1` |
| post_tag_ci_only_commits (NOT in tag) | `fa49b0c`, `2da3de8` |
| source_tree | git worktree `/tmp/lumen-v0.1.221-audit` @ `fabceea` |
| tested_by | `exergyleizhou-ux` |
| tested_at | `2026-07-20T03:36:47Z` – `03:43:51Z` |
| host | macOS arm64 (Darwin) |

## Isolation method

```bash
WT=/tmp/lumen-v0.1.221-audit
git worktree add "$WT" v0.1.221
cd "$WT/agent"
git rev-parse HEAD    # fabceea0238654865a301a45b2bd91b09f702cda
# All builds and tests run inside this isolated worktree.
```

No edits were made inside the worktree. The tag was checked out read-only.

## Verification matrix (tag `fabceea` ONLY)

Every result below was produced from the isolated worktree at tag `v0.1.221`.
No post-tag CI fix (`fa49b0c`, `2da3de8`) is reflected in any row.

| name | command (cwd = agent/) | exit_code | duration_s | log_sha256 | result |
|------|------------------------|-----------|------------|------------|--------|
| cargo check | `cargo check -p xai-grok-shell` | 0 | 107 | `56bb2d5e6f9eb44dcb7fcbd9d3301572d68807a97ec4827d64b2122fc6e9b962` | pass |
| expert_consultant_tools | `cargo test -p xai-grok-shell --lib expert_consultant_tools -- --test-threads=1` | 0 | 162 | `04b5c5b467e1e7c50addc7eb632da24bbcdb6fbdec36b2da50eac79472aad88d` | pass (20/20) |
| expert_model_restore | `cargo test -p xai-grok-shell --lib expert_model_restore -- --test-threads=1` | 0 | 28 | `6318e5427bf5607acea9dd5f54215a3ee1d9c8ead4d2e924b42422fb02b2dca1` | pass (7/7) |
| zero_http | `cargo test -p xai-grok-shell --lib zero_http -- --test-threads=1` | 0 | 28 | `566bfe9d8221a181b71fada24750438c351332ee0eb3b6ae13e59284a229ee68` | pass (1/1) |
| dual_two | `cargo test -p xai-grok-shell --lib dual_two -- --test-threads=1` | 0 | 31 | `99680c3e2f508a7bdbeddae699df42561bf33cd2912aef55a9db901dab236a3b` | pass (1/1) |
| session::expert:: | `cargo test -p xai-grok-shell --lib 'session::expert::' -- --test-threads=1` | 0 | 35 | `c7d986bbc29ffded4302a3adbe6e47d188ef850e023c9c5beef2efb6851da396` | pass (26/26) |
| goal_compose | `cargo test -p xai-grok-shell --lib goal_compose -- --test-threads=1` | 0 | 33 | `f296cefd244a25a1217c0609d42f6f8e82e86ceb4bedd1836f7b5aac01e01f23` | pass (6/6) |

## Explicit non-claims

- **Post-tag CI fixes (`fa49b0c`, `2da3de8`) are NOT in the tag.** The tag CI run
  (`29713267337`) was cancelled/not-run due to GitHub Actions queue issues.
  Current `main` CI status does NOT imply the tag is green.
- **GitHub Actions release workflow for `v0.1.221`** was queued without runners.
  A companion release `v0.1.221-macos` carries the local-release binary asset.
- **Full workspace clippy** is not claimed on this tag (only `cargo check`).
- **No "every phase × every failure mode" matrix** — only the E3-critical paths
  (consultant tools, dual, model restore, zero-HTTP, compose, session::expert unit).

## Dual / consultant tools / iron rules checklist

| Rule | Status |
|------|--------|
| SessionActor sole truth | PASS |
| Single Writer = Executor | PASS |
| Consultant advisory only (no write, no completion authority) | PASS |
| `/goalexpert` is Goal alias only | PASS |
| No `SetDefaultModel` in Expert path | PASS |
| Durable reserve before HTTP; barrier fail ⇒ zero HTTP | PASS (test: `zero_http`) |
| Dual source A/B are two independent requests with distinct IDs | PASS (test: `dual_request_ids_differ*`) |
| Dual both fail → empty plan, `dual_failed` verdict | PASS (test: `dual_both_sources_fail*`) |
| Dual merge is deterministic, `merge_algorithm=deterministic-v1` | PASS |
| Consultant readonly tools: deny globs, path sandbox, binary reject, redact | PASS (17+ tests) |
| Git diff: `--no-ext-diff`, `--no-textconv`, env scrub | PASS (code review) |
| No cargo probe from diagnostics/test_summary without snapshot | PASS (test: `read_diagnostics_without_snapshot_*`) |
| Repair resets HostVerification to Unknown | PASS (test: `repair_resets_host_verification_pass`) |

## Follow-up commits after this evidence

The hardening commit that supplements this evidence will be on branch
`codex/expert-prove-and-harden`. Its diff adds:

- Dual-budget pre-check before source B
- VerificationSummary `workspace_fingerprint`, `executor_pass`, `clear_for_repair_pass()`
- Git/sandbox hardening
- Diagnostics/test_summary snapshot-only fallback
- This evidence file

## Raw log archive

Raw output lines available at `/tmp/lumen-v0.1.221-evidence-raw.txt`.
