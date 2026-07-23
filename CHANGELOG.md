# Changelog

All notable changes to Lumen are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.222] - 2026-07-20

### Fixed

- Upstream P1 under PINNED policy: `/summarize` alias for `/recap` (pager).
- Marketplace `require_sha` pin gate for remote plugin installs (`GROK_MARKETPLACE_REQUIRE_SHA` / `LUMEN_MARKETPLACE_REQUIRE_SHA`).
- P0 (already in 0.1.221 line): cancel/prompt `dispatch_locks`; OSC 52 kill switch.

### Notes

- Windows package deferred (toolchain not available on this Mac).
- Expert dual / lumen-guard / DeepSeek defaults: no intentional behavior change.


## [0.1.221] - 2026-07-20

### Added

- CI gate, host cmd timeout, real evidence tools, dual-B readonly tools (`0b05bc1`)
- complete E3 dual read-only consultation and rollout (`fd6aa2d`)
- dual copy/light-load safety tests (`f2e6318`)
- E3 dual two-source proposals and rollout gates (`ecd8cd7`)
- E3 dual two-source proposals and rollout gates (`997a389`)
- complete E2 vision and bounded review workflow (`56f5291`)
- compose Expert policy with Goal orchestration (`64a955f`)
- complete E1 session expert workflow (`476ae92`)
- complete DeepSeek V4 expert E0 readiness (`d14f6dd`)

### Fixed

- skip denied dirs in search and filter-before-cap list (`a1f97dd`)
- harden consultant readonly tool host sandbox, redaction, timeout, and testing (`bee4695`)
- canonicalize workspace root in consultant path sandbox (`c9a0611`)

## [0.1.220] - 2026-07-19

### Added

- Immutable four-platform release artifacts with target-scoped SPDX SBOMs and Minisign signatures.
- Automated release preparation with synchronized version bumping, changelog generation, signed tags, and GitHub Actions publishing.
- automate version and changelog preparation (`5d71669`)
- add immutable Lumen release foundation (`9566940`)
- merge codex/production-ready-p1-rust truth hardening (P1) (`877ecbd`)
- wire StormBreaker, RepeatSuccessGuard, DeliverySessionState into agent loop (`b933176`)
- wire goals and harden provider boundaries (`b156143`)
- update to DeepSeek V4 Pro + V4 Flash (was V3/R1) (`ae6cb91`)
- 三层审核 — 硬脚本 + AI分析 + 人终审 (`8e051d7`)
- 增强审核 skill — 结构化报告 + 用户审批 + 逐文件分类 (`448088d`)
- 自更新系统 — self-update.sh + skill + review-upstream + memory (`ebea119`)
- rebrand Grok Build → Lumen (`80ab3a7`)
- wire observe path, mid-turn feed, Anthropic breakpoints (`29f0ad6`)
- Reasonix-class DeepSeek-first stack + multi-provider matrix (`37dd695`)
- Lumen oasis pixel logo and Chinese greeting (`2adc3da`)
- readiness recovery, /probe, verification hooks (`b5ff836`)
- live tool_call truth probe + residual inventory (`491252c`)
- Gate D truth surfaces + runtime refresh (`fe51d36`)
- Gate C data — assemble TruthSnapshot from probe evidence (`9d5b778`)
- Gate B Lumen config home and product identity chrome (`a4412ec`)
- FINAL-5UX Gate A UI truth contract (`e301b99`)
- close readiness and human gates (`9d5d9f2`)
- add full L4/L5 localhost harness (`1dfd331`)
- bound verify-after-edit repair loops (`1a54360`)
- verify Go edits automatically (`9414614`)
- add honest local and science dogfood paths (`ab86f62`)
- full multi-provider catalog from legacy Go Lumen presets (`6b8291c`)
- multi-provider BYOK catalog (OpenAI/Claude/xAI/GLM/Qwen/MiMo/Ollama) (`9c5fec3`)
- SBOM, LEGAL, reconcile, R0-full, eval-live 20/20 (`d0b0f9a`)
- engineering_complete + honest M6 productivity gate (`20d5fe2`)
- Lumen UX polish — help strings, install-local, productivity diary (`e8eeeec`)
- user-visible Lumen CLI name, version, and help (`4bb5019`)
- sign L4-min fault recovery + L5-min continue/cache (`060b89e`)
- sign L2/L3 agent e2e + R0-min process kill contract (`cd24242`)
- S0 contract + L1 CanToolCall path (honest readiness) (`3ac838a`)
- vertical packs science/oasis/quant + doctor-verticals.sh (`aa3f68b`)
- coding eval tasks 01-20 + eval-coding runner + BASELINE.md (`6435a77`)
- lumen-verify crate — language detect, build/vet/test steps, diagnostics parse, repair state machine (`d5f0bd2`)
- stage eval tasks 01-08 (broken workspaces) (`fe45f9a`)
- CC_PARITY 41 rows + parity-run harness (≥80%) (`92c381b`)
- loop discipline — storm, delivery/goal gate, presets, cache line (`331fd09`)
- lumen-guard L0–L3 hard-deny wired before YOLO (`9802602`)
- ship release lumen with DeepSeek defaults (`e197369`)

