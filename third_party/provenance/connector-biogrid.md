# Provenance: connector-biogrid

```yaml
upstream_project: BioGRID REST API (Tyers Lab, Université de Montréal)
repo_url: https://wiki.thebiogrid.org/doku.php/webservice
pinned_commit: n/a (public web API)
source_path: n/a (no code copied; descriptor only)
source_file_sha256: n/a
license_at_source: BioGRID data available for academic use; see terms at https://thebiogrid.org/terms.php
notice_attribution_requirements: n/a (rejected — no runtime adapter)
key_dependencies_and_licenses: none
reuse_mode: rejected
lumen_target_path: agent/crates/codegen/xai-grok-science/src/connectors.rs (BIOGRID_REJECTED descriptor only; no adapter)
modifications_made: n/a
verification_evidence: descriptor exists but is never dispatched; any request fails closed with PolicyError
owner: lumen-science
admission_status: rejected-safety-policy
final_disposition: rejected-unsafe-or-duplicate
rejection_reason: mandatory accessKey credential in URL query string violates Lumen credential-never-in-URL safety invariant
tos_url: https://thebiogrid.org/terms.php
```
