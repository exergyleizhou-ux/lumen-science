# Provenance: connector-europepmc

upstream_project: Europe PMC Articles REST API
source_url: https://europepmc.org/RestfulWebService
api_base: https://www.ebi.ac.uk/europepmc/webservices/rest
reviewed_at: 2026-07-24
source_code_imported: false
runtime_authority: rust-lumen-tui-only
authentication: public query; no credential
local_rate_limit: conservative one request per second and one exchange per run; not represented as an official Europe PMC quota
result_type: lite JSON, maximum 50 records, synonym expansion disabled
selected_fields: source,id,title,journalTitle,pubYear
excluded_fields: abstractText,full text,references,annotations,external links
copyright_boundary: bibliographic metadata only; article content remains subject to each work's copyright and license
offline_fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_europepmc_search.json
live_probe: ignored by default; requires explicit network authorization
