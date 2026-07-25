# Provenance: connector-uniprot

upstream_project: UniProtKB website REST API
source_url: https://www.uniprot.org/help/api
api_base: https://rest.uniprot.org/uniprotkb
reviewed_at: 2026-07-24
source_code_imported: false
runtime_authority: rust-lumen-tui-only
authentication: public query; no credential
local_rate_limit: conservative one request per second and one exchange per run
selected_fields: accession,id,protein_name,gene_names,organism_name
excluded_fields: sequence,features,references,citation text
license_at_source: CC-BY-4.0 for copyrightable database content
notice_attribution_requirements: attribute UniProt; retain its no-warranty and third-party-rights warning
offline_fixture: agent/crates/codegen/xai-grok-science/fixtures/connector_uniprot_search.json
live_probe: ignored by default; requires explicit network authorization
