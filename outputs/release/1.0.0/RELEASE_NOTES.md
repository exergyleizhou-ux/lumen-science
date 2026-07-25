# Lumen Science 1.0.0

Local-first, auditable scientific workbench.

## Highlights
- Offline sequence analysis (`seq analyze`, `pipeline offline`) with SHA-256 artifacts
- Multi-platform CLI + MCP builds (darwin/linux arm64+amd64, windows amd64)
- GPG-signed `SHA256SUMS` (`outputs/release/1.0.0/SHA256SUMS.asc`)
- Motif workbench vendored (Lumen-managed renderer; not independent MCP)
- 40 active connectors + 2 rejected (BioGRID, KEGG)
- Live probes PASS: PubMed, ChEMBL, Europe PMC, Crossref, UniProt
- Live brief matrix PASS (aspirin, BRCA2, CRISPR Cas9, metformin)
- 10 skills approved with controlled tools only; 17 GPU/remote pending

## Install
```bash
git clone https://github.com/exergyleizhou-ux/lumen-science
cd lumen-science && git checkout v1.0.0
./scripts/install-science.sh
lumen-science version
```

Or build release matrix:
```bash
cd packs/science && make release
shasum -a 256 -c dist/science-release/SHA256SUMS
```

## Not claimed
- Medical/clinical certification
- Embodied lab / Lumen Science 5.0
- All 27 skills compute-ready
