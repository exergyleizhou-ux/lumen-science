//! Offline primer secondary-structure screening, adapted from Motif.
//!
//! This is a pure domain helper: it has no filesystem, process, network, ACP,
//! or SessionActor authority.  Product admission is deliberately separate.
//! Source: `jvogan/motif@876a4f9`, `src/bio/primer-thermodynamics.ts`
//! SHA-256 `464f85110fc071e5e30b95a7ff7c4b8e066a35d5e97f4fb003005554ad5ed72e`.
//! The nearest-neighbor parameter table is from `src/bio/tm-calculator.ts`
//! SHA-256 `b6f5fc408a01d6dff5aef85adc7706466bfb22f478e5278aa6c087e5eb8eb0d2`.

use serde::{Deserialize, Serialize};

const T37_KELVIN: f64 = 310.15;
const INIT_H_KCAL_PER_MOL: f64 = 0.1;
const INIT_S_CAL_PER_MOL_K: f64 = -2.8;
const MIN_STEM_OR_DIMER_BASES: usize = 3;
const MIN_HAIRPIN_LOOP_BASES: usize = 3;
/// At most 16 candidates gives at most 120 unordered hetero-dimer screens.
pub const MAX_PRIMER_CANDIDATES: usize = 16;
/// Bound the durable request surface as well as the screening cost.
pub const MAX_PRIMER_LENGTH: usize = 120;

/// Approximate first-order screening cutoff compatible with the source slice.
pub const DEFAULT_MAX_HAIRPIN_DELTA_G_KCAL_PER_MOL: f64 = -3.0;
/// Approximate first-order screening cutoff compatible with the source slice.
pub const DEFAULT_MAX_DIMER_DELTA_G_KCAL_PER_MOL: f64 = -5.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HairpinResult {
    /// Most-negative predicted delta-G at 37 C, in kcal/mol.
    pub delta_g: f64,
    pub stem_length: usize,
    pub loop_size: usize,
    pub structure: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DimerResult {
    /// Most-negative predicted delta-G at 37 C, in kcal/mol.
    pub delta_g: f64,
    pub pair_length: usize,
    pub offset: isize,
    pub structure: String,
}

/// One input primer after the deterministic DNA normalization used by Motif.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimerScreen {
    pub sequence: String,
    pub hairpin: HairpinResult,
    pub self_dimer: DimerResult,
}

/// One unordered pair of candidate primers, represented by input indices.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimerPairDimer {
    pub first_index: usize,
    pub second_index: usize,
    pub dimer: DimerResult,
}

/// Deterministic secondary-structure screen for a bounded caller-owned list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimerThermodynamicsReport {
    pub primers: Vec<PrimerScreen>,
    pub hetero_dimers: Vec<PrimerPairDimer>,
}

fn round_hundredth(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn normalized_dna(value: &str) -> String {
    value
        .bytes()
        .filter_map(|byte| match byte.to_ascii_uppercase() {
            b'A' => Some('A'),
            b'C' => Some('C'),
            b'G' => Some('G'),
            b'T' | b'U' => Some('T'),
            _ => None,
        })
        .collect()
}

/// Canonical, lossless admission form for caller-supplied primer candidates.
///
/// Motif's UI helper intentionally strips non-base characters. That is useful
/// for interactive display, but it is unsafe at a durable authority boundary:
/// two distinct requests must not silently become the same approved request.
/// The product path therefore accepts only A/C/G/T/U (case-insensitive),
/// normalizes RNA U to DNA T, and rejects duplicates before hashing or writing.
pub fn canonical_primer_candidates(candidates: &[String]) -> Result<Vec<String>, String> {
    if candidates.len() > MAX_PRIMER_CANDIDATES {
        return Err(format!(
            "primer screening accepts at most {MAX_PRIMER_CANDIDATES} candidates"
        ));
    }
    let mut canonical = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if candidate.is_empty() || candidate.len() > MAX_PRIMER_LENGTH {
            return Err(format!(
                "each primer candidate must be 1..={MAX_PRIMER_LENGTH} nucleotide bases"
            ));
        }
        if !candidate
            .bytes()
            .all(|byte| matches!(byte.to_ascii_uppercase(), b'A' | b'C' | b'G' | b'T' | b'U'))
        {
            return Err("primer candidates may contain only A, C, G, T, or U".into());
        }
        let normalized = normalized_dna(candidate);
        if canonical.contains(&normalized) {
            return Err("primer candidates must be unique after DNA normalization".into());
        }
        canonical.push(normalized);
    }
    Ok(canonical)
}

