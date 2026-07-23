# M2 progress — loop discipline — 2026-07-16

## Shipped
- Crate `lumen-discipline`: Storm, RepeatSuccess, Delivery, Goal gate, cache format
- Wired SoftOnce incomplete-todo gate into `update_goal` tool (before SessionActor)
- Model presets: deepseek-chat / deepseek-reasoner / local-openai (hidden)
- `scripts/smoke-m2.sh` + `docs/masterplan/08-M2-循环纪律.md`

## Demo paths
- Goal: incomplete todos → first `completed:true` → `incomplete_todos` error
- Storm: unit test third identical failure → nudge
- Cache: `format_cache_line` for status consumers

## Not yet (later M2 polish)
- Storm resource in live tool loop (inject nudge into model turn)
- Turn-end delivery reminder injection in shell
- Status bar widget binding for cache line
- 5 productivity days
