# Legal — Lumen 终检（FINAL-2.0 M6）

**Status:** SIGNED for engineering publish package  
**Date:** 2026-07-16  
**Authority:** `docs/masterplan/` + desktop FINAL-2.0

## 1. Product statement

Lumen is an **independent** coding agent product based on an Apache-2.0 derivative of
SpaceXAI Grok Build (`https://github.com/xai-org/grok-build`). It is **not** an
official xAI, SpaceXAI, Anthropic, OpenAI, or DeepSeek product.

## 2. License inventory

| Component | Path | License | Action |
|-----------|------|---------|--------|
| Agent runtime (Grok Build pin) | `agent/` | Apache-2.0 | Retain `agent/LICENSE` |
| Third-party Rust crates | `agent/THIRD-PARTY-NOTICES` | Various | Retain notices |
| Root NOTICE | `NOTICE` | Apache-2.0 attribution | Ship with binary |
| Vertical packs (science/oasis/quant) | `packs/` | Lumen authors + noted deps | Keep pack READMEs |
| Eval harness tasks | `evals/tasks/` | Lumen / original fixtures | Internal eval only |
| SBOM | `SBOM.spdx.json` | Generated SPDX-2.3 | Regenerate on release |

## 3. 终检 checklist（发布前必勾）

- [x] Apache-2.0 NOTICE / LICENSE present and not rewritten as proprietary-only
- [x] No claim of official xAI / SpaceXAI / Anthropic affiliation in user-facing CLI strings
- [x] Telemetry defaults **off** in product path (assert-defaults / smoke-security)
- [x] User API keys (`DEEPSEEK_API_KEY`) never committed to git
- [x] Upstream pin documented in `agent/UPSTREAM.md` + `SOURCE_LOCK.json`
- [x] SBOM generated via `./scripts/generate-sbom.sh` → `SBOM.spdx.json`
- [x] Trademark: CLI brand is **Lumen** (not “official Grok”)
- [ ] Human counsel review for commercial redistribution jurisdictions (optional beyond private beta)
- [ ] 15-day self-use gate (M6 productivity) — **human**, not legal counsel

## 4. Distribution rules

1. Distribute binary **with** `NOTICE`, `LEGAL.md`, and `agent/LICENSE` (or equivalent bundle).
2. Do not strip SPDX / copyright headers from vendored sources.
3. Third-party model APIs (DeepSeek etc.) are **BYOK** — Lumen does not resell model access.
4. Vertical packs that include scientific data or third-party assets must keep their own notices.

## 5. Evidence

| Artifact | Purpose |
|----------|---------|
| `NOTICE` | Apache attribution |
| `agent/LICENSE` | Upstream Apache-2.0 |
| `agent/THIRD-PARTY-NOTICES` | Crate inventory |
| `SBOM.spdx.json` | Machine-readable package list |
| `SOURCE_LOCK.json` | Pin + critical file hashes |
| `artifacts/readiness/reconcile.json` | Source/binary/evidence tuple |

## 6. Sign-off

| Role | Result | Notes |
|------|--------|-------|
| Engineering legal package | **PASS** | Files above present; SBOM generated; no secret leakage in tracked tree |
| Human productivity M6 | **OPEN** | Not a legal item; tracked by `productivity-gate.sh` |
| External counsel | N/A private beta | Required before public marketplace if applicable |

Regenerate SBOM after dependency bumps:

```bash
./scripts/generate-sbom.sh
./scripts/reconcile-evidence.sh
```
