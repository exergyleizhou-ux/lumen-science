# Provenance: connector-pubmed

```yaml
upstream_project: NCBI E-utilities (Entrez Programming Utilities)
repo_url: https://www.ncbi.nlm.nih.gov/books/NBK25501/
pinned_commit: n/a (public web API; protocol pinned by documentation retrieval 2026-07-23)
source_path: n/a (no code copied; descriptor + protocol only)
source_file_sha256: n/a
license_at_source: US Government public data; API usage governed by NCBI policies
notice_attribution_requirements: Cite NCBI/PubMed as data source in derived records
key_dependencies_and_licenses: none (no vendored code)
reuse_mode: protocol/workflow adaptation
lumen_target_path: agent/crates/codegen/xai-grok-science/src/connectors.rs (descriptor), src/connectors/pubmed.rs (adapter), src/connectors/fetch.rs (run protocol)
modifications_made: n/a
verification_evidence: descriptor + adapter + fetch unit tests (connectors::tests, 55 passed); product-path e2e test_stdio_science_connector_fetch_product_path (L4); live probe evidence outputs/evidence/pubmed_live_probe.log (L5, see that file for query/hits/first-record/retrieved_at)
owner: lumen-science
tos_url: https://www.ncbi.nlm.nih.gov/home/about/policies/
rate_policy: 3 requests/second without API key (descriptor-enforced)
```
