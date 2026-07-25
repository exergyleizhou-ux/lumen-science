# Provenance — jvogan/motif (MotifRenderer)

| Field | Value |
|-------|-------|
| upstream | https://github.com/jvogan/motif |
| commit | 876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0 |
| version | 0.2.1 |
| license | MIT |
| role in Lumen | Optional molecular-biology **renderer** only |
| runtime authority | none |
| admission | pending-per-file-and-dependency-review |

## Integration contract

- SessionActor remains sole execution and permission authority.
- Motif UI may only load **registered** artifacts (project/session/owner + SHA-256).
- Renderer output, if any, is a **new derived artifact** with its own hash.
- Upstream Motif MCP server, Claude Science installer, and external MSA CLIs are **not** wired.

## Notices

- Preserve upstream `LICENSE` and `THIRD_PARTY_NOTICES.md` on redistribution.
- See `third_party/motif/NOTICE` and `docs/science/MOTIF_SUPPLY_CHAIN_AUDIT.md`.
