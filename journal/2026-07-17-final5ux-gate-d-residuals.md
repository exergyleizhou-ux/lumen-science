# FINAL-5UX Gate D–F engineering residuals (`codex/final-5ux-gate-d`)

Honest inventory for handoff. **Not** a product `ready=true` claim.

## Human gates only (cannot automate)

| Residual | Why |
|----------|-----|
| M5 onboarding (≤10 min human path + evidence) | Requires a real human run + gate artifacts |
| M6 productivity (15-day self-use) | Human diary / productivity gate |

## Engineering still limited by environment / external systems

| Residual | Status |
|----------|--------|
| Interactive multi-step first-run wizard (provider picker walkthrough) | In-session recovery is shipped (`/probe`, `/status` Recovery, chat-only block). Full first-run chrome wizard still depends on auth/startup surfaces outside this package. |
| Isolated HTTP capability probe without a live turn | `/probe` → Checking; Tool-ready from live ACP tool_call or external `scripts/probe-local.sh` / `apply_truth_probe`. No fake Tool-ready. |
| Provider cache auto-feed | **Shipped**: PromptResponse `_meta` → `feed_truth_cache_from_prompt_meta` → truth bar; `lumen-discipline` SessionCacheTracker + multi-provider matrix (`policy/LUMEN_CACHE.md`). Mid-turn session/update meta still optional. |
| Full interactive PTY color matrix (truecolor/256/16/NO_COLOR × sizes) | Unit matrix 80/120/180 drives shipped `truth_bar::line` + status Recovery; full PTY harness is env/auth gated (`--ignored`). |
| Exhaustive docs/billing SuperGrok string deletion | Legal/upstream/xAI commerce paths retained by design. |

## Implemented on this branch

- Shared `Arc<TruthSnapshot>` + truth bar (fullscreen / minimal / dashboard)
- Redacted `/status` + Recovery section; click ≡ `/status` ≡ dashboard path
- `/probe` → Checking + recovery (never invents Tool-ready)
- Runtime: model switch, live tool_call, execute→verification, edit→stale, permission sync
- Chat-only edit block + recovery report on permission path
- Static welcome logo; idle welcome `TickDemand::None`
- Contract-gated `install_truth_snapshot`
- Colour-independent size matrix tests (80×24 / 120×40 / 180×50 semantics)
