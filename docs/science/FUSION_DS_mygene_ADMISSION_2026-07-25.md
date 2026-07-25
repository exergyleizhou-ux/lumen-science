# Connector Admission: mygene

**Date:** 2026-07-25
**Status:** approved
**Disposition:** implemented

## Source
- connector_id: mygene
- display_name: mygene
- auth_class: none
- data_class: public_reference

## Implementation
- Descriptor: agent/crates/codegen/xai-grok-science/src/connectors.rs
- Adapter: agent/crates/codegen/xai-grok-science/src/connectors/mygene.rs
- Fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_mygene_*.json
- Provenance: third_party/provenance/connector-mygene.md

## Verification
- Descriptor validation: PASS
- Exact HTTPS host: enforced
- Offline fixture product proof: L4
- Live probe: NOT RUN (requires user authorization)
