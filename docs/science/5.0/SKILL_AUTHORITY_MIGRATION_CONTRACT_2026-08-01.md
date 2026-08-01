# Skill Authority Migration Contract

## Status and scope

This is an implementation contract, not evidence that the migration is
complete.  It records the remaining Desktop skill authority surface after
`4f7244b` closed the direct GitHub and agent-home import mutations.

The only admitted mutation path today is an uploaded ZIP/`.skill` bundle:

```text
Desktop sender identity
  -> settings:import-skill-zip[-batch]
  -> x.ai/science/skill_quarantine_import
  -> SessionActor Begin / durable pending approval / Allow-only Finish
  -> hashed ScienceStore archive + manifest + evidence + provenance
```

Its success state is deliberately `materialized=false` and `enabled=false`.
Quarantine is not execution, installation, or a general permission grant.

## Current boundary inventory

| Desktop operation | Current source | Current effect | Authority status | Required destination |
|---|---|---|---|---|
| ZIP bundle import | `files/science-ipc.ts` | immutable archive/manifest only | actor-gated | retain as compatibility input |
| GitHub URL import | `settings/ipc.ts` | formerly copied into imported store | fail-closed since `4f7244b` | source receipt + actor submission |
| agent-home import | `settings/ipc.ts` | formerly copied into imported store | fail-closed since `4f7244b` | source receipt + actor submission |
| create personal skill | `settings/service.ts:createSkill` | writes mutable personal store | **legacy bypass remains** | prepared authored-skill admission |
| update personal skill | `settings/service.ts:updateSkill` | rewrites mutable personal store | **legacy bypass remains** | new immutable revision admission |
| delete imported/personal skill | `settings/service.ts:deleteSkill` | deletes mutable store entry | **legacy bypass remains** | actor-owned revocation/tombstone |
| enable/disable skill | `settings/service.ts:setSkillEnabled` | updates local disabled set and reloads runtime | **legacy bypass remains** | actor-owned activation record |
| runtime materialization | `settings/service.ts:materializeAgentSkills` | copies enabled local entries into Claude/Codex/OpenCode config | **legacy bypass remains** | only actor-approved immutable revisions |

Read-only listing, preview and source scanning may remain Desktop operations.
They cannot create a candidate, activate a skill, or cause a runtime reload.

## Target contract

`SkillRevisionAdmissionV1` is one operation family, not a second Desktop
authority. Its kind is one of:

```text
authored-create | authored-revision | external-source-receipt |
activation | revocation
```

Every candidate has a canonical byte representation, deterministic revision
digest, declared source, owner/project/session binding and operation id. The
Desktop must not choose a store root, output path, owner, project or session.

The generic Lumen extension host must expose an operation with this state
machine:

```text
DraftBytes -> Prepared -> AwaitingApproval -> Running
           -> Succeeded(immutable revision) | Denied | TimedOut | Cancelled | Failed
```

Only a durable `Running + Allow` can publish an immutable revision artifact.
Denied, timed-out, cancelled, malformed, cross-owner, cross-project and
post-approval byte-swap requests publish neither revision artifact nor active
runtime entry. A duplicate operation id may replay only the byte-identical,
same-owner/project/session succeeded record.

An activation is separate from admission. It references an already succeeded
revision artifact by id and SHA-256, and produces an actor-owned activation
record. It cannot activate arbitrary local paths or a mutable `UserSkillRepository`
entry. Revocation is a tombstone; it never deletes scientific evidence.

## Implementation order

1. **Freeze the bypass.** Keep GitHub/home imports fail-closed and add a
   mechanical guard that rejects future direct calls to `importFromGitHub`,
   `importAgentHomeSkill`, `createPersonal`, `updatePersonal`, `delete`, or
   runtime reload from a shipping IPC mutation handler.
2. **Extract pure codecs.** Define canonical `SkillRevisionV1` bytes and
   `SkillSourceReceiptV1`; unit-test malformed UTF-8/frontmatter, unsafe slug,
   oversized content, duplicate operation id and digest mismatch. No store or
   filesystem writes in this step.
3. **Add the generic Core operation.** This belongs in canonical Lumen's
   versioned extension API, not another Science-specific `SessionCommand`.
   It must use the existing Begin/Prepared/permission/Finish cancellation and
   replay rules. Do not start this until `PLATFORM_API_GATE=PASS`.
4. **Build the Science adapter.** Convert ZIP, authored text and external
   source receipts into the generic request; preserve source URI/ref/license
   facts as provenance, never assert missing license facts.
5. **Read actor records for the catalog.** Replace `UserSkillRepository` as
   the shipped runtime source with actor-approved revision and activation
   projections. Legacy store is migration-read-only until explicit recovery
   import creates new admissions.
6. **Cut over materialization.** Materializer receives only immutable actor
   revision bytes and activation records, verifies artifact SHA-256, writes
   app-owned read-only copies, and refuses missing/revoked/cross-session
   records. It remains an adapter and cannot make approval decisions.
7. **Delete legacy mutation routes.** Remove create/update/delete/enable
   SettingsService handlers and their preload APIs only after parity corpus and
   rebuilt-binary proof pass. Do not merely hide buttons.

## Required negative and product proof

The migration is not complete until exact rebuilt Lumen + Desktop product tests
prove all of the following:

1. authored create and revision require a durable human Allow;
2. deny, timeout, cancel and caller-drop leave no candidate, active entry or
   materialized runtime file;
3. owner/project/session/workspace substitution fails closed;
4. a byte change after approval fails and leaves no artifact;
5. duplicate operation replays only the exact accepted revision;
6. activation requires an admitted immutable revision and cannot use a path;
7. revocation stops next runtime materialization without erasing evidence;
8. GitHub/home/direct SettingsService mutation calls cannot write or reload;
9. the materializer verifies artifact SHA-256 and never reads legacy mutable
   skill content in the shipped path.

CI must run the source-negative tests and the rebuilt-binary product proof.
macOS packaging, clean install/upgrade/rollback, signing/SBOM and authorized
live/provider validation remain separate gates.

## Non-goals

This contract does not authorize a live GitHub fetch, provider call, automatic
skill execution, auto-activation, conversion of third-party licenses, or a
second scheduler/permission service. It also does not justify expanding the
copied Rust Core: the generic operation waits for the canonical Lumen platform
extension API.
