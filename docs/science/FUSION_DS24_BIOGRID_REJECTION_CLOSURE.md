# Connector DS-24: BioGRID — Final Rejection

**Date:** 2026-07-25
**Disposition:** `rejected-unsafe-or-duplicate`
**Authority:** Rust Lumen SessionActor (sole permission and credential boundary)

## Source

```yaml
service: BioGRID REST API
service_owner: BioGRID / Tyers Lab (Université de Montréal)
api_base: https://webservice.thebiogrid.org/
api_doc: https://wiki.thebiogrid.org/doku.php/webservice
```

## Rejection Reason

BioGRID's official WADL defines only GET operations, and the mandatory
`accessKey` parameter is passed as a **URL query string parameter**.

This violates Lumen's hard safety invariant:

> `credential-never-in-URL`

Credentials in URLs are logged, cached, persisted in browser history,
proxied through intermediate servers, and potentially captured in artifact
and evidence records. Lumen must never emit or store a credential in any
URL, query string, or path component, even for read-only public services.

No workaround exists without modifying the BioGRID service itself.

## Alternative Path

BioGRID data can be obtained through alternative connectors already admitted:
- **IntAct** (DS-25) — molecular interactions; distinct access model (no URL credential)
- **NCBI Gene** (DS-18) — gene-level summaries from NCBI's public E-utilities

Users needing BioGRID-specific interaction data must retrieve it directly
from the BioGRID web interface, outside Lumen's automated pipeline.

## Evidence

```yaml
admission_status: rejected-safety-policy
final_disposition: rejected-unsafe-or-duplicate
evidence_path: ../../third_party/provenance/connector-biogrid.md
verified_at: 2026-07-25
```

## Registry Status

The `BIOGRID_REJECTED` descriptor exists in `connectors.rs` for documentation
and audit purposes but has **no runtime adapter** and is **never dispatched**.
Any request to `connector_id = "biogrid"` must fail closed with
`PolicyError::CredentialInURL`.
