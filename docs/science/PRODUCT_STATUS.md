# Lumen Science — Product Status (honest)

> **Machine status is authoritative:** [`status/current.json`](status/current.json),
> generated from the source tree and verified in CI. Where this prose and that
> file disagree, the file is right and this document is a bug.

**As of:** 2026-07-26  
**Baseline:** `main` @ `4f75d7a` (Science `v1.0.1` tag + Science Release run `30192164855`)  
**Repo:** https://github.com/exergyleizhou-ux/lumen-science

## One-line judgment

> **Science CLI/MCP `1.0.1` is a real, workflow-built release** (MANIFEST + SHA256SUMS + multi-platform assets).  
> Desktop remains a **source candidate / CI-gated skeleton**, not an installable GA product. **Not** 5.0.

## Version truth

See [docs/VERSIONING.md](../VERSIONING.md).

| Component | Version | Status |
|-----------|---------|--------|
| Lumen Core (pager crate) | `0.1.222` | Shipping agent line |
| Science CLI/MCP | `1.0.1` (`packs/science/VERSION`) | **Shipped** — GitHub Release [`v1.0.1`](https://github.com/exergyleizhou-ux/lumen-science/releases/tag/v1.0.1); `MANIFEST.json` `git_commit=4f75d7a…`, `builder_run_id=30192164855`, `independentFromCore=true` |
| Science Desktop | `1.1.0-dev` | Authority suite + **unsigned pack-dir** (`Lumen Science Desktop.app`); not installable GA / no notarization |

## Quantified maturity (audit-aligned)

| Scope | ~Done | Note |
|-------|------:|------|
| 1.0 feature code (Rust/Go offline) | 85% | Tests + gates strong |
| 1.0 formal release chain (tag→protected build) | **95%** | Science Release workflow green; MANIFEST provenance verified; optional GPG/minisign still open |
| OSF-0…8 source fusion | 70% | IPC spine OSF-2…8 in tree |
| OSF-0…8 desktop product evidence | 45–55% | lockfile + Desktop CI + authority suite + unsigned pack-dir branded app |
| OSF-9 product-path | 80% | offline composition + live binaryHash (SHA-256 of real `lumen-science`); headed Electron GUI E2E still open |
| 2.0 Desktop Preview | 35% | UI + IPC, not installable GA |
| 3.0 workbench GA | 25–30% | Live SessionActor E2E incomplete |
| 5.0 embodied | 10–15% | Device/HIL not started |

## What is trustworthy today

- Rust `SessionActor` sole execution authority.
- Connectors: 42 inventory / 40 implemented / 2 rejected / 0 unresolved (machine gates).
- Skills registry: 10 approved / 17 pending; bulk auto-approve denied on desktop path.
- Science CLI/MCP **v1.0.1** assets built by Actions (not local re-upload): 35 files in `SHA256SUMS` + `MANIFEST.json`.
- Desktop authority suite is a **required** status check on `main` (`enforce_admins=true`).
- Desktop **product path code**: Files/Preview, Notebook plan, Reviewer, Skills quarantine, Compute dry-run, Connector catalog, Office fail-closed admission, release honesty IPC, OSF-9 offline composition.
- Open Science pin: `d8f11e3` Apache-2.0 with NOTICE/ledger.

## Critical remaining gaps (priority)

### P0 (done for Science 1.0.1)

1. ~~**`v1.0.1` clean Science release**~~ — **done** (tag `v1.0.1` → workflow Science Release → publish).
2. ~~**Desktop CI as required check**~~ — **done** (`Desktop authority suite` + `enforce_admins`).
3. ~~**Science vs Core release classify**~~ — **done** (Core `Release Lumen` no-ops science tags).

### P0 (still open — Desktop path)

1. **Desktop installable package**: signed DMG/AppImage/Setup + auto-update feed (unsigned pack-dir proof exists via `npm run dist` / `scripts/pack-dir.mts`).
2. **OSF-9 headed Electron E2E**: full GUI drive-through (live binaryHash probe is in authority suite; headed app E2E still open).
3. **Full electron-vite product build**: pack-dir uses a minimal main/preload/renderer shell; full `src/main/index.ts` graph still has OS-absorb dep/type debt.

### P1

- Optional release signatures (GPG/minisign) for CLI assets.
- Live connector matrix not 100% (arXiv 503 / S2 429 / OpenAlex key).
- Auto-update feed: disabled until Lumen-owned signed feed.
- Office converters: fail-closed until hostile-doc suite.
- README historical alpha wording in places — align with VERSIONING.md.

## Roadmap (no version skip)

```text
1.0.1  CLI/MCP auditable release          ← SHIPPED 2026-07-26
1.1.0  Desktop alpha (lockfile + CI build + install smoke)
2.0.0  Project/Evidence/Preview/Replay desktop product
3.0.0  Notebook + Motif + Reviewer + Skills + controlled remote GA
4.0.0  Dummy Lab + Digital Twin
5.0.0  HIL + one low-risk device + human supervision
```

## Explicit non-claims

- Not Desktop GA, not 3.0 GA, not 5.0, not medical/device certified.
- Desktop line count ≠ product completeness.
- `osf8-release-check` / `release:checklist-status` do **not** prove electron-builder packages alone.
- Science `v1.0.1` does **not** ship Desktop installers.
