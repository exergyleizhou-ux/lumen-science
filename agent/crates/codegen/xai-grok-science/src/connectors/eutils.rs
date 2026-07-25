//! NCBI E-utilities shared transport. Returns empty results — protocol partially overlaps with PubMed.
use super::adapter::ProtocolAdapter; use super::fetch::ParsedResponse;
pub struct EutilsAdapter;
impl ProtocolAdapter for EutilsAdapter { fn descriptor(&self) -> &'static super::ConnectorDescriptor { &super::EUTILS } fn expected_exchanges(&self) -> usize { 1 } fn build_fixture_paths(&self, _q: &str, _m: u32, _f: &[Vec<u8>]) -> crate::Result<Vec<String>> { Ok(vec!["/esearch.fcgi?db=nucleotide&retmode=json&retmax=1&term=test".into()]) } fn parse_responses(&self, _e: &[super::fetch::FetchExchange]) -> crate::Result<ParsedResponse> { Ok(ParsedResponse { total_hits: 0, records: vec![] }) } }
