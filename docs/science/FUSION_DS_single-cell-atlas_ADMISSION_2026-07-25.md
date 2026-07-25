# Connector Admission: single-cell-atlas

**Date:** 2026-07-25
**Status:** approved
**Disposition:** implemented

## Source
- connector_id: single-cell-atlas
- display_name: single cell atlas
- auth_class: none
- data_class: public_reference

## Implementation
- Descriptor: agent/crates/codegen/xai-grok-science/src/connectors.rs
- Adapter: agent/crates/codegen/xai-grok-science/src/connectors/single-cell-atlas.rs
- Fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_single-cell-atlas_*.json
- Provenance: third_party/provenance/connector-single-cell-atlas.md

## Verification
- Descriptor validation: PASS
- Exact HTTPS host: enforced
- Offline fixture product proof: L4
- Live probe: NOT RUN (requires user authorization)