fn reverse_complement_dna(value: &str) -> String {
    value
        .bytes()
        .rev()
        .filter_map(|byte| match byte {
            b'A' => Some('T'),
            b'T' => Some('A'),
            b'C' => Some('G'),
            b'G' => Some('C'),
            _ => None,
        })
        .collect()
}

fn nn_params(pair: &[u8]) -> Option<(f64, f64)> {
    match pair {
        b"AA" | b"TT" => Some((-7.9, -22.2)),
        b"AT" => Some((-7.2, -20.4)),
        b"TA" => Some((-7.2, -21.3)),
        b"CA" | b"TG" => Some((-8.5, -22.7)),
        b"GT" | b"AC" => Some((-8.4, -22.4)),
        b"CT" | b"AG" => Some((-7.8, -21.0)),
        b"GA" | b"TC" => Some((-8.2, -22.2)),
        b"CG" => Some((-10.6, -27.2)),
        b"GC" => Some((-9.8, -24.4)),
        b"GG" | b"CC" => Some((-8.0, -19.9)),
        _ => None,
    }
}

/// SantaLucia-parameter delta-G at 37 C for a perfect Watson-Crick duplex.
fn nearest_neighbor_delta_g(top_strand: &str) -> f64 {
    let bytes = top_strand.as_bytes();
    if bytes.len() < 2 {
        return 0.0;
    }
    let mut enthalpy = 2.0 * INIT_H_KCAL_PER_MOL;
    let mut entropy = 2.0 * INIT_S_CAL_PER_MOL_K;
    for pair in bytes.windows(2) {
        if let Some((delta_h, delta_s)) = nn_params(pair) {
            enthalpy += delta_h;
            entropy += delta_s;
        }
    }
    enthalpy - (T37_KELVIN * entropy) / 1000.0
}

/// Find the strongest contiguous self-complementary stem with a >=3-base loop.
pub fn predict_hairpin(primer: &str) -> HairpinResult {
    let sequence = normalized_dna(primer);
    let mut best = HairpinResult {
        delta_g: 0.0,
        stem_length: 0,
        loop_size: 0,
        structure: String::new(),
    };
    if sequence.len() < 2 * MIN_STEM_OR_DIMER_BASES + MIN_HAIRPIN_LOOP_BASES {
        return best;
    }
    for start in 0..sequence.len() {
        for stem_length in MIN_STEM_OR_DIMER_BASES.. {
            let after_left = start + stem_length;
            if after_left + MIN_HAIRPIN_LOOP_BASES + stem_length > sequence.len() {
                break;
            }
            for loop_size in MIN_HAIRPIN_LOOP_BASES.. {
                let right_start = after_left + loop_size;
                let right_end = right_start + stem_length;
                if right_end > sequence.len() {
                    break;
                }
                let left = &sequence[start..after_left];
                let right = &sequence[right_start..right_end];
                if left != reverse_complement_dna(right) {
                    continue;
                }
                let delta_g = nearest_neighbor_delta_g(left);
                if delta_g < best.delta_g {
                    best = HairpinResult {
                        delta_g: round_hundredth(delta_g),
                        stem_length,
                        loop_size,
                        structure: format!("5'-{left}-{}-{right}-3'", ".".repeat(loop_size)),
                    };
                }
            }
        }
    }
    best
}

