# Security Policy

## Reporting a vulnerability

Report suspected vulnerabilities through **GitHub private vulnerability
reporting**: <https://github.com/exergyleizhou-ux/lumen-science/security/advisories/new>

Please do not open a public issue for a suspected vulnerability.

Include where practical: affected component and version, reproduction steps or
proof of concept, observed versus expected behaviour, and any conditions
required (a specific platform, a configured connector, an enabled feature gate).

We aim to acknowledge within 5 working days. This is a small project, not a
funded security team — treat that as a best effort, not a contractual SLA.

## Scope

Lumen Science ships three independently versioned components
(see [`docs/VERSIONING.md`](docs/VERSIONING.md)):

| Component | Source of truth | In scope |
|---|---|---|
| Lumen Science CLI/MCP | `packs/science/VERSION` | Yes |
| Lumen Core (agent/pager) | `agent/crates/codegen/xai-grok-pager` | Yes |
| Lumen Science Desktop | `packs/science-desktop/package.json` | Yes, **pre-GA** |

Current machine-readable status, including which components have which level of
verification evidence: [`docs/science/status/current.json`](docs/science/status/current.json).

### Especially interested in

- Bypasses of the execution-authority boundary — anything that lets the Electron
  main process, the renderer, a plugin, a skill, or a connector perform an
  operation the Rust `SessionActor` did not authorise.
- Escapes from artifact preview containment: path traversal, symlink or hardlink
  tricks, TOCTOU between capability grant and file read.
- Evidence-graph integrity: causing an artifact digest to be accepted without
  matching registered bytes, or two distinct artifacts to collapse onto one node.
- Supply-chain issues in the release pipeline: anything that lets published
  release assets be replaced, or a tag to be bound to a commit it was not built
  from.
- Update-channel attacks. Updating is disabled unless Lumen-owned signing
  material is configured (see `packs/science-desktop/src/shared/update-policy.ts`);
  a way to re-enable it or redirect it is in scope.

### Known and already documented

The following are **known** and tracked; reports restating them are welcome but
will be closed as duplicates. Please read
[`docs/science/status/current.json`](docs/science/status/current.json)
(`authority.knownAuthorityGaps`) first.

- Science release assets are unsigned and carry no SBOM or provenance
  attestation.

### Out of scope

Physical security, host OS or kernel compromise, side-channel attacks, social
engineering, and vulnerabilities in third-party services a connector talks to
(report those to the service owner). See
[`docs/science/5.0/THREAT_MODEL.md`](docs/science/5.0/THREAT_MODEL.md).

## Devices and laboratory hardware

Lumen Science contains data models for laboratory device control. Real-device
execution (`DeviceCommand`, `HardwareInLoop`, `RealDevice`) is **disabled** and
there is no supported path to enable it.

If you find a way to reach a real-device code path in a shipped build, treat it
as a **critical** report. Do not test against physical equipment.

## Supported versions

Only the latest release of each component receives fixes. There is no long-term
support branch.

## Disclosure

We prefer coordinated disclosure and will credit reporters who want credit. If
you plan to publish, tell us your intended date so a fix and advisory can be
ready.
