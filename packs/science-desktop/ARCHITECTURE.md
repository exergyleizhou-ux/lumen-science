# Lumen Science Desktop — Architecture

## Authority model

```
┌─────────────────────────────────────────────┐
│ Electron Renderer (React)                   │
│ - Project UI, notebook UX, preview, skills  │
│ - NO direct execution authority             │
│ - Communicates via IPC → Electron main      │
└──────────────┬──────────────────────────────┘
               │ IPC (whitelist only)
┌──────────────▼──────────────────────────────┐
│ Electron Main Process                       │
│ - Window, tray, menu, updater, notifications│
│ - Lumen binary lifecycle                    │
│ - ACP proxy (renderer → Rust)               │
│ - NO science execution; NO kernel authority  │
└──────────────┬──────────────────────────────┘
               │ ACP (authenticated loopback)
┌──────────────▼──────────────────────────────┐
│ Rust Lumen Binary (SessionActor)            │
│ - PermissionManager                         │
│ - ArtifactRegistry (SHA-256)                │
│ - EvidenceGraph (provenance)                │
│ - WorkflowActor (validated DAG)             │
│ - ProjectStore                              │
│ - KernelAdapter                             │
│ - ToolAdapters                              │
└─────────────────────────────────────────────┘
```

## Key invariants

1. **Single execution authority**: Rust SessionActor only. Electron never
   executes science tools, notebooks, or reviewers directly.

2. **No "Full Access" escape hatch**: even if the UI shows "full access",
   the Rust side enforces hard-deny policies.

3. **Artifact integrity**: every artifact goes through SHA-256 registration
   in Rust; the files/preview UI loads by artifact_id, not path.

4. **Evidence chain**: reviewer verdicts, workflow steps, and connector
   fetches all enter EvidenceGraph via Rust — never via Electron persistence.

5. **Binary attestation**: the Electron main process hashes the Rust binary
   on launch and exposes the hash to the UI for user verification.

## IPC channel policy

**Allowed** (UI-only, no side effects on science state):
- Window management, app lifecycle, tray, updater, settings dialog,
  file dialogs, notifications, clipboard, session layout save/restore.

**ACP-proxied** (validated by Rust before execution):
- All science operations go through `acp:call` → Rust tools/call.

**Banned** (rejected at Electron main before reaching Rust):
- Any direct project/artifact/notebook/reviewer/connector/compute IPC.

See `src/main/lumen-acp-bridge.ts` for the channel registry.

## Persistence split

| Layer | What it owns | Storage |
|-------|-------------|---------|
| Electron (main) | window bounds, recent projects list, tray state | electron-store in app data |
| Renderer | UI layout, accordion state, draft text | localStorage/Renderer context |
| Rust Lumen | projects, artifacts, evidence, workflow state, permissions, connector dispositions | `~/.lumen/science/` |

## Upstream credits

Desktop shell UI adapted from Open Science v0.7.1 (Apache-2.0).
Commit: d8f11e34314fdfa36f750cdb617af1cc2f30bace.
Modifications: brand, authority model, ACP bridge, IPC policy.
See third_party/open-science/NOTICE and IMPORT_LEDGER.md.