/// Find the strongest contiguous duplex between two primers (or a self dimer).
pub fn predict_primer_dimer(first: &str, second: &str) -> DimerResult {
    let first = normalized_dna(first);
    let second_reverse_complement = reverse_complement_dna(&normalized_dna(second));
    let mut best = DimerResult {
        delta_g: 0.0,
        pair_length: 0,
        offset: 0,
        structure: String::new(),
    };
    if first.len() < MIN_STEM_OR_DIMER_BASES
        || second_reverse_complement.len() < MIN_STEM_OR_DIMER_BASES
    {
        return best;
    }
    for offset in -(second_reverse_complement.len() as isize - 1)..first.len() as isize {
        let start = usize::try_from((-offset).max(0)).unwrap_or(0);
        let end = first
            .len()
            .min((second_reverse_complement.len() as isize - offset) as usize);
        let mut run_start = 0;
        let mut run_length = 0;
        let mut best_run_start = None;
        let mut best_run_length = 0;
        for index in start..end {
            let other = (index as isize + offset) as usize;
            if first.as_bytes()[index] == second_reverse_complement.as_bytes()[other] {
                if run_length == 0 {
                    run_start = index;
                }
                run_length += 1;
                if run_length > best_run_length {
                    best_run_length = run_length;
                    best_run_start = Some(run_start);
                }
            } else {
                run_length = 0;
            }
        }
        if best_run_length < MIN_STEM_OR_DIMER_BASES {
            continue;
        }
        let matched = &first[best_run_start.expect("set with positive run")..][..best_run_length];
        let delta_g = nearest_neighbor_delta_g(matched);
        if delta_g < best.delta_g {
            best = DimerResult {
                delta_g: round_hundredth(delta_g),
                pair_length: best_run_length,
                offset,
                structure: format!(
                    "5'-{matched}-3' ({best_run_length} bp duplex, offset {offset})"
                ),
            };
        }
    }
    best
}

pub fn predict_self_dimer(primer: &str) -> DimerResult {
    predict_primer_dimer(primer, primer)
}

