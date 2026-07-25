# Connector Admission: gtopdb

**Date:** 2026-07-25
**Status:** approved
**Disposition:** implemented

## Source
- connector_id: gtopdb
- display_name: gtopdb
- auth_class: none
- data_class: public_reference

## Implementation
- Descriptor: agent/crates/codegen/xai-grok-science/src/connectors.rs
- Adapter: agent/crates/codegen/xai-grok-science/src/connectors/gtopdb.rs
- Fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_gtopdb_*.json
- Provenance: third_party/provenance/connector-gtopdb.md

## Verification
- Descriptor validation: PASS
- Exact HTTPS host: enforced
- Offline fixture product proof: L4
- Live probe: NOT RUN (requires user authorization)