### Fixed

- lockstep xai-grok-version and dynamic SBOM test tag (`c46a572`)
- resolve audit version and macOS network issues (`6d91080`)
- add missing storm_breaker/repeat_success_guard/delivery_state to 4 test SessionActor constructors (`a74305f`)
- correct DeepSeek names — V4 Flash (deepseek-chat), V4 Pro (deepseek-reasoner) (`2a37148`)
- sign BIN_SRC before copy to avoid checksum mismatch in install (`c5800d7`)
- macOS taskgated kills unsigned binary — add ad-hoc codesign to build/install (`08c1d7a`)
- export LUMEN_HOME for Gate B config path (`4ab22bc`)
- keep BYOK catalog + product default on session prefetch (`2f85ace`)
- Lumen product shell on cold-start paths (`c07d97e`)
- preserve event sequence across resume (`6e8e48c`)
- reconcile current-run evidence (`a9146c4`)
- use tool-capable Ollama default (`973614d`)
- fail closed on skipped publish gates (`b16ee44`)
- idempotent write when material fields unchanged (`46a399d`)
- stable artifact digests for idempotent runs (`c83c572`)
- write only reconcile.json (status owned by verify-readiness) (`c37f55f`)
- content-hash freshness for SOURCE_LOCK (not HEAD churn) (`5d9ad30`)
- do not auto-refresh SOURCE_LOCK (stop lock churn) (`198ce1d`)
- allow SOURCE_LOCK meta-commit without false drift (`e933d6f`)
- acceptance gates — honest eval harness, red T14/T20, verify CLI (`46f940a`)
- real DeepSeek BYOK routing + auto_update defaults (`4cf23a3`)

### Changed

- 极致模型参数 — temperature=0, max_tokens=8192, laziness_detector, reasoning_efforts, pricing (`a0f3f25`)
- 极致优化 — auto_compact 80%, laziness_detector, stream_tool_calls, reasoning_efforts, pricing (`3611bee`)

### Documentation

- note Go archive branch and Rust main product line (`5f63eb3`)
- update ENHANCEMENTS.md after P1 merge (`31b7609`)
- import FINAL-5UX spec and gap analysis vs 21ef079 (`905e68c`)
- map legacy Go modules to Rust runtime (`fd940eb`)
- handoff journal — all M4 exits met (`9766c7b`)
- Day 0 progress — monorepo import and cargo check green (`3790aa3`)

### Maintenance

- add sync-upstream.sh for tracking Grok Build updates (`6c95924`)
- record H_code acceptance evidence (`21ef079`)
- reconcile beta evidence at b16ee44 (`17fe70d`)
- pin SOURCE_LOCK to e933d6f HEAD (`bf64f71`)
- refresh SOURCE_LOCK to post-publish HEAD (`e6f0593`)
- update Cargo.lock for lumen-guard (`494bd76`)
- add scripts/verify-day0.sh for foundation acceptance gates (`57dc78a`)
- import grok-build as agent foundation (pinned Day 0) (`853a305`)
