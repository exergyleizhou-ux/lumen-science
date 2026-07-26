# Lumen Science — Product Status (honest)

**As of:** 2026-07-26  
**Baseline:** `main` @ `fa34f9b`+ (post version-truth / desktop builder fix)  
**Repo:** https://github.com/exergyleizhou-ux/lumen-science

## One-line judgment

> Solid **Rust Science kernel + CLI/MCP release assets** + large **desktop source skeleton** after Open Science absorb.  
> **Not** a repeatable, installable, auto-updating Desktop product yet. **Not** 5.0.

## Version truth

See [docs/VERSIONING.md](../VERSIONING.md).

| Component | Version | Status |
|-----------|---------|--------|
| Lumen Core (pager crate) | `0.1.222` | Shipping agent line |
| Science CLI/MCP | `1.0.0` (`packs/science/VERSION`) | Assets on GitHub `v1.0.0`; **tag→CI provenance still weak → need `v1.0.1`** |
| Science Desktop | `1.1.0-dev` | Source candidate only |

## Quantified maturity (audit-aligned)

| Scope | ~Done | Note |
|-------|------:|------|
| 1.0 feature code (Rust/Go offline) | 85% | Tests + gates strong |
| 1.0 formal release chain (tag→protected build) | 65% | Assets exist; provenance incomplete |
| OSF-0…8 source fusion | 70% | IPC spine OSF-2…8 in tree |
| OSF-0…8 desktop product evidence | 20–35% | No lockfile historically; builder fixed; CI desktop job added |
| 2.0 Desktop Preview | 35% | UI + IPC, not installable GA |
| 3.0 workbench GA | 25–30% | Live SessionActor E2E incomplete |
| 5.0 embodied | 10–15% | Device/HIL not started |

## What is trustworthy today

- Rust `SessionActor` sole execution authority.
- Connectors: 42 inventory / 40 implemented / 2 rejected / 0 unresolved (machine gates).
- Skills registry: 10 approved / 17 pending; bulk auto-approve denied on desktop path.
- Rust science tests + strict clippy: green on recent main (verify in CI).
- Go product packages: green.
- Desktop **product path code**: Files/Preview, Notebook plan, Reviewer, Skills quarantine, Compute dry-run, Connector catalog, Office fail-closed admission, release honesty IPC.
- Open Science pin: `d8f11e3` Apache-2.0 with NOTICE/ledger.

## Critical remaining gaps (priority)

### P0

1. **`v1.0.1` clean Science release** — preflight: `bash scripts/science-release-preflight.sh` + `python3 scripts/science_release_contract.py preflight --tag v1.0.1`. Still need **protected workflow tag build** (not just local preflight).
2. **Desktop reproducible build**: lockfile + `npm ci` OK; builder paths Lumenized. Full `electron-builder` package + typecheck still partial (deps/type debt from OS absorb).
3. **Desktop CI** workflow present; enable as **required** check in branch protection (manual GitHub setting).
4. **OSF-9 product-path**: offline composition test `test-osf9-product-path.mts` green (bind→preview→notebook→review→restart). Full installable Electron + exact binary hash E2E still open.

### P1

- Live connector matrix not 100% (arXiv 503 / S2 429 / OpenAlex key).
- Auto-update feed: disabled until Lumen-owned signed feed.
- Office converters: fail-closed until hostile-doc suite.
- README still mentions historical alpha text in places — align with VERSIONING.md.

## Roadmap (no version skip)

```text
1.0.1  CLI/MCP auditable release
1.1.0  Desktop alpha (lockfile + CI build + install smoke)
2.0.0  Project/Evidence/Preview/Replay desktop product
3.0.0  Notebook + Motif + Reviewer + Skills + controlled remote GA
4.0.0  Dummy Lab + Digital Twin
5.0.0  HIL + one low-risk device + human supervision
```

## Explicit non-claims

- Not 3.0 GA, not 5.0, not medical/device certified.
- Desktop line count ≠ product completeness.
- `osf8-release-check` / `release:checklist-status` do **not** prove electron-builder packages or GitHub asset provenance alone.
