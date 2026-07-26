# Import Ledger — Open Science → Lumen Science Desktop

Source repo: https://github.com/aipoch/open-science
Source commit: d8f11e34314fdfa36f750cdb617af1cc2f30bace
Source license: Apache-2.0

Each entry records: source path, destination, modifications, and dependency changes.

---

## Batch 0: License + Provenance (OSF-0) — 2026-07-26

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| LICENSE | third_party/open-science/LICENSE | No | Original Apache-2.0 text |
| — | third_party/open-science/NOTICE | N/A | New file; attributes upstream, lists Lumen modifications |
| — | third_party/provenance/open-science.md | N/A | New file; provenance record |
| — | docs/science/OPEN_SCIENCE_SOURCE_AUDIT.md | N/A | New file; audit decision and risk assessment |
| — | IMPORT_LEDGER.md | N/A | This file |

---

## Batch 1: Desktop Shell (OSF-1) — 2026-07-26

### renderer/ (React UI — full copy)

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| src/renderer/** | packs/science-desktop/src/renderer/ | Planned | React pages, components, hooks, styles. Execution authority removed at IPC boundary. |
| src/main/index.ts | packs/science-desktop/src/main/ | Planned | Electron entry; launch adapted to Lumen binary |
| src/main/windows.ts | packs/science-desktop/src/main/ | Planned | Window management |
| src/main/tray.ts | packs/science-desktop/src/main/ | Planned | System tray |
| src/main/update/ | packs/science-desktop/src/main/ | Planned | Auto-updater |
| src/main/settings/ | packs/science-desktop/src/main/ | Planned | Settings persistence (UI only) |
| src/preload/** | packs/science-desktop/src/preload/ | Planned | Preload scripts |
| src/shared/** | packs/science-desktop/src/shared/ | Planned | Shared types |
| electron-builder.yml | packs/science-desktop/ | Planned | Release packaging |
| resources/ | packs/science-desktop/resources/ | Planned | App icons, assets |

### New Lumen-only files (not from Open Science)

| File | Purpose |
|------|---------|
| packs/science-desktop/src/main/lumen-acp-bridge.ts | ACP bridge replacing agent-framework authority |
| packs/science-desktop/ARCHITECTURE.md | Authority model and IPC channel policy |
| packs/science-desktop/package.json | Lumen-adapted package manifest |

### Authority removal summary

The following Open Science subsystems have execution authority REMOVED:
- src/main/agent-framework/ → execution paths disconnected; Lumen bridge used instead
- src/main/notebook/ executor → references kept for visual design; actual execution via Rust
- src/main/reviewer/ executor → same treatment
- src/main/compute/ SSH runner → same treatment
- src/main/skills/ approval → admission routed through Lumen DS-43
- Any persistence of science state → routed to Rust stores

---

## Batch 2: Files + Preview (OSF-2) — pending

(TBD)

---

## Batch 3: Notebook UX (OSF-3) — pending

(TBD)

---

## Batch 4: Reviewer UX (OSF-4) — pending

(TBD)

---

## Batch 5: Skills UX (OSF-5) — pending

(TBD)

---

## Batch 6: Remote Compute UX (OSF-6) — pending

(TBD)

---

## Batch 7: Connector Catalog UX (OSF-7) — pending

(TBD)

---

## Batch 8: Desktop Release Pipeline (OSF-8) — pending

(TBD)
