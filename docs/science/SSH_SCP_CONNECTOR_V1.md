# Lumen Science SSH/SCP connector capability v1

Seam contracts: **S3, S4**. Status: C3 provides a real, bounded SCP transport
through the existing Rust `SessionActor`. Rust Lumen remains the sole
admission, permission, execution, persistence, and verification authority.

## Exact capability

This capability permits one project-scoped SCP put or get operation. It does
not grant remote shell execution, port forwarding, arbitrary local paths, MCP,
private-compute access, retries, or remote-job recovery.

Before permission or execution, Lumen requires:

1. Exact project and owner policy matches.
2. An allowlisted DNS-shaped hostname, port, SHA-256 host-key fingerprint,
   bounded timeout, and explicit egress permission.
3. An operation digest binding the approved direction and paths. Raw host,
   path, key, and command material remains process-local.

Literal IP addresses, `localhost`, `*.localhost`, `*.local`, whitespace,
slashes, and userinfo are rejected at production admission. The debug-only
`x.ai/science/ssh_scp_fixture` endpoint uses the allowlisted name
`fixture.lumen.test` and a temporary SSH config to reach its per-test loopback
server; it is not compiled as a production capability.

## Dispatch and execution

Admission and the existing Lumen permission request run through
`SessionActor`. Allow produces a ticket carrying only the operation digest.
Execution recomputes that digest and rejects post-approval substitution.

The transport invokes system `/usr/bin/scp` with batch mode, strict host-key
checking, an explicit identity file, and an explicit fixture-scoped
known-hosts file. Before starting SCP, the executor independently verifies the
known-hosts public-key blob against the approved SHA-256 fingerprint. No new
network crate or second agent/runtime is introduced.

Production callers do not receive an SSH config override and therefore cannot
use the debug fixture's loopback mapping. Real-host validation remains blocked
until a user supplies and explicitly authorizes a suitable host and test data.

## Durable audit and artifacts

Durable connector records contain target and operation correlation hashes,
terminal outcome, OpenSSH provenance, and—on success only—a redacted transfer
artifact and evidence. They do not contain hostname, paths, user, identity
file, key material, command line, stdout, or stderr.

Timeout and cancellation kill and reap the child process, end the run as
`TimedOut` or `Cancelled`, and register no artifact. A successful put or get
ends as `Succeeded`. Store and artifact roots, fixture files, and local
transfer paths must remain inside the session workspace.

On reopen, `recover_interrupted` turns any non-terminal run into
`Interrupted`; stale tickets are not resumed or retried. The legacy
`execute_offline_transport` state-machine harness remains useful for unit
coverage but is not the C3 product transport.

## Provenance

The external execution boundary and licence family are recorded in
`third_party/provenance/transport-openssh.md`. Test identities, host keys,
known-hosts, and SSH configuration are created inside each temporary test
workspace; the implementation does not read or write `~/.ssh`.
