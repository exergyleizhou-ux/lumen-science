# Lumen Science Pack — productivity workbench

Local-first scientific tooling for [Lumen](https://github.com/exergyleizhou-ux/lumen-science).

**Product name today:** Lumen Science **Fusion candidate**  
**Not yet:** formal `v1.0.0` release tag / signed multi-platform release

## What you can do *now* (offline, real work)

```bash
# from repo root
./scripts/install-science.sh

lumen-science version
lumen-science doctor
lumen-science gates

# Motif-class sequence analysis (no network, no shell)
lumen-science seq analyze gene.fa
lumen-science seq analyze --json gene.fa -o analysis.json

# Full offline loop: register → analyze → derived artifacts → integrity review
lumen-science pipeline offline --project myproj --run run1 gene.fa

# Artifact store (SHA-256, fail-closed paths)
lumen-science artifact put --project p --run r --path raw/x.fa gene.fa
lumen-science artifact list --project p --run r
lumen-science artifact verify --project p --run r --path raw/x.fa --sha256 <hex>
```

Live literature brief (network):

```bash
export NCBI_API_KEY=...   # optional but recommended
lumen-science brief aspirin --out brief.md
```

Offline dogfood (CI-friendly):

```bash
./scripts/dogfood-science-offline.sh
```

## Architecture (authority)

```text
Rust Lumen SessionActor     = sole product execution authority
lumen-science CLI           = local productivity adapter
MCP servers (Go)            = contract prototypes
MotifRenderer / sequence UI = CSP review surfaces over registered artifacts
Skills                      = plans only until DS-43 approved (currently 0 approved)
```

## Binaries

| Binary | Role |
|--------|------|
| `lumen-science` | **Primary productivity CLI** |
| `lumen-mcp-artifacts` | Artifact MCP (stdio) |
| `lumen-mcp-notebook` | Python kernel MCP |
| `lumen-mcp-reviewer` | Review MCP |
| `lumen-mcp-http_bridge` | Loopback HTTP bridge |

```bash
cd packs/science
make all
make test
```

## Connectors (Rust crate)

```text
Inventory 42 = 40 active runtime + 2 rejected (BioGRID, KEGG)
Rejected are NOT in connectors::registry()
```

## Renderers

CSP-locked first-class:

- `motif.html` — Lumen-managed MotifRenderer **contract**
- `sequence.html` — FASTA workbench viewer

Legacy CDN renderers remain marked `pending-cdn-elimination`.

## Honesty docs

- `docs/science/LUMEN_SCIENCE_FUSION_CANDIDATE_STATUS.md`
- `docs/science/PRODUCT_PATH_CONTRACT.md`
- `docs/science/MOTIF_SUPPLY_CHAIN_AUDIT.md`

## License

See repo root `LICENSE` / `NOTICE`. Motif MIT notices under `third_party/motif/`.
