//! Human Protein Atlas. Seam: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{FetchExchange, ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn search_path(query: &str) -> String {
    format!(
        "/api/search_download.php?search={}&format=json&columns=g,gs,eg,up,gd,chr&compress=no",
        super::url_encode(query)
    )
}

pub fn parse_search(bytes: &[u8]) -> crate::Result<ParsedResponse> {
    if bytes.is_empty() {
        return Ok(ParsedResponse {
            total_hits: 0,
            records: vec![],
        });
    }
    let v: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| ScienceError::Invalid(format!("hpa: malformed JSON: {e}")))?;
    let arr = v
        .as_array()
        .ok_or_else(|| ScienceError::Invalid("hpa: not an array".into()))?;
    let mut recs = Vec::with_capacity(arr.len());
    for r in arr {
        let gene = r.get("Gene").and_then(|g| g.as_str()).unwrap_or("");
        if gene.is_empty() {
            continue;
        }
        let ensembl = r.get("Ensembl").and_then(|e| e.as_str()).unwrap_or("");
        recs.push(RetrievedRecord {
            id: ensembl.to_owned(),
            title: gene.to_owned(),
            container: "HPA".to_owned(),
            url: format!("https://www.proteinatlas.org/{ensembl}"),
        });
    }
    Ok(ParsedResponse {
        total_hits: recs.len() as u64,
        records: recs,
    })
}

pub struct HpaAdapter;
impl ProtocolAdapter for HpaAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::HPA
    }
    fn expected_exchanges(&self) -> usize {
        1
    }
    fn build_fixture_paths(
        &self,
        q: &str,
        _m: u32,
        _f: &[Vec<u8>],
    ) -> crate::Result<Vec<String>> {
        Ok(vec![search_path(q)])
    }
    fn parse_responses(&self, e: &[FetchExchange]) -> crate::Result<ParsedResponse> {
        if e.len() != 1 {
            return Err(ScienceError::Invalid(format!(
                "hpa: expected 1 exchange, got {}",
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
        br#"[{"Gene":"BRCA2","Ensembl":"ENSG00000139618","Gene synonym":"FANCD1"}]"#;

    #[test]
    fn ok_parse() {
        let p = parse_search(OK).unwrap();
        assert_eq!(p.total_hits, 1);
        assert_eq!(p.records[0].title, "BRCA2");
    }

    #[test]
    fn empty_ok() {
        assert_eq!(parse_search(b"").unwrap().total_hits, 0);
    }

    #[test]
    fn garbage_fails() {
        assert!(parse_search(b"???") .is_err());
    }

    #[test]
    fn not_array_fails() {
        assert!(parse_search(br#"{"Gene":"X"}"#).is_err());
    }

    #[test]
    fn wrong_exchange_count() {
        assert!(HpaAdapter.parse_responses(&[]).is_err());
    }
}
