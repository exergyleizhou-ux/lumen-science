//! Offline Motif-class sequence analysis. Zero network, zero shell.
//! Deterministic — same bytes always produce the same report.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const TOOL: &str = "lumen-seqbench";
pub const TOOL_VERSION: &str = "1.2.0";
pub const MOTIF_REPOSITORY: &str = "https://github.com/jvogan/motif.git";
pub const MOTIF_COMMIT: &str = "876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0";
pub const MOTIF_LICENSE: &str = "MIT";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub sequence: String,
    pub kind: String, // dna | rna | protein | unknown
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gaps_removed: usize,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NucleotideComposition {
    #[serde(rename = "A")]
    pub a: usize,
    #[serde(rename = "T")]
    pub t: usize,
    #[serde(rename = "U")]
    pub u: usize,
    #[serde(rename = "G")]
    pub g: usize,
    #[serde(rename = "C")]
    pub c: usize,
    #[serde(rename = "N")]
    pub n: usize,
    pub other: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AlgorithmSource {
    pub repository: String,
    pub commit: String,
    pub license: String,
    pub components: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Orf {
    pub frame: i32,
    pub strand: i8,
    pub start: usize,
    pub end: usize,
    pub length_bp: usize,
    pub amino_acids: usize,
    pub start_codon: String,
    pub stop_codon: String,
    pub protein: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RestrictionHit {
    pub enzyme: String,
    pub site: String,
    pub position: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecordSummary {
    pub id: String,
    pub kind: String,
    pub length: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub gaps_removed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nucleotide_composition: Option<NucleotideComposition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gc_fraction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gc_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_fraction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_molecular_weight_da: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_melting_temperature_c: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_protein_average_molecular_weight_da: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_protein_monoisotopic_molecular_weight_da: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse_complement: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub translation_frames: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orfs: Vec<Orf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restriction_hits: Vec<RestrictionHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Analysis {
    pub schema_version: u32,
    pub tool: String,
    pub tool_version: String,
    pub source_sha256: String,
    pub algorithm_sources: Vec<AlgorithmSource>,
    pub records: Vec<RecordSummary>,
    pub notes: Vec<String>,
}

/// Parse multi-FASTA or raw sequence. Fail-closed on empty.
///
/// The FASTA record handling is adapted from Motif's `fasta-parser.ts` at
/// [`MOTIF_COMMIT`]: headers split on the first whitespace (not only a literal
/// space), NBRF/PIR `;` comments are ignored, only ASCII letters plus the
/// protein stop glyph survive sequence cleaning, and alignment gaps are
/// counted before removal so the analysis can disclose degapping.
pub fn parse_fasta(raw: &str) -> Result<Vec<Record>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty sequence input".into());
    }
    if !raw.starts_with('>') {
        let (seq, gaps_removed) = normalize_seq_with_gaps(raw);
        if seq.is_empty() {
            return Err("no sequence characters found".into());
        }
        let kind = detect_kind(&seq);
        return Ok(vec![Record {
            id: "unnamed".into(),
            description: String::new(),
            sequence: seq,
            kind,
            gaps_removed,
        }]);
    }
    let mut out = Vec::new();
    let mut cur_id = String::new();
    let mut cur_desc = String::new();
    let mut buf = String::new();
    let mut gaps_removed = 0usize;
    let mut have = false;
    let flush = |id: &str,
                 desc: &str,
                 buf: &mut String,
                 gaps_removed: &mut usize,
                 out: &mut Vec<Record>| {
        let (seq, counted_gaps) = normalize_seq_with_gaps(buf);
        buf.clear();
        if seq.is_empty() {
            *gaps_removed = 0;
            return;
        }
        let kind = detect_kind(&seq);
        out.push(Record {
            id: if id.is_empty() {
                "unnamed".into()
            } else {
                id.to_string()
            },
            description: desc.to_string(),
            sequence: seq,
            kind,
            gaps_removed: (*gaps_removed).max(counted_gaps),
        });
        *gaps_removed = 0;
    };
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with(';') {
            continue;
        }
        if let Some(header) = line.strip_prefix('>') {
            if have {
                flush(&cur_id, &cur_desc, &mut buf, &mut gaps_removed, &mut out);
            }
            let header = header.trim();
            if let Some((index, ch)) = header.char_indices().find(|(_, ch)| ch.is_whitespace()) {
                cur_id = header[..index].to_string();
                cur_desc = header[index + ch.len_utf8()..].trim().to_string();
            } else {
                cur_id = header.to_string();
                cur_desc.clear();
            }
            gaps_removed = 0;
            have = true;
            continue;
        }
        if have && !line.is_empty() {
            gaps_removed += line.chars().filter(|ch| matches!(ch, '-' | '.')).count();
            buf.push_str(line);
        }
    }
    if have {
        flush(&cur_id, &cur_desc, &mut buf, &mut gaps_removed, &mut out);
    }
    if out.is_empty() {
        return Err("no FASTA records parsed".into());
    }
    Ok(out)
}

pub fn analyze(records: &[Record], source_bytes: &[u8]) -> Analysis {
    let source_sha256 = hex_sha256(source_bytes);
    let mut summaries = Vec::with_capacity(records.len());
    for r in records {
        summaries.push(summarize(r));
    }
    Analysis {
        schema_version: 3,
        tool: TOOL.into(),
        tool_version: TOOL_VERSION.into(),
        source_sha256,
        algorithm_sources: vec![AlgorithmSource {
            repository: MOTIF_REPOSITORY.into(),
            commit: MOTIF_COMMIT.into(),
            license: MOTIF_LICENSE.into(),
            components: vec![
                "src/bio/fasta-parser.ts".into(),
                "src/bio/gc-content.ts".into(),
                "src/bio/reverse-complement.ts".into(),
                "src/bio/translate.ts".into(),
                "src/bio/codon-tables.ts".into(),
                "src/bio/orf-detection.ts".into(),
            ],
        }],
        records: summaries,
        notes: vec![
            "Deterministic offline analysis. Not a substitute for wet-lab validation.".into(),
            "Restriction sites are recognition-pattern hits only.".into(),
            "ORFs use NCBI translation table 1 starts/stops; min length 30 aa.".into(),
            format!(
                "FASTA and sequence metrics are adapted from Motif {MOTIF_COMMIT} ({MOTIF_LICENSE})."
            ),
        ],
    }
}

pub fn markdown_report(a: &Analysis, source_label: &str) -> String {
    let mut b = String::new();
    b.push_str("# Sequence analysis report\n\n");
    b.push_str(&format!("- tool: `{}` {}\n", a.tool, a.tool_version));
    b.push_str(&format!("- source: `{source_label}`\n"));
    b.push_str(&format!("- source_sha256: `{}`\n", a.source_sha256));
    b.push_str(&format!("- records: {}\n\n", a.records.len()));
    for r in &a.records {
        b.push_str(&format!("## {}\n\n", r.id));
        b.push_str("| field | value |\n|---|---|\n");
        b.push_str(&format!("| kind | {} |\n", r.kind));
        b.push_str(&format!("| length | {} |\n", r.length));
        if r.gaps_removed > 0 {
            b.push_str(&format!(
                "| alignment gaps removed | {} |\n",
                r.gaps_removed
            ));
        }
        if let Some(gc) = r.gc_percent {
            b.push_str(&format!("| GC% | {gc:.2} |\n"));
        }
        if let Some(tm) = r.estimated_melting_temperature_c {
            b.push_str(&format!("| estimated Tm °C | {tm:.2} |\n"));
        }
        if let Some(mw) = r.estimated_molecular_weight_da {
            b.push_str(&format!("| estimated molecular weight Da | {mw:.2} |\n"));
        }
        if let Some(mw) = r.estimated_protein_average_molecular_weight_da {
            b.push_str(&format!(
                "| estimated protein average molecular weight Da | {mw:.2} |\n"
            ));
        }
        if let Some(mw) = r.estimated_protein_monoisotopic_molecular_weight_da {
            b.push_str(&format!(
                "| estimated protein monoisotopic molecular weight Da | {mw:.2} |\n"
            ));
        }
        b.push('\n');
        if !r.orfs.is_empty() {
            b.push_str("### ORFs (min 30 aa)\n\n");
            b.push_str(
                "| frame | strand | start | end | bp | aa | start codon | stop codon | truncated |\n\
                 |---|---:|---:|---:|---:|---:|---|---|---|\n",
            );
            for o in &r.orfs {
                b.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                    o.frame,
                    o.strand,
                    o.start,
                    o.end,
                    o.length_bp,
                    o.amino_acids,
                    o.start_codon,
                    o.stop_codon,
                    o.truncated
                ));
            }
            b.push('\n');
        }
        if !r.restriction_hits.is_empty() {
            b.push_str("### Restriction sites\n\n");
            b.push_str("| enzyme | site | position |\n|---|---|---:|\n");
            for h in &r.restriction_hits {
                b.push_str(&format!(
                    "| {} | `{}` | {} |\n",
                    h.enzyme, h.site, h.position
                ));
            }
            b.push('\n');
        }
    }
    b.push_str(
        "## Provenance\n\nGenerated offline by Lumen Science seqbench inside the Rust \
         SessionActor. FASTA parsing and sequence-metric algorithms are adapted from \
         [jvogan/motif](https://github.com/jvogan/motif) at commit \
         `876a4f9e5d99af1bc3cf5caa639ce8f5402dfbe0` (MIT). Not medical advice.\n",
    );
    b
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

fn summarize(r: &Record) -> RecordSummary {
    let mut s = RecordSummary {
        id: r.id.clone(),
        kind: r.kind.clone(),
        length: r.sequence.len(),
        gaps_removed: r.gaps_removed,
        nucleotide_composition: None,
        gc_fraction: None,
        gc_percent: None,
        at_fraction: None,
        estimated_molecular_weight_da: None,
        estimated_melting_temperature_c: None,
        estimated_protein_average_molecular_weight_da: None,
        estimated_protein_monoisotopic_molecular_weight_da: None,
        reverse_complement: None,
        translation_frames: BTreeMap::new(),
        orfs: Vec::new(),
        restriction_hits: Vec::new(),
    };
    if r.kind == "dna" || r.kind == "rna" {
        let composition = nucleotide_composition(&r.sequence);
        let gc = gc_fraction_from_composition(&composition);
        s.nucleotide_composition = Some(composition.clone());
        s.gc_fraction = Some(gc);
        s.gc_percent = Some(gc * 100.0);
        s.at_fraction = Some(at_fraction_from_composition(&composition));
        s.estimated_molecular_weight_da = Some(molecular_weight(&r.sequence));
        s.estimated_melting_temperature_c = melting_temperature_from_composition(&composition);
        let rna = r.kind == "rna";
        s.reverse_complement = Some(reverse_complement(&r.sequence, rna));
        for frame in 1..=3 {
            let pep = translate(&r.sequence, frame);
            let short = if pep.len() > 80 {
                format!("{}…", &pep[..80])
            } else {
                pep
            };
            s.translation_frames.insert(format!("+{frame}"), short);
            let rc = reverse_complement(&r.sequence, rna);
            let pep_n = translate(&rc, frame);
            let short_n = if pep_n.len() > 80 {
                format!("{}…", &pep_n[..80])
            } else {
                pep_n
            };
            s.translation_frames.insert(format!("-{frame}"), short_n);
        }
        s.orfs = find_orfs(&r.sequence, 30);
        if r.kind == "dna" {
            s.restriction_hits = find_restriction_sites(&r.sequence);
        }
    } else if r.kind == "protein" {
        s.estimated_protein_average_molecular_weight_da = Some(protein_molecular_weight(
            &r.sequence,
            ProteinMassMode::Average,
        ));
        s.estimated_protein_monoisotopic_molecular_weight_da = Some(protein_molecular_weight(
            &r.sequence,
            ProteinMassMode::Monoisotopic,
        ));
    }
    s
}

fn normalize_seq_with_gaps(s: &str) -> (String, usize) {
    let gaps_removed = s.chars().filter(|ch| matches!(ch, '-' | '.')).count();
    let sequence = s
        .chars()
        .map(|ch| ch.to_ascii_uppercase())
        .filter(|ch| ch.is_ascii_alphabetic() || *ch == '*')
        .collect();
    (sequence, gaps_removed)
}

fn detect_kind(seq: &str) -> String {
    let mut nucleotide_like = 0usize;
    let mut t = 0usize;
    let mut u = 0usize;
    let mut protein_signal = 0usize;
    for ch in seq.chars() {
        if matches!(
            ch,
            'A' | 'C'
                | 'G'
                | 'T'
                | 'U'
                | 'N'
                | 'R'
                | 'Y'
                | 'S'
                | 'W'
                | 'K'
                | 'M'
                | 'B'
                | 'D'
                | 'H'
                | 'V'
        ) {
            nucleotide_like += 1;
        }
        match ch {
            'T' => t += 1,
            'U' => u += 1,
            'E' | 'F' | 'I' | 'L' | 'P' | 'Q' | 'Z' | 'X' | '*' => protein_signal += 1,
            _ => (),
        }
    }
    let n = seq.len().max(1);
    if (nucleotide_like as f64) / (n as f64) >= 0.8 {
        if u > t {
            return "rna".into();
        }
        return "dna".into();
    }
    if protein_signal > 0 {
        return "protein".into();
    }
    "unknown".into()
}

// The following composition, molecular-weight and Tm functions are direct
// Rust adaptations of Motif's `src/bio/gc-content.ts` at `MOTIF_COMMIT`.
fn nucleotide_composition(seq: &str) -> NucleotideComposition {
    let mut composition = NucleotideComposition::default();
    for ch in seq.chars() {
        match ch {
            'A' | 'a' => composition.a += 1,
            'T' | 't' => composition.t += 1,
            'U' | 'u' => composition.u += 1,
            'G' | 'g' => composition.g += 1,
            'C' | 'c' => composition.c += 1,
            'N' | 'n' => composition.n += 1,
            _ => composition.other += 1,
        }
    }
    composition
}

fn canonical_nucleotide_count(composition: &NucleotideComposition) -> usize {
    composition.a + composition.t + composition.u + composition.g + composition.c
}

fn gc_fraction_from_composition(composition: &NucleotideComposition) -> f64 {
    let total = canonical_nucleotide_count(composition);
    if total == 0 {
        0.0
    } else {
        (composition.g + composition.c) as f64 / total as f64
    }
}

fn at_fraction_from_composition(composition: &NucleotideComposition) -> f64 {
    let total = canonical_nucleotide_count(composition);
    if total == 0 {
        0.0
    } else {
        (composition.a + composition.t + composition.u) as f64 / total as f64
    }
}

fn molecular_weight(seq: &str) -> f64 {
    let upper = seq.to_ascii_uppercase().replace('U', "T");
    if upper.is_empty() {
        return 0.0;
    }
    let mut mass = 0.0;
    for ch in upper.chars() {
        mass += match ch {
            'A' => 313.21,
            'T' => 304.19,
            'G' => 329.21,
            'C' => 289.18,
            _ => 308.95,
        };
    }
    mass -= (upper.len() - 1) as f64 * 18.02;
    mass += 17.01 + 79.0;
    round_hundredths(mass)
}

fn melting_temperature_from_composition(composition: &NucleotideComposition) -> Option<f64> {
    let total = canonical_nucleotide_count(composition);
    if total == 0 {
        return None;
    }
    let at = composition.a + composition.t + composition.u;
    let gc = composition.g + composition.c;
    if total <= 20 {
        Some((2 * at + 4 * gc) as f64)
    } else {
        Some(64.9 + 41.0 * (gc as f64 - 16.4) / total as f64)
    }
}

#[derive(Debug, Clone, Copy)]
enum ProteinMassMode {
    Average,
    Monoisotopic,
}

fn protein_molecular_weight(seq: &str, mode: ProteinMassMode) -> f64 {
    let residues = seq
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .map(|ch| ch.to_ascii_uppercase())
        .collect::<Vec<_>>();
    if residues.is_empty() {
        return 0.0;
    }
    let mut mass = match mode {
        ProteinMassMode::Average => 18.015,
        ProteinMassMode::Monoisotopic => 18.0106,
    };
    for residue in residues {
        mass += amino_acid_residue_mass(residue, mode);
    }
    round_hundredths(mass)
}

fn amino_acid_residue_mass(residue: char, mode: ProteinMassMode) -> f64 {
    match mode {
        ProteinMassMode::Average => match residue {
            'G' => 57.052,
            'A' => 71.079,
            'V' => 99.133,
            'L' | 'I' => 113.160,
            'P' => 97.117,
            'F' => 147.177,
            'W' => 186.213,
            'M' => 131.199,
            'S' => 87.078,
            'T' => 101.105,
            'C' => 103.145,
            'Y' => 163.176,
            'H' => 137.141,
            'D' => 115.089,
            'E' => 129.116,
            'N' => 114.104,
            'Q' => 128.131,
            'K' => 128.174,
            'R' => 156.188,
            _ => 111.1,
        },
        ProteinMassMode::Monoisotopic => match residue {
            'G' => 57.0215,
            'A' => 71.0371,
            'V' => 99.0684,
            'L' | 'I' => 113.0841,
            'P' => 97.0528,
            'F' => 147.0684,
            'W' => 186.0793,
            'M' => 131.0405,
            'S' => 87.0320,
            'T' => 101.0477,
            'C' => 103.0092,
            'Y' => 163.0633,
            'H' => 137.0589,
            'D' => 115.0269,
            'E' => 129.0426,
            'N' => 114.0429,
            'Q' => 128.0586,
            'K' => 128.0949,
            'R' => 156.1011,
            _ => 111.1,
        },
    }
}

fn round_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

// IUPAC complement semantics are adapted from Motif's
// `src/bio/reverse-complement.ts` at `MOTIF_COMMIT`.
fn reverse_complement(seq: &str, rna: bool) -> String {
    let map = |c: char| -> char {
        match c {
            'A' => {
                if rna {
                    'U'
                } else {
                    'T'
                }
            }
            'T' | 'U' => 'A',
            'G' => 'C',
            'C' => 'G',
            'R' => 'Y',
            'Y' => 'R',
            'S' => 'S',
            'W' => 'W',
            'K' => 'M',
            'M' => 'K',
            'B' => 'V',
            'D' => 'H',
            'H' => 'D',
            'V' => 'B',
            'N' => 'N',
            other => other,
        }
    };
    seq.chars().rev().map(map).collect()
}

fn to_dna(seq: &str) -> String {
    seq.chars()
        .map(|ch| match ch.to_ascii_uppercase() {
            'U' => 'T',
            upper => upper,
        })
        .collect()
}

// Standard-code frame translation is adapted from Motif's
// `src/bio/translate.ts` at `MOTIF_COMMIT`. Lumen's public frame labels remain
// one-based (+1..+3 / -1..-3), so the offset conversion stays at this boundary.
fn translate(seq: &str, frame: usize) -> String {
    if !(1..=3).contains(&frame) {
        return String::new();
    }
    let dna = to_dna(seq);
    let bytes = dna.as_bytes();
    let mut out = String::new();
    let mut i = frame - 1;
    while i + 3 <= bytes.len() {
        let codon = std::str::from_utf8(&bytes[i..i + 3]).unwrap_or("NNN");
        out.push(genetic_code(codon));
        i += 3;
    }
    out
}

fn genetic_code(codon: &str) -> char {
    match codon {
        "TTT" | "TTC" => 'F',
        "TTA" | "TTG" | "CTT" | "CTC" | "CTA" | "CTG" => 'L',
        "TCT" | "TCC" | "TCA" | "TCG" | "AGT" | "AGC" => 'S',
        "TAT" | "TAC" => 'Y',
        "TAA" | "TAG" | "TGA" => '*',
        "TGT" | "TGC" => 'C',
        "TGG" => 'W',
        "CCT" | "CCC" | "CCA" | "CCG" => 'P',
        "CAT" | "CAC" => 'H',
        "CAA" | "CAG" => 'Q',
        "CGT" | "CGC" | "CGA" | "CGG" | "AGA" | "AGG" => 'R',
        "ATT" | "ATC" | "ATA" => 'I',
        "ATG" => 'M',
        "ACT" | "ACC" | "ACA" | "ACG" => 'T',
        "AAT" | "AAC" => 'N',
        "AAA" | "AAG" => 'K',
        "GTT" | "GTC" | "GTA" | "GTG" => 'V',
        "GCT" | "GCC" | "GCA" | "GCG" => 'A',
        "GAT" | "GAC" => 'D',
        "GAA" | "GAG" => 'E',
        "GGT" | "GGC" | "GGA" | "GGG" => 'G',
        _ => 'X',
    }
}

const STANDARD_START_CODONS: [&str; 3] = ["TTG", "CTG", "ATG"];
const STANDARD_STOP_CODONS: [&str; 3] = ["TAA", "TAG", "TGA"];

// Direct Rust adaptation of Motif's `src/bio/orf-detection.ts` and the NCBI
// table-1 start/stop sets in `src/bio/codon-tables.ts` at `MOTIF_COMMIT`.
// Lumen keeps the same six-frame scan, nested-start behavior, terminal
// no-stop ORFs, reverse-strand coordinate mapping, and length-descending sort.
// The only product-specific addition is a 50-record output cap.
fn find_orfs(seq: &str, min_aa: usize) -> Vec<Orf> {
    let dna = to_dna(seq);
    let mut out = Vec::new();
    let sequence_len = dna.len();
    if sequence_len < 3 {
        return out;
    }

    for frame_offset in 0..3 {
        out.extend(orfs_in_frame(&dna, frame_offset, 1, min_aa));
    }

    let reverse = reverse_complement(&dna, false);
    for frame_offset in 0..3 {
        for mut orf in orfs_in_frame(&reverse, frame_offset, -1, min_aa) {
            let reverse_start = orf.start;
            let reverse_end = orf.end;
            orf.start = sequence_len - reverse_end;
            orf.end = sequence_len - reverse_start;
            out.push(orf);
        }
    }

    out.sort_by(|left, right| right.length_bp.cmp(&left.length_bp));
    out.truncate(50);
    out
}

fn orfs_in_frame(seq: &str, frame_offset: usize, strand: i8, min_aa: usize) -> Vec<Orf> {
    let mut start_positions = Vec::new();
    let mut stop_positions = Vec::new();
    let mut position = frame_offset;
    while position + 2 < seq.len() {
        let codon = &seq[position..position + 3];
        if STANDARD_START_CODONS.contains(&codon) {
            start_positions.push(position);
        }
        if STANDARD_STOP_CODONS.contains(&codon) {
            stop_positions.push(position);
        }
        position += 3;
    }

    let mut out = Vec::new();
    let mut next_stop_index = 0usize;
    for start in start_positions {
        while next_stop_index < stop_positions.len() && stop_positions[next_stop_index] <= start {
            next_stop_index += 1;
        }

        let (end, amino_acids, stop_codon, truncated) =
            if let Some(stop) = stop_positions.get(next_stop_index).copied() {
                let end = stop + 3;
                (
                    end,
                    (end - start) / 3 - 1,
                    seq[stop..end].to_string(),
                    false,
                )
            } else {
                let end = seq.len() - ((seq.len() - start) % 3);
                (end, (end - start) / 3, String::new(), true)
            };
        if amino_acids < min_aa {
            continue;
        }

        let translated_end = if truncated { end } else { end - 3 };
        out.push(Orf {
            frame: frame_offset as i32 + 1,
            strand,
            start,
            end,
            length_bp: end - start,
            amino_acids,
            start_codon: seq[start..start + 3].to_string(),
            stop_codon,
            protein: translate(&seq[start..translated_end], 1),
            truncated,
        });
    }
    out
}

const ENZYMES: &[(&str, &str)] = &[
    ("EcoRI", "GAATTC"),
    ("BamHI", "GGATCC"),
    ("HindIII", "AAGCTT"),
    ("XhoI", "CTCGAG"),
    ("NotI", "GCGGCCGC"),
    ("NdeI", "CATATG"),
    ("NcoI", "CCATGG"),
    ("SacI", "GAGCTC"),
    ("KpnI", "GGTACC"),
    ("PstI", "CTGCAG"),
    ("SalI", "GTCGAC"),
    ("XbaI", "TCTAGA"),
    ("SpeI", "ACTAGT"),
    ("BglII", "AGATCT"),
    ("ClaI", "ATCGAT"),
];

fn find_restriction_sites(seq: &str) -> Vec<RestrictionHit> {
    let dna = to_dna(seq);
    let mut hits = Vec::new();
    for (name, site) in ENZYMES {
        let mut start = 0;
        while let Some(pos) = dna[start..].find(site) {
            let abs = start + pos;
            hits.push(RestrictionHit {
                enzyme: (*name).into(),
                site: (*site).into(),
                position: abs,
            });
            start = abs + 1;
            if hits.len() >= 100 {
                return hits;
            }
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multi_and_analyze() {
        let raw = ">a BRCA2\nGAATTCATGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAGAAAAAAGCGGCCGCTAA\n";
        let recs = parse_fasta(raw).unwrap();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].kind, "dna");
        let a = analyze(&recs, raw.as_bytes());
        assert!(!a.source_sha256.is_empty());
        let r = &a.records[0];
        assert!(r.restriction_hits.iter().any(|h| h.enzyme == "EcoRI"));
        assert!(r.restriction_hits.iter().any(|h| h.enzyme == "NotI"));
        let md = markdown_report(&a, "x.fa");
        assert!(md.contains("source_sha256"));
    }

    #[test]
    fn empty_fails() {
        assert!(parse_fasta("   ").is_err());
    }

    #[test]
    fn deterministic_hash() {
        let raw = ">x\nATGC\n";
        let r = parse_fasta(raw).unwrap();
        let a1 = analyze(&r, raw.as_bytes());
        let a2 = analyze(&r, raw.as_bytes());
        assert_eq!(a1.source_sha256, a2.source_sha256);
        assert_eq!(markdown_report(&a1, "a"), markdown_report(&a2, "a"));
    }

    #[test]
    fn motif_fasta_port_handles_comments_whitespace_headers_and_gap_provenance() {
        let raw = "  >seq-1\twith description\r\n; legacy PIR comment\r\nAA-TT.GGCCN123?!\r\n";
        let records = parse_fasta(raw).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "seq-1");
        assert_eq!(records[0].description, "with description");
        assert_eq!(records[0].sequence, "AATTGGCCN");
        assert_eq!(records[0].gaps_removed, 2);
        assert_eq!(records[0].kind, "dna");
    }

    #[test]
    fn motif_nucleotide_metrics_match_the_cross_language_reference_values() {
        let raw = ">seq\nAATTGGCCN\n";
        let records = parse_fasta(raw).unwrap();
        let analysis = analyze(&records, raw.as_bytes());
        let summary = &analysis.records[0];
        let composition = summary.nucleotide_composition.as_ref().unwrap();

        assert_eq!(analysis.schema_version, 3);
        assert_eq!(analysis.tool_version, "1.2.0");
        assert_eq!(analysis.algorithm_sources[0].commit, MOTIF_COMMIT);
        assert_eq!(
            composition,
            &NucleotideComposition {
                a: 2,
                t: 2,
                u: 0,
                g: 2,
                c: 2,
                n: 1,
                other: 0,
            }
        );
        assert_eq!(summary.gc_fraction, Some(0.5));
        assert_eq!(summary.gc_percent, Some(50.0));
        assert_eq!(summary.at_fraction, Some(0.5));
        assert_eq!(summary.estimated_melting_temperature_c, Some(24.0));
        assert_eq!(summary.estimated_molecular_weight_da, Some(2732.38));
    }

    #[test]
    fn motif_protein_mass_tables_preserve_average_and_monoisotopic_modes() {
        let raw = ">protein\nACDE\n";
        let records = parse_fasta(raw).unwrap();
        assert_eq!(records[0].kind, "protein");

        let summary = &analyze(&records, raw.as_bytes()).records[0];
        assert_eq!(
            summary.estimated_protein_average_molecular_weight_da,
            Some(436.44)
        );
        assert_eq!(
            summary.estimated_protein_monoisotopic_molecular_weight_da,
            Some(436.13)
        );
        assert!(summary.nucleotide_composition.is_none());
    }

    #[test]
    fn motif_iupac_reverse_complement_and_rna_translation_match() {
        assert_eq!(
            reverse_complement("ACGTRYSWKMBDHVN", false),
            "NBDHVKMWSRYACGT"
        );
        assert_eq!(
            reverse_complement("ACGURYSWKMBDHVN", true),
            "NBDHVKMWSRYACGU"
        );
        assert_eq!(translate("augugauga", 1), "M**");
    }

    #[test]
    fn motif_orf_port_honors_standard_alternative_starts_and_nested_order() {
        let orfs = find_orfs("TTGATGAAATAA", 1);
        let forward = orfs
            .iter()
            .filter(|orf| orf.strand == 1)
            .collect::<Vec<_>>();

        assert_eq!(forward.len(), 2);
        assert_eq!(forward[0].start, 0);
        assert_eq!(forward[0].end, 12);
        assert_eq!(forward[0].length_bp, 12);
        assert_eq!(forward[0].amino_acids, 3);
        assert_eq!(forward[0].start_codon, "TTG");
        assert_eq!(forward[0].stop_codon, "TAA");
        assert_eq!(forward[0].protein, "LMK");
        assert!(!forward[0].truncated);
        assert_eq!(forward[1].start, 3);
        assert_eq!(forward[1].amino_acids, 2);
    }

    #[test]
    fn motif_orf_port_preserves_terminal_orfs_and_reverse_coordinates() {
        let terminal = find_orfs("ATGAAA", 1);
        let terminal = terminal
            .iter()
            .find(|orf| orf.strand == 1 && orf.start == 0)
            .unwrap();
        assert_eq!(terminal.end, 6);
        assert_eq!(terminal.amino_acids, 2);
        assert_eq!(terminal.stop_codon, "");
        assert_eq!(terminal.protein, "MK");
        assert!(terminal.truncated);

        let reverse = find_orfs("TTATTTCAT", 1);
        let reverse = reverse
            .iter()
            .find(|orf| orf.strand == -1 && orf.start == 0)
            .unwrap();
        assert_eq!(reverse.end, 9);
        assert_eq!(reverse.frame, 1);
        assert_eq!(reverse.start_codon, "ATG");
        assert_eq!(reverse.stop_codon, "TAA");
        assert_eq!(reverse.protein, "MK");
    }
}

