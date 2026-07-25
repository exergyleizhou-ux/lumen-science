# Provenance: connector-crossref

upstream_project: Crossref REST API
source_url: https://www.crossref.org/documentation/retrieve-metadata/rest-api/
api_base: https://api.crossref.org
reviewed_at: 2026-07-24
source_code_imported: false
runtime_authority: rust-lumen-tui-only
authentication: public pool requires no sign-up; an explicitly authorized live probe requires runtime-only CROSSREF_MAILTO
rate_limit: bounded list search at 1 request per second and one exchange per run
selected_fields: DOI,title,container-title
excluded_fields: abstract,link,full-text
metadata_rights: bibliographic facts generally not subject to copyright; Crossref-generated data CC0
notice_attribution_requirements: abstracts can retain publisher or author copyright and are intentionally not retrieved
offline_fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_crossref_works.json
live_probe: ignored by default; requires explicit network authorization and CROSSREF_MAILTO
