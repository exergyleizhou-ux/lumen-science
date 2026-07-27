# Motif `876a4f9e` → Rust seqbench algorithm adaptation

| Field | Value |
|---|---|
| Repository | https://github.com/jvogan/motif.git |
| Exact commit | `876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0` |
| Root license | MIT |
| Lumen implementation | `agent/crates/codegen/xai-grok-science/src/seqbench.rs` |
| Lumen tool version | `lumen-seqbench 1.1.0` |
| Analysis schema | 2 |
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

The Rust implementation preserves Lumen's existing one-based frame labels,
bounded summaries, restriction-site behavior, and SessionActor begin/approval/
finish protocol. It does not import Motif's MCP server, Claude installer,
Node runtime, filesystem authority, or external alignment runners.

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

## Durable evidence boundary

- `analysis.json` records the exact Motif repository, commit, license and
  adapted component paths.
- durable provenance repeats the repository, commit and license in its
  environment map.
- outputs remain store-owned SHA-256 artifacts and are impossible before an
  `Allow` decision.
- deny, timeout, cancel, owner/project/call mismatch and parse failure retain
  the pre-existing fail-closed terminal behavior.

This slice has source, cross-language conformance, focused Rust test, and fresh
rebuilt-binary evidence. The product seam reopened the store-owned
`analysis.json` and verified schema 2, tool 1.1.0, the locked Motif commit,
composition output, and durable provenance; all three `seq_analyze`
allow/boundary/deny built-binary tests passed. This is not CI, live/provider,
release, or deployment proof.
