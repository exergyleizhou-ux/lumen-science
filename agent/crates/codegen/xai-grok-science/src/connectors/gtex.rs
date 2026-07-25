//! GTEx Portal API v2. Seam: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{FetchExchange, ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn gene_path(gene_id: &str, max: u32) -> String {
    format!(
        "/api/v2/reference/gene?geneId={}&itemsPerPage={}",
        super::url_encode(gene_id),
        max.clamp(1, 25)
    )
}

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    if bytes.is_empty() {
        return Ok(ParsedResponse {
            total_hits: 0,
            records: vec![],
        });
    }
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
        ScienceError::Invalid(format!("gtex: malformed JSON: {e}"))
    })?;
    let genes: Vec<serde_json::Value> = v
        .get("gene")
        .and_then(|g| g.as_array())
        .cloned()
        .or_else(|| v.as_array().cloned())
        .unwrap_or_default();
    let mut recs = Vec::with_capacity(genes.len());
    for g in &genes {
        let id = g
            .get("gencodeId")
            .and_then(|s| s.as_str())
            .or_else(|| g.get("geneSymbol").and_then(|s| s.as_str()))
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ScienceError::Invalid("gtex: missing id".into()))?;
        let name = g
            .get("geneSymbol")
            .and_then(|s| s.as_str())
            .unwrap_or(id);
        recs.push(RetrievedRecord {
            id: id.to_owned(),
            title: name.to_owned(),
            container: "GTEx".to_owned(),
            url: format!("https://gtexportal.org/home/gene/{id}"),
        });
    }
    Ok(ParsedResponse {
        total_hits: recs.len() as u64,
        records: recs,
    })
}

pub struct GtexAdapter;
impl ProtocolAdapter for GtexAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::GTEX
    }
    fn expected_exchanges(&self) -> usize {
        1
    }
    fn build_fixture_paths(
        &self,
        q: &str,
        m: u32,
        _f: &[Vec<u8>],
    ) -> crate::Result<Vec<String>> {
        Ok(vec![gene_path(q, m)])
    }
    fn parse_responses(&self, e: &[FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 {
            return Err(ScienceError::Invalid(format!(
                "gtex: expected 1 exchange, got {}",
                e.len()
            )));
        }
        parse_search(&e[0].response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &[u8] =
        br#"{"gene":[{"gencodeId":"ENSG00000139618.15","geneSymbol":"BRCA2"}]}"#;

    #[test]
    fn ok_parse() {
        let p = parse_search(OK).unwrap();
        assert_eq!(p.total_hits, 1);
        assert_eq!(p.records[0].id, "ENSG00000139618.15");
    }

    #[test]
    fn empty_ok() {
        let p = parse_search(b"").unwrap();
        assert_eq!(p.total_hits, 0);
    }

    #[test]
    fn garbage_fails() {
        assert!(parse_search(b"not-json").is_err());
    }

    #[test]
    fn truncated_fails() {
        assert!(parse_search(b"{").is_err());
    }

    #[test]
    fn partial_missing_id_fails() {
        assert!(parse_search(br#"{"gene":[{"geneSymbol":""}]}"#).is_err());
    }

    #[test]
    fn wrong_exchange_count() {
        let a = GtexAdapter;
        assert!(a.parse_responses(&[]).is_err());
    }
}
