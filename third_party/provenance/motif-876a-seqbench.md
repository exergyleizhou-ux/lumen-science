# Motif `876a4f9e` → Rust seqbench algorithm adaptation

| Field | Value |
|---|---|
| Repository | https://github.com/jvogan/motif.git |
| Exact commit | `876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0` |
| Root license | MIT |
| Lumen implementation | `agent/crates/codegen/xai-grok-science/src/seqbench.rs` |
| Lumen tool version | `lumen-seqbench 1.4.0` |
| Analysis schema | 5 |
| Execution authority | Rust `SessionActor` |
| Network/provider behavior | none; deterministic and offline |

## Source-level mapping

This is a cross-language Rust adaptation of the upstream TypeScript
implementation, not a behavior-only rewrite:

| Motif source | SHA-256 | Rust behavior adapted |
|---|---|---|
| `src/bio/fasta-parser.ts` | `a762b07d0ca099912fe929482dc5054b51ce697abd78f8491472fd9b02b47d3d` | first-whitespace header parsing, PIR comments, ASCII sequence cleaning, gap-removal accounting |
| `src/bio/gc-content.ts` | `a26ee1474e69db5f9c1846b68746c7cc62ca5bd82f52ab7d212851747592e379` | nucleotide composition, GC/AT fractions, DNA/RNA molecular weight, short/long Tm estimates, average and monoisotopic protein mass |
| `src/bio/reverse-complement.ts` | `d095df3c7a8d6be3b86fa718b8aaf47ded8608752c34ebaa451eda489d4cb30d` | complete DNA/RNA IUPAC complement table |
| `src/bio/translate.ts` | `91c480612f998383baa7b858a5ca39c9437621576710c829a86dd649aba57fc6` | case-insensitive RNA normalization and standard-code frame translation |
| `src/bio/codon-tables.ts` | `87a683dcd4d3152f71c1d9e3c5efa6af32c7df3ff6d695bd4077f68d570e0304` | all 24 single-valued NCBI tables Motif ships, including names, starts, stops and codon overrides |
| `src/bio/orf-detection.ts` | `e163ddbb40ca051bdd723f1918588b1c341fec1171fe2572359990e8e1b44c1c` | table-aware six-frame ORF scan, nested starts, terminal ORFs, reverse coordinates and length ordering |
| `src/bio/restriction-sites.ts` | `1ffe08381227a498db2ccfd96862e33f0090cf77d35e64e019419d1cfc182fec` | exact 30-enzyme default panel, overlapping IUPAC recognition, forward/reverse scanning, mirrored Type IIS cuts and circular-origin scanning |

The Rust implementation preserves Motif's one-based frame plus explicit strand
metadata and adds a Lumen-specific 50-ORF output cap and explicit 100-hit
restriction-site cap. Restriction topology is frozen into the durable run
before approval. It does not import Motif's MCP server, Claude installer, Node
runtime, filesystem authority, digest feature mapper, full 154-enzyme catalog,
or external alignment runners.

Motif omits NCBI tables 27, 28 and 31 because their context-dependent
sense/stop semantics cannot be represented by a single codon map. Lumen keeps
that exact honesty boundary and rejects unknown or omitted table IDs instead
of silently falling back to table 1.

## Cross-language conformance

The exact Motif source files above were executed locally through Lumen's
already-installed `vite-node`, with no network or dependency install. The Rust
focused tests use the same inputs and require the same outputs:

| Case | Motif and Rust result |
|---|---|
| FASTA header, PIR comment, gaps and noise | id `seq-1`; description `with description`; sequence `AATTGGCCN`; 2 gaps removed |
| nucleotide composition | A=2, T=2, U=0, G=2, C=2, N=1, other=0 |
| GC fraction / Tm | 0.5 / 24 °C |
| nucleotide molecular weight | 2732.38 Da |
| protein `ACDE` average / monoisotopic mass | 436.44 / 436.13 Da |
| DNA / RNA IUPAC reverse complement | `NBDHVKMWSRYACGT` / `NBDHVKMWSRYACGU` |
| lowercase RNA translation `augugauga` | `M**` |
| nested standard-table ORFs `TTGATGAAATAA` | starts 0 and 3; end 12; 3 aa then 2 aa; `TTG` / `ATG` starts |
| terminal ORF `ATGAAA` | start 0; end 6; 2 aa; empty stop codon |
| reverse ORF `TTATTTCAT` | frame 1; strand -1; forward coordinates 0..9; `ATG` → `TAA` |
| translation-table registry | IDs `1,2,3,4,5,6,9,10,11,12,13,14,15,16,21,22,23,24,25,26,29,30,32,33` and all names match |
| table 2 / 15 / 32 translation | `MW*` / `MQ*` / `MW*` |
| table 1 vs table 2 ORF `ATGAAAAGATTTTAA` | end 15 at `TAA` vs end 9 at `AGA` |
| default restriction catalog | 30 entries; canonical SHA-256 `12598695d6a6476911608991c27370d95969dde13c08e219f941310a221d841a` |
| reverse BsaI `AAAAAAGAGACCTTTTT` | match 6; cut 1; strand -1 |
| EcoRI across `AATTCCCCCG` origin | no linear hit; circular match 9 and cut 0 |
| degenerate `RGATCY` recognition | `AGATCC` matches; sequence-side `NGATCC` does not |

## Durable evidence boundary

- `analysis.json` records the exact Motif repository, commit, license and
  adapted component paths.
- durable provenance repeats the repository, commit and license in its
  environment map.
- outputs remain store-owned SHA-256 artifacts and are impossible before an
  `Allow` decision.
- deny, timeout, cancel, owner/project/call mismatch and parse failure retain
  the pre-existing fail-closed terminal behavior.

All four slices have fresh rebuilt-binary evidence. The schema-5 product seam
reopened the store-owned output and verified the 30-enzyme panel, circular
topology, an origin-spanning EcoRI match at 9 with cut 0, table-2 ORF output,
and durable topology context/provenance. All three filtered
allow/boundary/deny tests passed. No CI, live/provider, release, or deployment
proof is claimed.
