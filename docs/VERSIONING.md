# Component versioning (truth table)

Lumen Science monorepo carries **three independent version lines**. Do not
assume any two of them must be equal.

| Component | Source of truth | Current (as of 2026-07-30) | Meaning |
|-----------|-----------------|----------------------------|---------|
| **Lumen Core** (coding agent / pager) | Root `VERSION` **and** `agent/VERSION` **and** the eight Core crate `Cargo.toml` versions (kept in lockstep by `scripts/release_version.py check`) | `0.1.222` | Rust agent product. **Single Rust base is NOT complete** — Science still carries a forked Core copy with known drift against audited Lumen main head `dc563b1e…`. |
| **Lumen Science CLI/MCP** | `packs/science/VERSION` only | `1.0.1` | Frozen Go Science CLI + MCP release line (tag `v1.0.1`). Not a Rust Core version. |
| **Lumen Science Desktop** | `packs/science-desktop/package.json` `version` | `1.1.0-dev` | Electron desktop — **not GA** |

## Rules

1. **Never** require Core `0.1.x` to equal Science CLI `1.0.x` in release contracts.
2. Science release tags (`v1.0.x`) refer to **CLI/MCP** assets, not Desktop and not Rust Core.
3. Desktop must not claim GA until lockfile + CI build + install smoke exist.
4. Root `VERSION` is the **Rust Core** product version (`0.1.222` today). It must match
   `agent/VERSION` and the eight Core crate manifests. It is **not** the Go CLI version.
5. Do **not** bump Core metadata to `0.1.251` until source parity is actually achieved.
   Known Core drift against audited Lumen main head `dc563b1e…` is machine-gated at **136** files
   (`131` diverged + `5` missing) by `scripts/check-core-drift.py` +
   `docs/science/5.0/core-v0.1.251-admission.lock.json`. The target tag
   `v0.1.251` (`88a204cf…`) remains separate and is not the comparison checkout.
   That Core number deliberately excludes the Science domain crate. Its separate,
   commit-to-commit inventory is `scripts/report-science-crate-drift.py` +
   `docs/science/5.0/science-crate-drift-audit-2026-08-01.json`; it records 53
   duplicated-crate differences at the same audited source pin and is not folded
   into the Core admission lock.

## Released / planned tags

```text
v1.0.1     Science CLI/MCP — SHIPPED 2026-07-26
           (Science Release run 30192164855; MANIFEST git_commit=4f75d7a…)
v0.1.251   Upstream Lumen tag — NOT claimed as Science Core parity
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

Verify Rust Core version honesty:

```bash
python3 scripts/release_version.py --root . check
# prints 0.1.222 when root VERSION, agent/VERSION, and eight crates agree
```

Verify Core drift is not silently growing:

```bash
python3 scripts/check-core-drift.py --self-test
python3 scripts/check-core-drift.py \
  --science-root . \
  --upstream-root /path/to/lumen@dc563b1e… \
  --lock docs/science/5.0/core-v0.1.251-admission.lock.json
```
