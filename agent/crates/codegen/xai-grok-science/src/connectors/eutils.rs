//! NCBI E-utilities shared transport connector.
//!
//! Provides access to 38+ NCBI databases through the Entrez Programming
//! Utilities. A two-exchange search-and-fetch protocol: esearch returns
//! UIDs, then esummary retrieves structured records.
//!
//! Protocol: 2 exchanges (esearch → esummary).
//! Base URL: https://eutils.ncbi.nlm.nih.gov/entrez/eutils/

use super::adapter::ProtocolAdapter;
use super::fetch::{FetchExchange, ParsedResponse, RetrievedRecord};
use crate::connectors::ConnectorDescriptor;

pub struct EutilsAdapter;

impl ProtocolAdapter for EutilsAdapter {
    fn descriptor(&self) -> &'static ConnectorDescriptor {
        &super::EUTILS
    }

    fn expected_exchanges(&self) -> usize {
        2
    }

    fn build_fixture_paths(
        &self,
        query: &str,
        max_results: u32,
        fixtures: &[Vec<u8>],
    ) -> crate::Result<Vec<String>> {
        let encoded = query.replace(' ', "+");
        let esearch = format!(
            "/esearch.fcgi?db=nucleotide&retmode=json&retmax={max_results}&term={encoded}"
        );
        let fallback = "/esummary.fcgi?db=nucleotide&retmode=json&id=1".to_string();
        let esummary = if fixtures.len() >= 2 {
            serde_json::from_slice::<serde_json::Value>(&fixtures[0])
                .ok()
                .and_then(|v| {
                    v["esearchresult"]["idlist"]
                        .as_array()
                        .and_then(|ids| ids.first())
                        .and_then(|id| id.as_str())
                        .map(|id| {
                            format!("/esummary.fcgi?db=nucleotide&retmode=json&id={id}")
                        })
                })
                .unwrap_or(fallback)
        } else {
            fallback
        };
        Ok(vec![esearch, esummary])
    }

    fn parse_responses(&self, exchanges: &[FetchExchange]) -> crate::Result<ParsedResponse> {
        if exchanges.len() != 2 {
            return Err(crate::ScienceError::Invalid(format!(
                "eutils expected 2 exchanges, got {}",
                exchanges.len()
            )));
        }

        let esearch: serde_json::Value =
            serde_json::from_slice(&exchanges[0].response)?;
        let count = esearch["esearchresult"]["count"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        if count == 0 {
            return Ok(ParsedResponse {
                total_hits: 0,
                records: vec![],
            });
        }

        let esummary: serde_json::Value =
            serde_json::from_slice(&exchanges[1].response)?;

        let records: Vec<RetrievedRecord> = esummary["result"]["uids"]
            .as_array()
            .map(|uids| {
                uids.iter()
                    .filter_map(|uid| uid.as_str())
                    .map(|uid| {
                        let title = esummary["result"][uid]["title"]
                            .as_str()
                            .unwrap_or("untitled");
                        let container = esummary["result"][uid]["organism"]
                            .as_str()
                            .map(|o| format!("NCBI: {o}"))
                            .unwrap_or_else(|| "NCBI Nucleotide".to_string());
                        let url = format!(
                            "https://www.ncbi.nlm.nih.gov/nuccore/{uid}"
                        );
                        RetrievedRecord {
                            id: uid.to_string(),
                            title: title.to_string(),
                            container,
                            url,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(ParsedResponse {
            total_hits: count,
            records,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::validate_descriptor;
    use super::super::EUTILS;

    fn dummy_req() -> crate::connectors::ValidatedRequest {
        crate::connectors::ValidatedRequest {
            connector_id: "eutils",
            url: String::new(),
            timeout_ms: 1000,
            rate_limit: EUTILS.rate_limit,
            retry: EUTILS.retry,
            tos_url: EUTILS.tos_url,
            data_class: EUTILS.data_class,
            cache_policy: EUTILS.cache_policy,
        }
    }

    #[test]
    fn descriptor_is_valid() {
        validate_descriptor(&super::super::EUTILS)
            .expect("eutils descriptor invalid");
    }

    #[test]
    fn expected_exchanges() {
        let adapter = EutilsAdapter;
        assert_eq!(adapter.expected_exchanges(), 2);
    }

    #[test]
    fn build_fixture_paths_basic() {
        let adapter = EutilsAdapter;
        let paths = adapter
            .build_fixture_paths("test query", 10, &[])
            .unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].contains("esearch.fcgi"));
        assert!(paths[0].contains("test+query"));
        assert!(paths[0].contains("retmax=10"));
    }

    #[test]
    fn parse_empty_response() {
        let adapter = EutilsAdapter;
        let req = dummy_req();
        let exchanges = vec![
            FetchExchange {
                request: req.clone(),
                response: br#"{"esearchresult":{"count":"0","idlist":[]}}"#
                    .to_vec(),
            },
            FetchExchange {
                request: req,
                response: br#"{"result":{"uids":[]}}"#.to_vec(),
            },
        ];
        let parsed = adapter.parse_responses(&exchanges).unwrap();
        assert_eq!(parsed.total_hits, 0);
        assert!(parsed.records.is_empty());
    }

    #[test]
    fn parse_result_with_records() {
        let adapter = EutilsAdapter;
        let req = dummy_req();
        let exchanges = vec![
            FetchExchange {
                request: req.clone(),
                response: br#"{"esearchresult":{"count":"2","idlist":["12345","67890"]}}"#
                    .to_vec(),
            },
            FetchExchange {
                request: req,
                response: br#"{"result":{"uids":["12345","67890"],"12345":{"title":"Gene A","organism":"Homo sapiens"},"67890":{"title":"Gene B","organism":"Mus musculus"}}}"#
                    .to_vec(),
            },
        ];
        let parsed = adapter.parse_responses(&exchanges).unwrap();
        assert_eq!(parsed.total_hits, 2);
        assert_eq!(parsed.records.len(), 2);
        assert_eq!(parsed.records[0].id, "12345");
        assert_eq!(parsed.records[0].title, "Gene A");
        assert_eq!(
            parsed.records[0].container,
            "NCBI: Homo sapiens"
        );
    }

    #[test]
    fn rejects_wrong_exchange_count() {
        let adapter = EutilsAdapter;
        let req = dummy_req();
        let exchanges = vec![FetchExchange {
            request: req,
            response: b"{}".to_vec(),
        }];
        assert!(adapter.parse_responses(&exchanges).is_err());
    }
}
