# Open Science `fd2853` skill-quarantine admission mapping

| Field | Value |
|---|---|
| Repository | https://github.com/aipoch/open-science.git |
| Exact commit | `fd2853f0b9bdb6c063ccc1e741687584ab94bf9a` |
| License | Apache-2.0 |
| Upstream input | `src/main/skills/zip-extract.ts` (`613b5ae735796472e477d041d0525c248799087ccb4aeaf1251a3dc17bed9bed`) |
| Lumen implementation | `agent/crates/codegen/xai-grok-science/src/skill_quarantine.rs` |
| Authority | Rust `SessionActor` only |

## Exact scope

This is a source-level Rust adaptation of the upstream archive-reader problem:
central-directory validation, raw ZIP-name safety, compression/resource bounds,
and deterministic archive-file selection.  The Rust implementation makes the
admission stricter because a later approval could commit bytes to a durable
Science store: malformed, unsupported, unsafe, ambiguous, case-colliding, or
over-budget entries reject the entire request before `Begin` opens a run.

The adaptation does **not** adopt `src/main/skills/materializer.ts`.  That
module copies and enables live skills, which would bypass the product rule here:
after Allow, `skill_quarantine_import` stores only hashed
`quarantine/original.skill` and `quarantine/manifest.json`, with
`materialized=false` and `enabled=false`.  It never writes `.grok/skills`,
`.lumen/skills`, or a loose import inbox.

## Evidence boundary

The associated capability record may claim E4 only for the recorded local
rebuilt-binary ACP tests.  It does not claim exact-head CI, package/release,
installation, live import, execution, or permission to enable a skill.
