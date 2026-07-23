# Lumen prompt-cache stack (DeepSeek-first · multi-provider)

Status: **implemented core** (discipline crate + truth-bar auto-feed).  
Source inspiration: Reasonix `cache_shape` / REASONIX.md cache-first — **not** a Go port.

## Goals (surpass Reasonix)

| Capability | Reasonix | Lumen |
|------------|----------|-------|
| Byte-stable system+tools prefix | yes | yes (discipline + comments + delivery tail) |
| Prefix shape / miss reasons | `cache_shape` | `lumen_discipline::{capture_shape,compare_shape}` |
| Session rolling hit + streak | limited | `SessionCacheTracker` + stability score 0–100 |
| Multi-provider adaptation matrix | DeepSeek-centric | `profile_for_model` (DeepSeek / OpenAI / Anthropic / local / generic) |
| Status cache line | yes | `format_cache_line` + rich line |
| Truth bar auto-feed | n/a | PromptResponse `_meta` → `note_truth_cache` |
| Cache-impact PR guard | scripts | `scripts/check-cache-impact.sh` |

## DeepSeek (default product path)

- Mechanism: **automatic prefix cache**.
- Value: **High** — default model `deepseek-chat` is chosen for this.
- Rules:
  1. Never put session counters, delivery state, storm state, or clock into the **system prefix**.
  2. Dynamic reminders go on the **turn tail** only (`DELIVERY_REMINDER`, tool results).
  3. Prefer stable tool schema order; avoid mid-session tool list churn.
  4. Show definitive cache % only from **provider-reported** tokens.

## Other models (adaptation)

| Family | Mechanism | Value | What to do |
|--------|-----------|-------|------------|
| DeepSeek | AutomaticPrefix | High | Full discipline; default |
| OpenAI GPT / o* / many xAI Grok | AutomaticPrefix | Medium | Same prefix rules; % only if reported |
| Claude / Anthropic Messages | ExplicitBreakpoints | Medium | Prefix stable + `cache_control` breakpoints when wiring Messages |
| Moonshot / Qwen / GLM / MiMo | ReportedOnly | Medium | Discipline + report-gated display |
| Local (Ollama, LM Studio, vLLM, exo) | None | None | No cloud cache claim; hygiene only |

API: `lumen_discipline::profile_for_model(model_id, base_url)`.

## Never claim

- Estimated cache as definitive hit (ui_contract: `CacheSource::ProviderReported` only).
- Compaction as a cache hit (separate metric).
- “All models have DeepSeek-level savings” — false; matrix above.

## Code map

- `lumen-discipline` — format, shape, session tracker, multi-provider matrix, request_prefix
- `xai-grok-shell/session/prompt_cache_registry.rs` — per-session observe/bump (no Actor field)
- `turn.rs` — observe every model call (system+tools shape + usage)
- `compaction.rs` — `bump_log_rewrite` on compact
- `updates.rs` — mid-turn `cacheHitRatio` / profile on session notifications
- `SessionInfoData` — cache_hit_ratio / stability / profile / cache_line for `/session-info`
- Messages API — system last block + **last tool** `cache_control: ephemeral`
- Pager — PromptResponse meta + mid-turn meta → `note_truth_cache`; `/session-info` prints cache line

## PR hygiene

Touching prompt assembly, tools schemas, default models, delivery reminders, or sampler usage fields requires:

```
Cache-impact: <none|low|medium|high> — <reason>
Cache-guard: <test name or rationale>
```

Run: `./scripts/check-cache-impact.sh`
