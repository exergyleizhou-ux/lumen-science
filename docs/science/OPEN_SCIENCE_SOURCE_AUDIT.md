# Open Science Source Audit

**Date**: 2026-07-26
**Subject**: Apache-2.0 licensed Open Science v0.7.1
**Commit**: d8f11e34314fdfa36f750cdb617af1cc2f30bace
**Repository**: https://github.com/aipoch/open-science.git

## Audit Summary

Open Science is a desktop Electron/React application providing a complete
scientific workbench UI including project management, notebook, artifact
preview, reviewer, skills management, remote compute, and automated release
pipeline with notarization.

Licensed under Apache-2.0, which permits:
- Commercial use ✓
- Modification ✓
- Distribution ✓
- Private use ✓
- Patent use ✓

With requirements:
- License and copyright notice ✓ (retained in third_party/)
- State changes ✓ (NOTICE documents modifications)
- Include license ✓ (third_party/open-science/LICENSE)

## Import Decision

**APPROVED** for import under the following constraints:

1. Import scope limited to priority modules (see provenance record)
2. All execution authority removed; Lumen Rust SessionActor remains sole authority
3. Connector runtime unchanged (Rust adapters); only catalog UX imported
4. Persistence limited to UI state; science state owned by Lumen Rust stores
5. No second agent-framework runtime; no "Full Access" passthrough
6. All modified files carry Apache-2.0 header + modification notice per §4

## Risk Assessment

| Risk | Severity | Mitigation |
|------|----------|------------|
| Electron as second execution path | HIGH | Authority boundary enforced in bridge; no direct executor wiring |
| Open Science persistence collides with Lumen | MEDIUM | Separate storage roots; UI-only state in Electron |
| Connector divergence | LOW | Rust connector inventory unchanged; only catalog UI imported |
| License compliance drift | LOW | Provenance record + NOTICE + IMPORT_LEDGER per file batch |

## References

- third_party/provenance/open-science.md — provenance record
- third_party/open-science/LICENSE — full Apache-2.0 text
- third_party/open-science/NOTICE — modifications and credits
- IMPORT_LEDGER.md — per-file import tracking
