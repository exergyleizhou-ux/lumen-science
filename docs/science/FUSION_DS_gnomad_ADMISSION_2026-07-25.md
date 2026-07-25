# Connector Admission: gnomad

**Date:** 2026-07-25
**Status:** approved
**Disposition:** implemented

## Source
- connector_id: gnomad
- display_name: gnomad
- auth_class: none
- data_class: public_reference

## Implementation
- Descriptor: agent/crates/codegen/xai-grok-science/src/connectors.rs
- Adapter: agent/crates/codegen/xai-grok-science/src/connectors/gnomad.rs
- Fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_gnomad_*.json
- Provenance: third_party/provenance/connector-gnomad.md

## Verification
- Descriptor validation: PASS
- Exact HTTPS host: enforced
- Offline fixture product proof: L4
- Live probe: NOT RUN (requires user authorization)
