# Provenance: connector-openalex

upstream_project: OpenAlex REST API
source_url: https://developers.openalex.org/
api_base: https://api.openalex.org
reviewed_at: 2026-07-24
source_code_imported: false
runtime_authority: rust-lumen-tui-only
authentication: runtime-only OPENALEX_API_KEY required for live requests; never persisted
service_model: metered freemium API; OpenAlex data remains CC0
official_rate_limit: 100 requests per second before HTTP 429
local_rate_limit: conservative one request per second and one exchange per run
result_type: works search JSON, maximum 50 records
selected_fields: id,doi,display_name,publication_year
excluded_fields: abstract_inverted_index,full text,authorships,locations,references,topics,content_url
copyright_boundary: selected OpenAlex metadata is CC0; underlying article content remains subject to article-level rights
offline_fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_openalex_search.json
live_probe: ignored by default; requires explicit billable-network authorization and runtime key
