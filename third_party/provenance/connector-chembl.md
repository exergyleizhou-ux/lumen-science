# Provenance: connector-chembl

```yaml
upstream_project: ChEMBL REST API (European Bioinformatics Institute)
repo_url: https://www.ebi.ac.uk/chembl/api/data/docs
pinned_commit: n/a (public web API; protocol pinned by documentation retrieval 2026-07-23)
source_path: n/a (no code copied; descriptor + protocol only)
source_file_sha256: n/a
license_at_source: ChEMBL data CC-BY-SA 3.0; API under EBI terms of use
notice_attribution_requirements: Attribute ChEMBL/EBI; CC-BY-SA share-alike applies to redistributed data
key_dependencies_and_licenses: none (no vendored code)
reuse_mode: protocol/workflow adaptation
lumen_target_path: agent/crates/codegen/xai-grok-science/src/connectors.rs (descriptor), src/connectors/chembl.rs (adapter), src/connectors/fetch.rs (run protocol)
modifications_made: n/a
verification_evidence: descriptor + adapter + fetch unit tests (connectors::tests); product-path e2e test_stdio_science_connector_fetch_product_path (L4); explicit ignored live_probe_chembl_real_search produces CHEMBL_LIVE_EVIDENCE from the public API (L5)
owner: lumen-science
tos_url: https://www.ebi.ac.uk/about/terms-of-use
rate_policy: conservative 5 requests/second (descriptor-enforced)
```
