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

## Batch 2: Files + Preview (OSF-2) — 2026-07-26

### Artifact/Files modules

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| (Lumen original) | packs/science-desktop/src/main/files/preview-resolver.ts | Yes | artifact_id resolve + trusted-context isolation |
| (Lumen original) | packs/science-desktop/src/main/files/session-identity.ts | Yes | main-process trusted owner/project (globalThis singleton) |
| (Lumen original) | packs/science-desktop/src/main/files/preview-service.ts | Yes | product entry loadArtifactPreview |
| (Lumen original) | packs/science-desktop/src/main/files/science-ipc.ts | Yes | single registration site: acp:* + files:preview-by-artifact |
| (Lumen original) | packs/science-desktop/src/main/files/acp-preview-store.ts | Yes | durable index + optional ACP artifact_resolve |
| (Lumen original) | packs/science-desktop/src/main/files/session-binding.ts | Yes | membership-gated bind + list seed |
| (Lumen original) | packs/science-desktop/src/main/files/acp-membership.ts | Yes | ACP project_assert_membership + artifact_list normalize |
| src/main/artifacts/ | packs/science-desktop/src/main/artifacts/ | Staged not wired | OS path; NOT registered in greenfield ipc.ts |
| src/main/managed-preview-*.ts | packs/science-desktop/src/main/ | Staged not wired | Banned channel path; use files:preview-by-artifact |
| src/main/office-preview/ | packs/science-desktop/src/main/office-preview/ | Planned | DOCX/XLSX/PPTX isolated renderer (follow-on) |
| src/renderer/.../previews/ | packs/science-desktop/src/renderer/ | Planned | Multi-tab preview UI (follow-on) |

### Batch 3 note (OSF-3 Notebook) — 2026-07-26

| Destination | Modified? | Notes |
|-------------|-----------|-------|
| packs/science-desktop/src/main/files/notebook-plan.ts | Yes (Lumen) | pure plan + ban patterns + ipynb export projection |
| packs/science-desktop/src/main/files/notebook-service.ts | Yes (Lumen) | dry-run local; execute only ACP notebook_execute |
| packs/science-desktop/src/main/notebook/kernel-executor.ts | Stub | EXECUTION AUTHORITY REMOVED |
| packs/science-desktop/src/main/notebook/ipc.ts | Stub | not registered in greenfield ipc.ts |

### Key modifications (OSF-2)
- Preview gated by artifact_id only (no arbitrary path open)
- Trusted session identity (main) vs store ownership — not client self-attestation
- Hash mismatch fail-closed (policy + resolver)
- Product IPC: `files:preview-by-artifact` via safeHandle allowlist
- Mock ipcMain registration test enforces Electron double-handle contract
- OS managed-preview / artifacts:read-preview remain banned
- Office preview renderer: dependency audit + hostile document tests still required

## Batch 3: Notebook UX (OSF-3) — 2026-07-26

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| src/main/notebook/ | packs/science-desktop/src/main/notebook/ | Planned | Python/R REPL control, history, IPYNB export, environment management, MCP server |
| src/renderer/src/pages/workspace/notebook/ | packs/science-desktop/src/renderer/ | Planned | Notebook UI, cell rendering, output display |
| src/renderer/src/stores/notebook-env-store.ts | packs/science-desktop/src/renderer/ | Planned | Environment management UX |
| src/shared/notebook*.ts | packs/science-desktop/src/shared/ | Planned | Notebook types and IPC contracts |
| resources/notebook/ | packs/science-desktop/resources/notebook/ | Planned | Python/R loops, REPL scripts |

### Key modifications (OSF-3)
- kernel-executor.ts: STUBBED — no Python/R kernel execution
- runtime-service.ts: STUBBED — no notebook runtime authority
- notebook/ipc.ts: STUBBED — no-op registerNotebookIpcHandlers
- Actual notebook execution requires wiring Rust KernelAdapter (follow-on)
- TypeScript kernel executor preserved in git history; replaced by stubs in working tree

## Batch 4: Reviewer UX (OSF-4) — 2026-07-26

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| src/main/reviewer/ | packs/science-desktop/src/main/reviewer/ | Planned | Rubric engine, artifact integrity, pass/warn/fail, stale detection, fix loop |
| src/renderer/src/stores/review-store.ts | packs/science-desktop/src/renderer/ | Planned | Review state management |
| src/shared/reviewer.ts | packs/science-desktop/src/shared/ | Planned | Reviewer types |

