# OpenSSH transport provenance

## Capability

The S3 SSH/SCP transport invokes the operating system's `/usr/bin/scp` binary;
no OpenSSH source code, library, protocol implementation, or credentials are
copied into Lumen.

## Invocation boundary

The implementation uses non-interactive batch mode, explicit identity and
fixture-scoped `UserKnownHostsFile`, strict host-key checking, a bounded
connect timeout, and a numeric port.  It captures no stdout or stderr in the
durable Science store.  Durable records retain only an irreversible target
SHA-256, outcome, timestamps, and verified artifact hash.

## Local verification

On 2026-07-23 this machine reported:

```text
OpenSSH_10.2p1, LibreSSL 3.3.6
```

## License

OpenSSH is a system binary distributed under the BSD license family.  This
integration performs subprocess invocation only and adds no Rust crate
dependency.
