# IMPORT_LEDGER

| When | Source | Destination | Policy |
|------|--------|-------------|--------|
| 2026-07-16 | ~/Desktop/grok-build-main | agent/ | Full pin; exclude target/.git |
| 2026-07-16 | ~/lumen evals/tasks 01-08 | evals/tasks/ | Tier1 coding eval |
| 2026-07-16 | new tasks 09-20 | evals/tasks/ | Tier2/3 |
| 2026-07-16 | ~/lumen internal science/oasis/quant | packs/ | Verticals |
| 2026-07-16 | FINAL-2.0 desktop doc | docs/masterplan/00A,09 | Contract extract (no full re-import) |

Secrets: never import `.env`, `*.pem`, API keys into git.
Refresh lock: `./scripts/source-lock.sh`
