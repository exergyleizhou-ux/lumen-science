# Open Science — Provenance Record

| Field | Value |
|-------|-------|
| Repository | https://github.com/aipoch/open-science.git |
| Branch | main |
| Commit | d8f11e34314fdfa36f750cdb617af1cc2f30bace |
| Version | 0.7.1 |
| License | Apache-2.0 |
| Import Date | 2026-07-26 |
| Imported By | Lumen Science (exergyleizhou-ux/lumen-science) |
| Purpose | Desktop UI shell, notebook UX, preview, reviewer, skills, compute, release |
| Authority Model | Read-only asset directory; NO execution authority in Lumen context |
| Modification Policy | Per Apache-2.0 §4: modified files MUST carry prominent notices |

## Import scope (by priority)

| Priority | Module | Destination in Lumen | Status |
|----------|--------|---------------------|--------|
| OSF-1 | src/renderer/ (desktop shell) | packs/science-desktop/src/renderer/ | absorbed (branding + ResearchShell) |
| OSF-2 | files/preview authority path | packs/science-desktop/src/main/files/* | **wired** bind/seed/preview + UI catalog |
| OSF-3 | notebook UX + plan path | packs/science-desktop/src/main/files/notebook-* | **wired** plan/dry-run/export; execute via ACP only; OS kernel stubs remain |
| OSF-4 | reviewer UX + verdict proj | packs/science-desktop/src/main/files/review-* | **wired** artifact-bound submit; OS orchestrator stubs remain |
| OSF-5 | skills import/admit path | packs/science-desktop/src/main/files/skill-* | **wired** quarantine+DS-43; no bulk auto-approve; OS skill modules staged for UI only |
| OSF-6 | compute UX + plan path | packs/science-desktop/src/main/files/compute-* | **wired** dry-run plan; SSH/SCP runners remain stubs |
| OSF-7 | connector catalog UX | packs/science-desktop/ | catalog only; Rust adapters stay |
| OSF-8 | electron-builder.yml + release | packs/science-desktop/ | scaffold present; notarization/certs deferred |

## Authority boundary (NON-NEGOTIABLE)

The following Open Science subsystems SHALL NOT be granted execution authority
in Lumen Science Desktop:

1. **Agent framework / multi-agent dispatch** — Lumen uses Rust SessionActor.
2. **Full Access permission semantics** — hard-deny policy prevails.
3. **Generic local MCP execution** — only registered/exact-hash MCP allowed.
4. **Electron direct persistence of authoritative state** — only UI state.
5. **Notebook/SSH direct TypeScript executors** — must route via Rust.
6. **Second kernel authority** — no Electron main as fallback science runtime.

Imported code for these subsystems serves as VISUAL/DATA REFERENCE only.
Runtime wiring goes through Lumen ACP paths exclusively.
