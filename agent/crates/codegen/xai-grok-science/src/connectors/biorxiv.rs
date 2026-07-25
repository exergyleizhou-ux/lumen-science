//! bioRxiv / medRxiv preprints API. Seam contract: S3.
//! Simple DOI lookup or recent-pool fetch. Abstracts excluded.

use super::adapter::ProtocolAdapter;
use super::fetch::{ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn doi_path(doi: &str, server: &str) -> String {
    format!("/details/{server}/{doi}")
}

fn parse_details(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    let collection = value.get("collection").and_then(|c| c.as_array())
        .ok_or_else(|| ScienceError::Invalid("biorxiv: missing collection".into()))?;
    let mut records = Vec::with_capacity(collection.len());
    for paper in collection {
        let doi = paper.get("doi").and_then(|d| d.as_str()).filter(|d| !d.is_empty())
            .ok_or_else(|| ScienceError::Invalid("biorxiv: paper without doi".into()))?;
        let title = paper.get("title").and_then(|t| t.as_str()).filter(|t| !t.trim().is_empty())
            .unwrap_or("(untitled preprint)");
        let container = paper.get("server").and_then(|s| s.as_str()).unwrap_or("bioRxiv");
        records.push(RetrievedRecord {
            id: doi.to_owned(), title: title.to_owned(), container: container.to_owned(),
            url: format!("https://doi.org/{doi}"),
        });
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct BiorxivAdapter;
impl ProtocolAdapter for BiorxivAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::BIORXIV }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, query: &str, _max: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> {
        Ok(vec![doi_path(query, "biorxiv")])
    }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 { return Err(ScienceError::Invalid("biorxiv: requires 1 exchange".into())); }
        parse_details(&e[0].response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const DETAILS: &[u8] = br#"{"collection":[{"doi":"10.1101/2024.01.01.123456","title":"A preprint study","server":"biorxiv","abstract":"not consumed","authors":"Smith et al."}]}"#;
    #[test] fn parse_reads_fields() { let p = parse_details(DETAILS).unwrap(); assert_eq!(p.total_hits,1); assert_eq!(p.records[0].id,"10.1101/2024.01.01.123456"); }
    #[test] fn malformed_fails() { assert!(parse_details(b"{}").is_err()); assert!(parse_details(br#"{"collection":[{"title":"x"}]}"#).is_err()); }
}
