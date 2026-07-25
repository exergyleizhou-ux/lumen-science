# Connector Admission: string-db

**Date:** 2026-07-25
**Status:** approved
**Disposition:** implemented

## Source
- connector_id: string-db
- display_name: string db
- auth_class: none
- data_class: public_reference

## Implementation
- Descriptor: agent/crates/codegen/xai-grok-science/src/connectors.rs
- Adapter: agent/crates/codegen/xai-grok-science/src/connectors/string-db.rs
- Fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_string-db_*.json
- Provenance: third_party/provenance/connector-string-db.md

## Verification
- Descriptor validation: PASS
- Exact HTTPS host: enforced
- Offline fixture product proof: L4
- Live probe: NOT RUN (requires user authorization)
