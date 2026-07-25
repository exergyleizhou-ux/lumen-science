//! UCSC Genome Browser API. Seam: S3.
use super::adapter::ProtocolAdapter; use super::fetch::{ParsedResponse, RetrievedRecord}; use crate::ScienceError;
pub fn search_path(term: &str, genome: &str) -> String { format!("/search?search={}&genome={genome}", super::url_encode(term)) }

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    let v: serde_json::Value = serde_json::from_slice(bytes)?;
    let position_matches = v.get("positionMatches").and_then(|m| m.as_array());
    let mut records = Vec::new();
    if let Some(pms) = position_matches {
        for pm in pms {
            let name = pm.get("trackName").or(pm.get("name")).and_then(|n| n.as_str()).unwrap_or("");
            let sub_matches = pm.get("matches").and_then(|m| m.as_array());
            if let Some(subs) = sub_matches {
                for m in subs {
                    let pos = m.get("position").and_then(|p| p.as_str()).unwrap_or("").to_string();
                    let desc = m.get("description").and_then(|d| d.as_str()).unwrap_or("").to_string();
                    let title = format!("{name} {desc}").trim().to_owned();
                    records.push(RetrievedRecord { id: format!("ucsc_{}", records.len()), title, container: pos, url: "https://genome.ucsc.edu".to_owned() });
                }
            }
        }
    }
    Ok(ParsedResponse { total_hits: records.len() as u64, records })
}

pub struct UcscAdapter;
impl ProtocolAdapter for UcscAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::UCSC }
    fn expected_exchanges(&self) -> usize { 1 }
    fn build_fixture_paths(&self, q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec![search_path(q, "hg38")]) }
    fn parse_responses(&self, e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { if e.len() != 1 { return Err(ScienceError::Invalid("ucsc: 1 exchange".into())); } parse_search(&e[0].response) }
}

#[cfg(test)] mod tests { use super::*;
    const F: &[u8] = br#"{"positionMatches":[{"trackName":"RefSeq","matches":[{"position":"chr7:43044295-43125483","description":"BRCA2"}]}]}"#;
    #[test] fn ok() { let p = parse_search(F).unwrap(); assert_eq!(p.total_hits,1); assert!(p.records[0].title.contains("BRCA2")); }
}