### Key modifications (OSF-4)
- Reviewer references artifacts by (artifact_id, expected_sha256, project_id), not file path
- Correction proposals routed through SessionActor (not direct execution)
- Reviewer verdicts enter EvidenceGraph (ReviewerVerdict node + supports/contradicts edges)
- Independent re-review on stale verdicts
- Fix-loop bounded; user can terminate

## Batch 5: Skills UX (OSF-5) — 2026-07-26

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| src/main/skills/ | packs/science-desktop/src/main/skills/ | Planned | Skill creation, ZIP/.skill import, GitHub preview/import, materializer, registry |
| src/renderer/src/pages/workspace/skills/ | packs/science-desktop/src/renderer/ | Planned | Skill selector UI |
| src/shared/skill-import-limits.ts | packs/science-desktop/src/shared/ | Planned | Import size and file count limits |
| resources/skills/ | packs/science-desktop/resources/skills/ | Planned | 18 built-in SKILL.md files (Open Science catalog) |

### Key modifications (OSF-5)
- Imported skills enter quarantine (not auto-approved)
- Lumen DS-43 admission required: hash → license → prompt-injection → tool list → human review
- Read-only materialization only after admission
- Open Science skills inventoried; Lumen's 10 approved + 17 pending count unaffected

## Batch 6: Remote Compute UX (OSF-6) — 2026-07-26

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| src/main/compute/ | packs/science-desktop/src/main/compute/ | Planned | SSH/SCP job model, concurrency, dispatch, polling, result harvesting |
| src/renderer/src/pages/workspace/compute/ | packs/science-desktop/src/renderer/ | Planned | Remote host UI, job submission, status, notifier |
| src/renderer/src/stores/compute-store.ts | packs/science-desktop/src/renderer/ | Planned | Compute state management |
| src/shared/compute.ts | packs/science-desktop/src/shared/ | Planned | Compute types |

### Key modifications (OSF-6)
- ssh-runner.ts: STUBBED — no SSH execution; throws 'use Rust Lumen'
- scp-runner.ts: STUBBED — no SCP execution
- job-poller.ts: STUBBED — no-op EventEmitter
- compute/ipc.ts: STUBBED — returns compatible stub shapes for ipc.ts destructuring
- compute-approval-broker.ts: STUBBED
- compute-service.ts: STUBBED — empty class
- job-dispatcher.ts: STUBBED — empty class
- harvest-engine.ts: STUBBED — returns rejection
- enabled-hosts-registry.ts: STUBBED
- Compute UI preserved; SSH/Slurm execution via Rust ToolAdapter (follow-on)
- TypeScript SSH/SCP stored in git history; replaced by stubs in working tree

## Batch 7: Connector Catalog UX (OSF-7) — 2026-07-26

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| src/main/connectors/ | packs/science-desktop/src/main/connectors/ | Planned | 24 connector groups, 200+ tools, catalog, metadata, health |
| src/renderer/src/pages/workspace/connectors/ | packs/science-desktop/src/renderer/ | Planned | Connector catalog UI, tool schema presentation, permission UI |

### Key modifications (OSF-7)
- Connector RUNTIME NOT imported; Lumen's 40 Rust adapters remain authoritative
- Only catalog UX, tool schema display, provider health/status, and permission UI imported
- New connector capabilities identified for Lumen gap analysis: ClinicalTrials, drug regulatory, GWAS/eQTL, Cancer Models, EMDB, BioMart, RNA tools, ZINC, molecule viewer

## Batch 8: Desktop Release Pipeline (OSF-8) — 2026-07-26

| Source Path | Destination | Modified? | Notes |
|-------------|-------------|-----------|-------|
| electron-builder.yml | packs/science-desktop/electron-builder.yml | Planned | macOS DMG+ZIP, Windows installer+ZIP, Linux AppImage+DEB |
| build/ | packs/science-desktop/build/ | Planned | Installer resources (icons, entitlements) |
| scripts/ (release) | packs/science-desktop/scripts/ | Planned | Release workflow, update feeds, blockmap |
| .github/workflows/ (release) | Reference only | Reference | Open Science release Actions |

### Key modifications (OSF-8)
- macOS notarization via Developer ID (org cert required later)
- Windows Authenticode (cert required later)
- SLSA provenance attestation adopted
- Update feeds adapted for Lumen endpoint
- Build artifacts: lumen-science binary bundled alongside Electron
