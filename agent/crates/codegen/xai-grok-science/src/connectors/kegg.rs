//! KEGG — LICENSE PENDING. Returns empty results until license accepted.
use super::adapter::ProtocolAdapter; use super::fetch::ParsedResponse;
pub struct KeggAdapter;
impl ProtocolAdapter for KeggAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::KEGG_PENDING } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, _q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec!["/find/pathway/test".into()]) } fn parse_responses(&self, _e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { Ok(ParsedResponse { total_hits: 0, records: vec![] }) } }