// ── SessionActor-gated run protocol ──────────────────────────────────────────
//
// `seq_analyze` used to parse, analyse and `std::fs::write` two files straight
// from the ACP request task: no permission prompt, no durable run record, no
// ownership check, and artifacts that existed on disk with no store entry
// claiming them. Every other science mutation goes through the actor's
// begin/decide/finish protocol; this one did not, so the sentence "the
// SessionActor is the sole execution authority" was not true.
//
// The two halves below close that. Analysis itself is pure and offline, so —
// unlike the csv and import paths — there is no external tool to drive: the
// actor decides, then computes and commits inside `finish_analysis`.

use crate::csv::ScienceRunTicket;
use crate::{
    Approval, ApprovalDecision, Artifact, CallId, Evidence, Provenance, RunContext, RunRecord,
    RunState, ScienceError, ScienceStore,
};
use chrono::Utc;
use std::path::Path;

/// Phase one: create the durable run and its pending approval BEFORE the
/// permission manager is awaited, so every allow/deny/timeout/cancel has a
/// record to finish rather than vanishing.
pub fn begin_analysis(
    store: &ScienceStore,
    context: RunContext,
) -> crate::Result<ScienceRunTicket> {
    let ticket = ScienceRunTicket {
        project_id: context.project_id.clone(),
        run_id: context.run_id.clone(),
        owner_id: context.owner_id.clone(),
        call_id: CallId::new("science_seq_analyze"),
    };
    store.create_run(context)?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "run.created",
        serde_json::json!({ "kind": "seq_analyze" }),
    )?;
    store.request_approval(Approval {
        project_id: ticket.project_id.clone(),
        run_id: ticket.run_id.clone(),
        call_id: ticket.call_id.clone(),
        owner_id: ticket.owner_id.clone(),
        decision: ApprovalDecision::Pending,
        decided_at: None,
    })?;
    store.transition(&ticket.run_id, RunState::AwaitingApproval, None)?;
    Ok(ticket)
}

