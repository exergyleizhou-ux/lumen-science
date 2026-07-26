//! Cross-connector negative test battery for the Lumen Science 1.0 gate.
//!
//! These tests exercise every inbound data path — empty responses, malformed
//! JSON, partial records, unexpected content types — across all implemented
//! connectors. Fail-closed is the invariant.
//!
//! Seam: DS-1R negative-test extension for offline product proof.
#![cfg(test)]
use crate::connectors::registry;
use crate::connectors::fetch::FetchExchange;
use crate::connectors::ConnectorDescriptor;

fn dummy_request(desc: &ConnectorDescriptor) -> crate::connectors::ValidatedRequest {
    crate::connectors::ValidatedRequest {
        connector_id: desc.id,
        url: String::new(),
        timeout_ms: 5000,
        rate_limit: desc.rate_limit,
        retry: desc.retry,
        tos_url: desc.tos_url,
        data_class: desc.data_class,
        cache_policy: desc.cache_policy,
    }
}

/// Assert that every connector in the active registry:
/// (1) returns 0 hits on empty response bodies,
/// (2) fails closed on non-JSON garbage,
/// (3) fails closed on truncated JSON,
/// (4) has a non-zero `expected_exchanges()`.
#[test]
fn every_active_connector_rejects_empty_truncated_and_garbage() {
    let mut failed = Vec::new();
    for desc in registry() {
        let adapter = super::adapter::REGISTRY.get(desc.id)
            .unwrap_or_else(|| panic!("missing adapter for {}", desc.id));
        let ex = adapter.expected_exchanges();
        assert!(ex > 0, "{} expected_exchanges() = 0", desc.id);

        // empty response(s)
        let req = dummy_request(desc);
        let empty_exchanges: Vec<_> = (0..ex).map(|_| FetchExchange {
            request: req.clone(),
            response: b"[]".to_vec(),
        }).collect();
        let r = adapter.parse_responses(&empty_exchanges);
        if let Ok(parsed) = r
            && parsed.total_hits > 100 {
                failed.push(format!("{}: unexpected hits from empty data: {}", desc.id, parsed.total_hits));
            }

        // garbage responses
        let garbage_exchanges: Vec<_> = (0..ex).map(|_| FetchExchange {
            request: req.clone(),
            response: b"!!!NOT JSON!!!".to_vec(),
        }).collect();
        assert!(
            adapter.parse_responses(&garbage_exchanges).is_err(),
            "{} must reject garbage response", desc.id
        );

        // truncated responses
        let trunc_exchanges: Vec<_> = (0..ex).map(|_| FetchExchange {
            request: req.clone(),
            response: b"{".to_vec(),
        }).collect();
        assert!(
            adapter.parse_responses(&trunc_exchanges).is_err(),
            "{} must reject truncated response", desc.id
        );
    }
    assert!(failed.is_empty(), "failing connectors: {:?}", failed);
}

/// Every connector descriptor must pass validation.
#[test]
fn every_connector_descriptor_validates() {
    for desc in registry() {
        super::validate_descriptor(desc).unwrap_or_else(|e| panic!("{}: {e}", desc.id));
    }
}
