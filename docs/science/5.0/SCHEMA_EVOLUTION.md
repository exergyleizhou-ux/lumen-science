# Lumen Science Schema Evolution Framework

**Status**: SPEC v1
**Date**: 2026-07-25
**Milestone**: LS5-4

## Principles

1. **Forward compatibility**: New binary can read old data.
2. **Atomic migration**: Migration succeeds completely or rolls back.
3. **Immutable history**: Old artifact hashes, events, and provenance are never rewritten.
4. **Fail closed**: Corrupt or unknown schema versions reject the store.
5. **Preserve unknowns**: Fields unknown to current reader are preserved through migration.

---

## Schema Identity

Every durable schema type carries:

```rust
struct SchemaIdentity {
    schema_id: String,           // e.g. "lumen-science-run-v1"
    schema_version: u32,         // monotonic
    minimum_reader_version: u32, // oldest version this reader supports
    migration_id: String,        // unique per migration step
    migration_checksum: String,  // SHA-256 of migration logic
    created_by_binary: String,   // binary version that wrote this
}
```

## Version Table

| Schema | V1 | V2 (planned) | V3 (planned) | V4 (planned) | V5 (planned) |
|--------|----|---------------|---------------|---------------|---------------|
| Run | 1 | 1 (compat) | 1 (compat) | 1 (compat) | 2 (project-owned) |
| Artifact | 1 | 1 | 1 | 1 | 2 (evidence edges) |
| Evidence | 1 | 2 (graph) | 2 | 2 | 3 (device) |
| Provenance | 1 | 1 | 1 | 2 (compute env) | 2 |
| Project | - | 1 | 1 | 1 | 2 (collaboration) |
| WorkflowSpec | - | - | 1 | 2 (device steps) | 2 |
| DeviceCommand | - | - | - | 1 | 2 (calibration) |
| ExperimentSession | - | - | - | 1 | 2 (target_mode) |

## Migration Contract

Every migration step must:

1. **Backup**: Create pre-migration snapshot.
2. **Validate**: Verify source store integrity.
3. **Migrate**: Apply transformations in order.
4. **Verify**: Check destination store integrity.
5. **Journal**: Record migration in append-only journal.
6. **Commit** or **Rollback**: Atomic decision.

## Crash Recovery

- Interrupted migration: auto-recovery on next open
- Corrupt journal: rollback to pre-migration backup
- Missing backup: fail closed (do not attempt migration)

## Rollback

- Old binary can open store if no migration has committed
- Once migration commits, old binary must reject the store
- Rollback binary: reads new store, ignores unknown fields
- Explicit rollback: restore from pre-migration backup

## Unknown Field Preservation

- JSON-based schemas: preserve unknown keys in a `__extras` map
- Binary schemas: append-only, unknown fields at end with length prefix
- On write-back: merge preserved unknowns with current fields