/// Screen each candidate and each unordered pair.  This helper intentionally
/// accepts no paths, handles, commands, URLs, or mutable stores.
pub fn screen_primer_candidates(
    candidates: &[String],
) -> Result<PrimerThermodynamicsReport, String> {
    if candidates.len() > MAX_PRIMER_CANDIDATES {
        return Err(format!(
            "primer candidate count {} exceeds maximum {MAX_PRIMER_CANDIDATES}",
            candidates.len()
        ));
    }
    let normalized: Vec<String> = candidates
        .iter()
        .map(|candidate| normalized_dna(candidate))
        .collect();
    let primers = normalized
        .iter()
        .map(|sequence| PrimerScreen {
            sequence: sequence.clone(),
            hairpin: predict_hairpin(sequence),
            self_dimer: predict_self_dimer(sequence),
        })
        .collect();
    let mut hetero_dimers = Vec::new();
    for first_index in 0..normalized.len() {
        for second_index in first_index + 1..normalized.len() {
            hetero_dimers.push(PrimerPairDimer {
                first_index,
                second_index,
                dimer: predict_primer_dimer(&normalized[first_index], &normalized[second_index]),
            });
        }
    }
    Ok(PrimerThermodynamicsReport {
        primers,
        hetero_dimers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_rna_case_and_non_bases_without_authority_or_io() {
        assert_eq!(normalized_dna(" aUgc-n "), "ATGC");
        assert_eq!(predict_hairpin("NNNN").delta_g, 0.0);
    }

    #[test]
    fn no_minimum_stem_or_loop_means_no_hairpin() {
        assert_eq!(predict_hairpin("ATGCGCAT").delta_g, 0.0);
        assert_eq!(predict_hairpin("ATGAAACAT").stem_length, 3);
    }

    #[test]
    fn predicts_known_perfect_hairpin_stem() {
        let result = predict_hairpin("ATGAAACAT");
        assert_eq!(result.stem_length, 3);
        assert_eq!(result.loop_size, 3);
        assert_eq!(result.structure, "5'-ATG-...-CAT-3'");
        assert!(result.delta_g < 0.0);
    }

    #[test]
    fn complementary_primers_form_a_dimer_and_order_is_symmetric_in_energy() {
        let first = predict_primer_dimer("CGCTCGGTACG", "CGTACCGAGCG");
        let second = predict_primer_dimer("CGTACCGAGCG", "CGCTCGGTACG");
        assert!(first.pair_length >= 3 && first.delta_g < DEFAULT_MAX_DIMER_DELTA_G_KCAL_PER_MOL);
        assert_eq!(first.delta_g, second.delta_g);
        assert_eq!(predict_self_dimer("AAAA").delta_g, 0.0);
    }

    #[test]
    fn cross_language_reference_vectors_match_fixed_motif_outputs() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../fixtures/motif_primer_thermo_reference.json"
        ))
        .expect("Motif primer reference fixture parses");
        assert_eq!(
            fixture["upstream"]["commit"],
            "876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0"
        );
        for expected in fixture["hairpins"].as_array().expect("hairpins array") {
            let result = predict_hairpin(expected["primer"].as_str().expect("primer"));
            assert_eq!(result.delta_g, expected["deltaG"].as_f64().expect("deltaG"));
            assert_eq!(
                result.stem_length,
                expected["stemLength"].as_u64().expect("stem") as usize
            );
            assert_eq!(
                result.loop_size,
                expected["loopSize"].as_u64().expect("loop") as usize
            );
            assert_eq!(
                result.structure,
                expected["structure"].as_str().expect("structure")
            );
        }
        for expected in fixture["dimers"].as_array().expect("dimers array") {
            let result = predict_primer_dimer(
                expected["first"].as_str().expect("first"),
                expected["second"].as_str().expect("second"),
            );
            assert_eq!(result.delta_g, expected["deltaG"].as_f64().expect("deltaG"));
            assert_eq!(
                result.pair_length,
                expected["pairLength"].as_u64().expect("pair length") as usize
            );
            assert_eq!(
                result.offset,
                expected["offset"].as_i64().expect("offset") as isize
            );
            assert_eq!(
                result.structure,
                expected["structure"].as_str().expect("structure")
            );
        }
    }

    #[test]
    fn bounded_report_keeps_only_unordered_hetero_pairs_and_normalized_sequences() {
        let report = screen_primer_candidates(&[
            "AUGC-N".to_owned(),
            "CGCTCGGTACG".to_owned(),
            "CGTACCGAGCG".to_owned(),
        ])
        .expect("bounded candidates");
        assert_eq!(report.primers.len(), 3);
        assert_eq!(report.primers[0].sequence, "ATGC");
        assert_eq!(report.hetero_dimers.len(), 3);
        assert_eq!(report.hetero_dimers[0].first_index, 0);
        assert_eq!(report.hetero_dimers[0].second_index, 1);
        let strongest = report
            .hetero_dimers
            .iter()
            .find(|pair| pair.first_index == 1 && pair.second_index == 2)
            .expect("complementary pair exists");
        assert_eq!(strongest.dimer.delta_g, -14.72);
    }

    #[test]
    fn candidate_count_cap_fails_before_quadratic_pairing() {
        let candidates = vec!["ATGC".to_owned(); MAX_PRIMER_CANDIDATES + 1];
        let error = screen_primer_candidates(&candidates).expect_err("candidate cap");
        assert!(error.contains("exceeds maximum"));
    }

    #[test]
    fn durable_admission_is_lossless_and_canonical() {
        assert_eq!(
            canonical_primer_candidates(&["auGc".to_owned()]).unwrap(),
            vec!["ATGC"]
        );
        assert!(canonical_primer_candidates(&["ATGC-N".to_owned()]).is_err());
        assert!(canonical_primer_candidates(&["ATGC".to_owned(), "AUGC".to_owned()]).is_err());
    }
}
