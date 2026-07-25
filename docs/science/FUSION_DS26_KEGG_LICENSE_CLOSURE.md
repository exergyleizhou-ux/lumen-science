# Connector DS-26: KEGG — License Closure

**Date:** 2026-07-25
**Disposition:** `rejected-license-or-terms`
**Authority:** Rust Lumen SessionActor

## Source

```yaml
service: KEGG REST API
service_owner: Kanehisa Laboratories (Kyoto University)
api_base: https://rest.kegg.jp/
license_url: https://www.kegg.jp/kegg/legal.html
```

## License Assessment

KEGG's terms explicitly state:

> "Academic users may use KEGG free of charge. Commercial use requires a
> paid subscription through Pathway Solutions Inc."

Lumen Science 1.0 does not have a commercial KEGG subscription. Academic
use alone does not satisfy our distribution requirements — any release
binary must be safe for all users regardless of use case.

Furthermore, KEGG's database content includes pathway diagrams, compound
structures, and disease associations whose copyright status varies by
entry. A blanket "academic use only" license is insufficient for a
production release where we cannot enforce how users classify themselves.

## Decision

**Rejected until a formal commercial license is obtained** or KEGG changes
its terms to permit unrestricted redistribution.

The `KEGG_PENDING` descriptor exists in `connectors.rs` for documentation
purposes but has **no runtime adapter** and is **never dispatched**.

## Evidence

```yaml
admission_status: rejected-license-or-terms
final_disposition: rejected-license-or-terms
evidence_path: ../../third_party/provenance/connector-kegg.md
verified_at: 2026-07-25
```

## Re-evaluation Trigger

- KEGG announces a CC0 or CC-BY-4.0 license for its database content, OR
- Lumen Science obtains a commercial subscription covering all users.

Until then: `rejected-license-or-terms`. Do not keep as `pending`.
