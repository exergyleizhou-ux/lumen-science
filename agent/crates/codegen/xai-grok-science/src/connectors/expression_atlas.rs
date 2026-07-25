//! Expression Atlas (EBI). Seam: S3.
use super::adapter::ProtocolAdapter;
use super::fetch::{FetchExchange, ParsedResponse, RetrievedRecord};
use crate::ScienceError;

pub fn search_path(query: &str) -> String {
    format!(
        "/gxa/json/experiments?speciesQuery={}&keyword={}&rows=25",
        super::url_encode(query),
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
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| {
        ScienceError::Invalid(format!("expression-atlas: malformed JSON: {e}"))
    })?;
    let experiments = v
        .get("experiments")
        .and_then(|e| e.as_array())
        .or_else(|| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut recs = Vec::new();
    for ex in &experiments {
        let id = ex
            .get("experimentAccession")
            .or_else(|| ex.get("accession"))
            .and_then(|s| s.as_str())
            .filter(|s| !s.is_empty());
        let Some(id) = id else {
            continue;
        };
        let title = ex
            .get("experimentDescription")
            .or_else(|| ex.get("description"))
            .and_then(|s| s.as_str())
            .unwrap_or(id);
        recs.push(RetrievedRecord {
            id: id.to_owned(),
            title: title.to_owned(),
            container: "Expression Atlas".to_owned(),
            url: format!("https://www.ebi.ac.uk/gxa/experiments/{id}"),
        });
    }
    Ok(ParsedResponse {
        total_hits: recs.len() as u64,
        records: recs,
    })
}

pub struct ExpressionAtlasAdapter;
impl ProtocolAdapter for ExpressionAtlasAdapter {
    fn descriptor(&self) -> &'static super::ConnectorDescriptor {
        &super::EXPRESSION_ATLAS
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
                "expression-atlas: expected 1 exchange, got {}",
                e.len()
            )));
        }
        parse_search(&e[0].response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &[u8] = br#"{"experiments":[{"experimentAccession":"E-MTAB-1234","experimentDescription":"demo"}]}"#;

    #[test]
    fn ok_parse() {
        let p = parse_search(OK).unwrap();
        assert_eq!(p.total_hits, 1);
        assert_eq!(p.records[0].id, "E-MTAB-1234");
    }

    #[test]
    fn empty_ok() {
        assert_eq!(parse_search(b"").unwrap().total_hits, 0);
    }

    #[test]
    fn garbage_fails() {
        assert!(parse_search(b"<html>").is_err());
    }

    #[test]
    fn wrong_exchange_count() {
        assert!(ExpressionAtlasAdapter.parse_responses(&[]).is_err());
    }
}
