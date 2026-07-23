# Lumen Science data model

Implements S1, S2, and S4. All identifiers and ownership relationships are checked by the Rust store.

- `ProjectId`, `RunId` (UUIDv7), `CallId`: opaque identifiers.
- `RunContext`: immutable project, session, owner, workspace, provider, approval policy, tool profile, artifact root, and environment.
- `RunRecord`: context plus explicit lifecycle state.
- `Event`: schema version, run, monotonic `seq`, actor, timestamp, typed payload.
- `Artifact`: producer run/call, safe relative path, SHA-256, bytes, MIME and preview type.
- `Evidence`: claim plus source/artifact references and verification time.
- `Provenance`: source URI/commit/path/license, retrieval time, input hash, tool/environment.
- `Approval`: project/run/call ownership plus pending or explicit terminal decision.

Recovery rewrites every pending approval to the explicit terminal
`interrupted` decision before marking its non-terminal run `interrupted`.

The store uses one directory per run and atomic temp-file rename. Persist failure is returned to the caller and must transition the run to `failed` when the directory remains writable. Corrupt JSON is an error, never an empty graph.
