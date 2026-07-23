# Official Grok vs Lumen

## Two binaries on this machine

| | Official Grok | Lumen |
|--|---------------|--------|
| Command | `~/.grok/bin/grok` (or `grok`) | `~/.local/bin/lumen` |
| Source | xAI install / update channel | Built from `/Users/lei/code/lumen` (Grok Build OSS pin) |
| Default config home (FINAL-5UX Gate B) | `~/.grok` | **`$LUMEN_HOME` or `~/.lumen`** |
| Product chrome | Grok Build | **Lumen** |

Lumen **reuses the Grok Build engine** (session, tools, TUI core). It is **not** guaranteed to match the official client version or default model (often `grok-4.5` + login).

## Config homes

```text
Lumen product config:  $LUMEN_HOME  or  ~/.lumen
Legacy Grok data:      $GROK_HOME   or  ~/.grok   (migration source only)
```

### Clean dogfood (recommended)

```bash
export PATH="$HOME/.local/bin:$PATH"
export LUMEN_HOME="$HOME/.lumen-dogfood"
export DEEPSEEK_API_KEY='…'   # BYOK — do not commit
mkdir -p "$LUMEN_HOME"
cd /path/to/your/project
lumen
```

### Migrate legacy `~/.grok` config into Lumen home

Programmatic API (tests / tools):

- `xai_grok_config::dry_run(legacy, lumen)`
- `xai_grok_config::apply_migration(legacy, lumen)` → writes `migration-receipt.json` (no secrets)

Does **not** delete `~/.grok` (official Grok client may still need it).

## Auth

- **DeepSeek / API-key providers:** connect via env key; primary UI should say **Connect provider**, not “Sign in to Grok”.
- **xAI OAuth:** optional when using xAI models; official Grok login remains valid for the **official** binary.

## Related docs

- Multi-provider catalog: `docs/user/multi-provider.md`
- Local models: `docs/user/local-models.md`
- UX target: `docs/masterplan/11-FINAL-5UX-目标态规格.md`