/// What an allowed analysis produced, all of it store-committed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SeqAnalyzeResult {
    pub run: RunRecord,
    pub analysis: Analysis,
    pub artifacts: Vec<Artifact>,
    pub evidence: Vec<Evidence>,
    pub provenance: Vec<Provenance>,
    pub approvals: Vec<Approval>,
    pub records: usize,
    pub replay_after: u64,
}

/// Phase two, on an allowed run: compute, commit both outputs as ARTIFACTS
/// (hashed and owned by the store, not loose files), record provenance, and
/// land the run in a terminal state.
pub fn finish_analysis(
    store: &ScienceStore,
    ticket: ScienceRunTicket,
    source_path: &Path,
    source_bytes: &[u8],
) -> crate::Result<SeqAnalyzeResult> {
    // The same guard the csv path uses: only an allowed, running run may
    // commit output. Without it a caller could finish a run it never got
    // permission for.
    let run = store.load_run(&ticket.run_id)?;
    if run.state != RunState::Running
        || store
            .approvals(&ticket.run_id)?
            .iter()
            .find(|approval| approval.call_id == ticket.call_id)
            .is_none_or(|approval| approval.decision != ApprovalDecision::Allow)
    {
        return Err(ScienceError::Invalid(
            "seq analysis output requires an allowed running run".into(),
        ));
    }

    let text = String::from_utf8_lossy(source_bytes);
    let records = match parse_fasta(&text) {
        Ok(records) => records,
        Err(error) => {
            // A malformed input is a FAILED run, not a silent error: the run
            // record must say what happened to the permission that was granted.
            let _ = store.transition(&ticket.run_id, RunState::Failed, Some(error.clone()));
            return Err(ScienceError::Invalid(error));
        }
    };
    let analysis = analyze(&records, source_bytes);
    let report = markdown_report(
        &analysis,
        source_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("input.fa"),
    );
    let analysis_json = serde_json::to_vec_pretty(&analysis)?;

    let analysis_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new("analysis.json"),
        &analysis_json,
        "application/json",
        "table",
    )?;
    let report_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id,
        Path::new("report.md"),
        report.as_bytes(),
        "text/markdown",
        "document",
    )?;

    let tool_identity = format!("{TOOL} {TOOL_VERSION} inside SessionActor");
    store.add_provenance(Provenance {
        run_id: ticket.run_id.clone(),
        source_uri: format!("file://{}", source_path.display()),
        source_commit: None,
        source_path: Some(source_path.display().to_string()),
        license: "caller-supplied input".into(),
        retrieved_at: Utc::now(),
        input_sha256: hex_sha256(source_bytes),
        tool: tool_identity.clone(),
        environment: BTreeMap::from([
            ("algorithm".into(), "seqbench-v3".into()),
            (
                "algorithm_source_repository".into(),
                MOTIF_REPOSITORY.into(),
            ),
            ("algorithm_source_commit".into(), MOTIF_COMMIT.into()),
            ("algorithm_source_license".into(), MOTIF_LICENSE.into()),
            ("authority".into(), "SessionActor".into()),
            ("network".into(), "disabled".into()),
        ]),
    })?;
    store.add_evidence(Evidence {
        run_id: ticket.run_id.clone(),
        claim: format!(
            "analyzed {} sequence record(s) with {TOOL} {TOOL_VERSION}",
            analysis.records.len()
        ),
        source: source_path.display().to_string(),
        artifact_sha256: Some(analysis_artifact.sha256.clone()),
        verified_at: Utc::now(),
    })?;
    store.append_event(
        &ticket.run_id,
        "SessionActor",
        "analysis.completed",
        serde_json::json!({
            "tool": tool_identity,
            "records": analysis.records.len(),
            "artifacts": [
                analysis_artifact.sha256,
                report_artifact.sha256,
            ],
        }),
    )?;
    let run = store.transition(&ticket.run_id, RunState::Succeeded, None)?;
    store.append_event(
        &ticket.run_id,
        "HostVerification",
        "run.succeeded",
        serde_json::json!({}),
    )?;
    let events = store.events_after(&ticket.run_id, 0, 1_000)?;

    Ok(SeqAnalyzeResult {
        records: analysis.records.len(),
        artifacts: store.artifacts(&ticket.run_id)?,
        evidence: store.evidence(&ticket.run_id)?,
        provenance: store.provenance(&ticket.run_id)?,
        approvals: store.approvals(&ticket.run_id)?,
        replay_after: events.last().map_or(0, |event| event.seq),
        run,
        analysis,
    })
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use crate::{ProjectId, RunId};

    const FASTA: &[u8] = b">seq1\nACGTACGT\n>seq2\nGAATTC\n";

    fn context(root: &Path, project: &str, owner: &str) -> RunContext {
        RunContext {
            run_id: RunId::new_v7(),
            project_id: ProjectId::new(project),
            session_id: "session-seq".into(),
            owner_id: owner.into(),
            workspace_root: root.to_path_buf(),
            provider: "offline-deterministic".into(),
            approval_policy: "production-session-permission".into(),
            tool_profile: "science-seqbench-v1".into(),
            artifact_root: root.join("science-store"),
            environment: BTreeMap::from([("network".into(), "disabled".into())]),
        }
    }

    #[test]
    fn begin_is_durable_and_finish_requires_allow() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let ticket = begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();

        let run = store.load_run(&ticket.run_id).unwrap();
        assert_eq!(run.state, RunState::AwaitingApproval);
        assert_eq!(
            store.approvals(&ticket.run_id).unwrap()[0].decision,
            ApprovalDecision::Pending
        );
        assert!(
            finish_analysis(&store, ticket.clone(), Path::new("input.fa"), FASTA).is_err(),
            "finish without Allow must fail"
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[test]
    fn non_allow_terminals_never_create_artifacts() {
        for (decision, state) in [
            (ApprovalDecision::Deny, RunState::Denied),
            (ApprovalDecision::Timeout, RunState::TimedOut),
            (ApprovalDecision::Cancel, RunState::Cancelled),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("science-store"));
            let ticket =
                begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
            crate::csv::finish_without_execution(
                &store,
                &ticket,
                decision,
                "focused protocol test",
            )
            .unwrap();

            assert_eq!(store.load_run(&ticket.run_id).unwrap().state, state);
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }

    #[test]
    fn allow_commits_only_store_owned_hashed_artifacts_and_audit_chain() {
        let temp = tempfile::tempdir().unwrap();
        let store_root = temp.path().join("science-store");
        let store = ScienceStore::new(&store_root);
        let ticket = begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();
        let result = finish_analysis(&store, ticket.clone(), Path::new("input.fa"), FASTA).unwrap();

        assert_eq!(result.run.state, RunState::Succeeded);
        assert_eq!(result.records, 2);
        assert_eq!(result.artifacts.len(), 2);
        assert_eq!(result.evidence.len(), 1);
        assert_eq!(result.provenance.len(), 1);
        assert_eq!(result.approvals[0].decision, ApprovalDecision::Allow);
        assert_eq!(
            result.evidence[0].artifact_sha256.as_deref(),
            Some(result.artifacts[0].sha256.as_str())
        );
        for artifact in &result.artifacts {
            let bytes = store
                .artifact_bytes(
                    &ticket.project_id,
                    &ticket.run_id,
                    &ticket.owner_id,
                    &artifact.relative_path,
                )
                .unwrap();
            assert_eq!(hex_sha256(&bytes), artifact.sha256);
        }
        assert!(
            !store_root.join("project-a").join("seqbench").exists(),
            "the legacy loose artifact path must not be written"
        );
    }

    #[test]
    fn owner_project_and_call_boundaries_fail_closed() {
        for mutate in ["owner", "project", "call"] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("science-store"));
            let mut ticket =
                begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
            crate::csv::mark_allowed(&store, &ticket).unwrap();
            match mutate {
                "owner" => ticket.owner_id = "mallory".into(),
                "project" => ticket.project_id = ProjectId::new("project-b"),
                "call" => ticket.call_id = CallId::new("forged-call"),
                _ => unreachable!(),
            }

            assert!(
                finish_analysis(&store, ticket.clone(), Path::new("input.fa"), FASTA).is_err(),
                "{mutate} boundary was bypassed"
            );
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }
}
