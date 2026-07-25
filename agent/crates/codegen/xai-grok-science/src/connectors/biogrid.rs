//! BioGRID — REJECTED: credential in URL. Returns empty results.
use super::adapter::ProtocolAdapter; use super::fetch::ParsedResponse;
pub struct BiogridAdapter;
impl ProtocolAdapter for BiogridAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::BIOGRID_REJECTED } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, _q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec!["/interactions/?searchNames=true&geneList=test&format=json&max=1".into()]) } fn parse_responses(&self, _e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { Ok(ParsedResponse { total_hits: 0, records: vec![] }) } }
