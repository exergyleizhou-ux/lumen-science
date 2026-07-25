# Lumen Science DS-1R — ProtocolAdapter coverage validation fix

**Milestone:** DS-1R (rework)
**Date:** 2026-07-24
**Branch:** codex/science-fusion-full

## Issue

Original DS-1 `coverage_rejects_orphan_adapter` test was a duplicate of `coverage_rejects_missing_adapter` — both only registered a subset of adapters, testing only the "missing adapter" path. The "orphan adapter" code path (adapter registered without matching descriptor) was untested.

## Fix

- Created `ORPHAN_DESCRIPTOR` with id `"orphan-test-only"` — an ID that deliberately does NOT exist in `connectors::registry()`
- Created `OrphanAdapter` struct implementing `ProtocolAdapter`
- Test registers all 7 real adapters plus `OrphanAdapter`, validates against real descriptor registry
- Assertion verifies error contains `"orphan-test-only"` and `"has no matching descriptor"`

## Tests

- coverage_rejects_missing_adapter: registers only pubmed, validates against all 7 descriptors → fail with descriptor name
- coverage_rejects_orphan_adapter: registers all 7 + OrphanAdapter, validates against 7 descriptors → fail with orphan ID
- adapter_descriptor_coverage_is_one_to_one: dynamic read from connectors::registry(), no hardcoded vec
- registry_rejects_duplicate_id: confirmed
- registry_unknown_id_returns_none: confirmed

## Negative tests

- Orphan adapter with descriptor ID not in registry → fail closed
- Missing adapter (fewer adapters than descriptors) → fail closed with specific ID
- Order mismatch → fail closed
- Duplicate ID registration → fail closed
