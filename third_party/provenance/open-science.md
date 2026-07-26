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
| OSF-1 | src/renderer/ (desktop shell) | packs/science-desktop/renderer/ | planned |
| OSF-2 | src/renderer/ (file/preview) | packs/science-desktop/renderer/ | planned |
| OSF-3 | src/main/notebook/ (UX) | packs/science-desktop/main/notebook/ | planned |
| OSF-4 | src/main/reviewer/ (UX) | packs/science-desktop/main/reviewer/ | planned |
| OSF-5 | src/main/skills/ (UX) | packs/science-desktop/main/skills/ | planned |
| OSF-6 | src/main/compute/ (UX) | packs/science-desktop/main/compute/ | planned |
| OSF-7 | src/main/connectors/ (catalog UX) | packs/science-desktop/main/connectors/ | planned |
| OSF-8 | electron-builder.yml + release scripts | packs/science-desktop/ | planned |

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
