# Lumen Science provider boundary

Seam contracts: **S4, S5**. Status: provider reuse policy only; this document
does not make a network request, read an API key, or introduce a new provider
runtime.

## Single runtime rule

Science workflows use the existing Rust Lumen model catalog, sampler, session,
credential, telemetry, and permission chain. They must not create a Science
HTTP client, provider registry, credential environment, retry loop, or model
selection authority. The active provider and model are immutable fields of the
existing `RunContext` and are recorded as provenance by the Science kernel.

The current catalog already contains Chinese-provider presets, including the
formal DeepSeek V4 identifiers and Qwen entries. Catalog presence, a mock, or
a unit test is not evidence of a live provider capability.

## L5 authorization and evidence

A real provider check is permitted only after the user explicitly authorizes a
specific provider/model and endpoint. The check must use the existing Lumen
chain, with credentials supplied through its normal secret mechanism—not a
test fixture, source file, Science store, or durable event. It must retain:

1. run ID and selected model/provider identity;
2. redacted endpoint identity and request timestamp;
3. the exact existing Lumen permission/dispatch path used;
4. response-derived artifact/evidence/provenance hashes, with no credential or
   raw provider payload persisted; and
5. restart/replay evidence that never retries a paid or side-effecting request.

Any missing credential, unknown endpoint, unexpected model substitution,
authorization denial, timeout, or provider error is fail-closed and cannot be
reported as a provider success.

## Explicit exclusions

- Do not copy a provider API key into configuration, tests, logs, prompts, or
  Science events.
- Do not silently fall back from the selected model to another provider.
- Do not validate a production claim with a catalog-only assertion or mock.
- Do not let Expert/Reviewer advice declare a Science run complete without the
  same HostVerification and durable evidence required of any Lumen run.
