# Connector Admission: pubchem

**Date:** 2026-07-25
**Status:** approved
**Disposition:** implemented

## Source
- connector_id: pubchem
- display_name: pubchem
- auth_class: none
- data_class: public_reference

## Implementation
- Descriptor: agent/crates/codegen/xai-grok-science/src/connectors.rs
- Adapter: agent/crates/codegen/xai-grok-science/src/connectors/pubchem.rs
- Fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_pubchem_*.json
- Provenance: third_party/provenance/connector-pubchem.md

## Verification
- Descriptor validation: PASS
- Exact HTTPS host: enforced
- Offline fixture product proof: L4
- Live probe: NOT RUN (requires user authorization)
