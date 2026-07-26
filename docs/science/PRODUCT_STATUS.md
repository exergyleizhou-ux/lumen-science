# Lumen Science — Product Status (honest)

**As of**: 2026-07-26  
**Repo**: exergyleizhou-ux/lumen-science  
**Strategy**: Month-1 — end architecture fork. Not 5.0 GA.

## Product label (current)

```text
PRODUCT:     Lumen Science Desktop (fusion in progress)
RELEASE:     1.1.0-dev desktop shell + OSF-2 authority path
CORE:        Rust SessionActor is sole science execution authority
LOCAL:       CLI/connectors offline-strong; desktop product path partial
EMBODIED:    Deferred (4.0/5.0) — Dummy Lab / HIL / real devices NOT done
MEDICAL:     Not certified
```

## Version roadmap (locked)

| Label | Meaning | Status |
|-------|---------|--------|
| 1.0.x | CLI/MCP offline product + formal downloadable assets | 1.0.0 tag exists; **binaries on GitHub Release still incomplete (P0)** |
| 2.0 | Desktop + ResearchProject + EvidenceGraph product surface | Preview models in Rust; desktop fusion WIP |
| **3.0 GA** | Notebook + Multimodal + Reviewer + Skills + Remote (authorized) | Target after Month 1–2 fusion |
| 4.0 | Dummy Lab + Digital Twin | Deferred |
| 5.0 | HIL + one low-risk device + supervised loop | **Preview only — do not announce GA** |

## Month-1 scope (what “done” means here)

**In scope**

1. Single Rust authority for science execution.
2. Desktop shell (Open Science UI absorb) with execution authority stripped.
3. Files/Preview by `artifact_id` + membership-gated session bind.
4. UI project/session open path that never becomes second authority.
5. Honest provenance (Open Science `d8f11e3`, Apache-2.0).
6. Packaging scaffold (electron-builder); notarization when certs exist.
7. No new connectors, no device types, no scope expansion.

**Out of scope this month**

- Device / BOS / HIL / real lab.
- Bulk skill auto-approve.
- Replacing 40 Rust connectors with Open Science runtime.
- Claiming 5.0 or “all OSF-0…9 complete”.

## Open Science absorb policy

| Source | Pin |
|--------|-----|
| https://github.com/aipoch/open-science.git | `d8f11e34314fdfa36f750cdb617af1cc2f30bace` (v0.7.1, Apache-2.0) |

Reference only (later Motif workbench): https://github.com/jvogan/motif.git — **not** Month-1.

Absorb **UI / preview / notebook UX / release engineering**.  
**Never** absorb OS multi-agent authority, Full Access bypass, TS SSH/kernel executors as production authority.

## Architecture fork status

| Surface | Authority | State |
|---------|-----------|--------|
| Connectors / evidence / workflow (Rust) | SessionActor | Strong (offline tests) |
| Desktop IPC science path | `acp:*` + `files:*` + `notebook:*` + `review:*` via safeHandle | OSF-2/3/4 wired |
| Notebook TS KernelExecutor | **Stub only** | Live path: ACP `notebook_execute` only |
| Reviewer TS orchestrator | **Stub only** | Live path: ACP `start_review` |
| Dossier gold path | Fixture-driven Q→P→E→R→R | Projection end-to-end |
| OSF-5 Skills admission | Quarantine import; single admit | Bulk auto-approve denied; registry 10/17 |
| OSF-6 Remote Compute | Dry-run plan only | No desktop SSH/SCP; live execute denied |
| OS `projects:*` / `artifacts:*` / TS compute | **Banned** | Not registered in greenfield `ipc.ts` |
| UI project catalog | Electron UI state only | Local catalog + membership hybrid |
| Go science pack | CLI/MCP compatibility | Not product authority |

## Sharp wedge (Month-2, not this commit)

**Target/Disease Research Dossier** (computational biology only):

```text
Question → Plan → Literature/DB → Artifacts → Notebook → Motif check
→ Reviewer → EvidenceGraph → Reproducible package → Replay
```

User-facing concepts only:

```text
Question · Plan · Evidence · Result · Review
```

## Known P0/P1 (from audit)

| ID | Issue | Disposition |
|----|--------|-------------|
| P0 | GitHub `v1.0.0` missing downloadable binaries listed in SHA256SUMS | Track as 1.0.1 release ops; not fixed by desktop code alone |
| P1 | Rust strict clippy `-D warnings` red | Separate quality PR; tests green |
| P1 | Desktop still imports large OS surface; many preload channels point at banned IPC | Month-1: Lumen `files:*` + `window.api.lumen`; OS channels fail-closed |

## Metrics (targets — not yet measured)

- Install → first Project: &lt; 5 min  
- Install → first evidenced ResearchResult: &lt; 15 min  
- Artifact registration rate for formal claims: 100%  
- Fixture replay consistency: 100%
