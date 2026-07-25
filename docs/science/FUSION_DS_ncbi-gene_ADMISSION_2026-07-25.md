# Connector Admission: ncbi-gene

**Date:** 2026-07-25
**Status:** approved
**Disposition:** implemented

## Source
- connector_id: ncbi-gene
- display_name: ncbi gene
- auth_class: none
- data_class: public_reference

## Implementation
- Descriptor: agent/crates/codegen/xai-grok-science/src/connectors.rs
- Adapter: agent/crates/codegen/xai-grok-science/src/connectors/ncbi-gene.rs
- Fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_ncbi-gene_*.json
- Provenance: third_party/provenance/connector-ncbi-gene.md

## Verification
- Descriptor validation: PASS
- Exact HTTPS host: enforced
- Offline fixture product proof: L4
- Live probe: NOT RUN (requires user authorization)
