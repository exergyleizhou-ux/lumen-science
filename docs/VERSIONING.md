# Component versioning (truth table)

Lumen Science monorepo carries **three independent version lines**. Do not
assume root `VERSION` equals Desktop or the Rust pager crate.

| Component | Source of truth | Current (as of 2026-07-26) | Meaning |
|-----------|-----------------|----------------------------|---------|
| **Lumen Core** (coding agent / pager) | `agent/crates/codegen/xai-grok-pager/Cargo.toml` `version` | `0.1.222` | Rust agent product |
| **Lumen Science CLI/MCP** | `packs/science/VERSION` (primary); root `VERSION` still used by some science install scripts | `1.0.1` | Offline science CLI + MCP release line (tag `v1.0.1` via Science Release workflow) |
| **Lumen Science Desktop** | `packs/science-desktop/package.json` `version` | `1.1.0-dev` | Electron desktop — **not GA** |

## Rules

1. **Never** require Core `0.1.x` to equal Science `1.0.x` in release contracts.
2. Science release tags (`v1.0.x`) refer to **CLI/MCP** assets, not Desktop.
3. Desktop must not claim GA until lockfile + CI build + install smoke exist.
4. Root `VERSION` historically mixed meanings; prefer `packs/science/VERSION` for
   science tooling going forward. Root remains `1.0.0` for backward-compatible
   scripts until release_contract is split (tracked as follow-on).

## Released / planned tags

```text
v1.0.1     Science CLI/MCP — SHIPPED 2026-07-26
           (Science Release run 30192164855; MANIFEST git_commit=4f75d7a…)
1.1.0      Desktop alpha (reproducible build + installable + ACP smoke)  ← next
2.0.0      Desktop product (Project/Evidence/Preview/Replay)
3.0.0      Notebook + Reviewer + Skills + controlled remote GA
4.0 / 5.0  Dummy Lab / HIL — not started
```

Verify a Science release:

```bash
gh release view v1.0.1 --repo exergyleizhou-ux/lumen-science
# MANIFEST.json: product, version, git_commit, builder_run_id, independentFromCore
# sha256sum -c SHA256SUMS
```
