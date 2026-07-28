//! Offline Motif-class sequence analysis. Zero network, zero shell.
//! Deterministic — same bytes always produce the same report.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const TOOL: &str = "lumen-seqbench";
pub const TOOL_VERSION: &str = "1.5.0";
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

pub const SUPPORTED_TRANSLATION_TABLE_IDS: &[u8] = &[
    1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14, 15, 16, 21, 22, 23, 24, 25, 26, 29, 30, 32, 33,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SeqAnalyzeOptions {
    pub translation_table_id: u8,
    #[serde(default)]
    pub topology: SequenceTopology,
    #[serde(default)]
    pub restriction_digest_enzymes: Vec<String>,
}

impl Default for SeqAnalyzeOptions {
    fn default() -> Self {
        Self {
            translation_table_id: 1,
            topology: SequenceTopology::Linear,
            restriction_digest_enzymes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SequenceTopology {
    #[default]
    Linear,
    Circular,
}

impl SequenceTopology {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Circular => "circular",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TranslationTableSummary {
    pub id: u8,
    pub name: String,
}

#[derive(Debug, Clone, Copy)]
struct TranslationTable {
    id: u8,
    name: &'static str,
    starts: &'static [&'static str],
    stops: &'static [&'static str],
    overrides: &'static [(&'static str, char)],
}

pub fn is_supported_translation_table(id: u8) -> bool {
    translation_table(id).is_some()
}

pub fn translation_table_name(id: u8) -> Option<&'static str> {
    translation_table(id).map(|table| table.name)
}

// Direct data adaptation of Motif's representable NCBI translation-table
// registry in `src/bio/codon-tables.ts` at `MOTIF_COMMIT`. Tables 27, 28 and
// 31 remain deliberately absent because their context-dependent sense/stop
// codons cannot be represented by a single-valued codon map.
fn translation_table(id: u8) -> Option<TranslationTable> {
    let table = match id {
        1 => TranslationTable {
            id,
            name: "Standard",
            starts: &["TTG", "CTG", "ATG"],
            stops: &["TAA", "TAG", "TGA"],
            overrides: &[],
        },
        2 => TranslationTable {
            id,
            name: "Vertebrate Mitochondrial",
            starts: &["ATT", "ATC", "ATA", "ATG", "GTG"],
            stops: &["TAA", "TAG", "AGA", "AGG"],
            overrides: &[("TGA", 'W'), ("ATA", 'M'), ("AGA", '*'), ("AGG", '*')],
        },
        3 => TranslationTable {
            id,
            name: "Yeast Mitochondrial",
            starts: &["ATA", "ATG", "GTG"],
            stops: &["TAA", "TAG"],
            overrides: &[
                ("TGA", 'W'),
                ("CTT", 'T'),
                ("CTC", 'T'),
                ("CTA", 'T'),
                ("CTG", 'T'),
                ("ATA", 'M'),
            ],
        },
        4 => TranslationTable {
            id,
            name: "Mold, Protozoan, Coelenterate Mitochondrial / Mycoplasma",
            starts: &["TTA", "TTG", "CTG", "ATT", "ATC", "ATA", "ATG", "GTG"],
            stops: &["TAA", "TAG"],
            overrides: &[("TGA", 'W')],
        },
        5 => TranslationTable {
            id,
            name: "Invertebrate Mitochondrial",
            starts: &["ATT", "ATC", "ATA", "ATG", "GTG", "TTG"],
            stops: &["TAA", "TAG"],
            overrides: &[("TGA", 'W'), ("ATA", 'M'), ("AGA", 'S'), ("AGG", 'S')],
        },
        6 => TranslationTable {
            id,
            name: "Ciliate, Dasycladacean and Hexamita Nuclear",
            starts: &["ATG"],
            stops: &["TGA"],
            overrides: &[("TAA", 'Q'), ("TAG", 'Q')],
        },
        9 => TranslationTable {
            id,
            name: "Echinoderm and Flatworm Mitochondrial",
            starts: &["ATG", "GTG"],
            stops: &["TAA", "TAG"],
            overrides: &[("TGA", 'W'), ("AAA", 'N'), ("AGA", 'S'), ("AGG", 'S')],
        },
        10 => TranslationTable {
            id,
            name: "Euplotid Nuclear",
            starts: &["ATG"],
            stops: &["TAA", "TAG"],
            overrides: &[("TGA", 'C')],
        },
        11 => TranslationTable {
            id,
            name: "Bacterial, Archaeal and Plant Plastid",
            starts: &["ATG", "GTG", "TTG", "ATT", "CTG", "ATC", "ATA"],
            stops: &["TAA", "TAG", "TGA"],
            overrides: &[],
        },
        12 => TranslationTable {
            id,
            name: "Alternative Yeast Nuclear",
            starts: &["CTG", "ATG"],
            stops: &["TAA", "TAG", "TGA"],
            overrides: &[("CTG", 'S')],
        },
        13 => TranslationTable {
            id,
            name: "Ascidian Mitochondrial",
            starts: &["ATG", "GTG", "TTG", "ATA"],
            stops: &["TAA", "TAG"],
            overrides: &[("TGA", 'W'), ("ATA", 'M'), ("AGA", 'G'), ("AGG", 'G')],
        },
        14 => TranslationTable {
            id,
            name: "Alternative Flatworm Mitochondrial",
            starts: &["ATG"],
            stops: &["TAG"],
            overrides: &[
                ("TAA", 'Y'),
                ("TGA", 'W'),
                ("AAA", 'N'),
                ("AGA", 'S'),
                ("AGG", 'S'),
            ],
        },
        15 => TranslationTable {
            id,
            name: "Blepharisma Macronuclear",
            starts: &["ATG"],
            stops: &["TAA", "TGA"],
            overrides: &[("TAG", 'Q')],
        },
        16 => TranslationTable {
            id,
            name: "Chlorophycean Mitochondrial",
            starts: &["ATG"],
            stops: &["TAA", "TGA"],
            overrides: &[("TAG", 'L')],
        },
        21 => TranslationTable {
            id,
            name: "Trematode Mitochondrial",
            starts: &["ATG", "GTG"],
            stops: &["TAA", "TAG"],
            overrides: &[
                ("TGA", 'W'),
                ("ATA", 'M'),
                ("AAA", 'N'),
                ("AGA", 'S'),
                ("AGG", 'S'),
            ],
        },
        22 => TranslationTable {
            id,
            name: "Scenedesmus obliquus Mitochondrial",
            starts: &["ATG"],
            stops: &["TAA", "TCA", "TGA"],
            overrides: &[("TCA", '*'), ("TAG", 'L')],
        },
        23 => TranslationTable {
            id,
            name: "Thraustochytrium Mitochondrial",
            starts: &["ATT", "ATG", "GTG"],
            stops: &["TTA", "TAA", "TAG", "TGA"],
            overrides: &[("TTA", '*')],
        },
        24 => TranslationTable {
            id,
            name: "Rhabdopleuridae Mitochondrial",
            starts: &["TTG", "CTG", "ATG", "GTG"],
            stops: &["TAA", "TAG"],
            overrides: &[("TGA", 'W'), ("AGA", 'S'), ("AGG", 'K')],
        },
        25 => TranslationTable {
            id,
            name: "Candidate Division SR1 and Gracilibacteria",
            starts: &["TTG", "ATG", "GTG"],
            stops: &["TAA", "TAG"],
            overrides: &[("TGA", 'G')],
        },
        26 => TranslationTable {
            id,
            name: "Pachysolen tannophilus Nuclear",
            starts: &["CTG", "ATG"],
            stops: &["TAA", "TAG", "TGA"],
            overrides: &[("CTG", 'A')],
        },
        29 => TranslationTable {
            id,
            name: "Mesodinium Nuclear",
            starts: &["ATG"],
            stops: &["TGA"],
            overrides: &[("TAA", 'Y'), ("TAG", 'Y')],
        },
        30 => TranslationTable {
            id,
            name: "Peritrich Nuclear",
            starts: &["ATG"],
            stops: &["TGA"],
            overrides: &[("TAA", 'E'), ("TAG", 'E')],
        },
        32 => TranslationTable {
            id,
            name: "Balanophoraceae Plastid",
            starts: &["TTG", "CTG", "ATT", "ATC", "ATA", "ATG", "GTG"],
            stops: &["TAA", "TGA"],
            overrides: &[("TAG", 'W')],
        },
        33 => TranslationTable {
            id,
            name: "Cephalodiscidae Mitochondrial UAA-Tyr",
            starts: &["TTG", "CTG", "ATG", "GTG"],
            stops: &["TAG"],
            overrides: &[("TAA", 'Y'), ("TGA", 'W'), ("AGA", 'S'), ("AGG", 'K')],
        },
        _ => return None,
    };
    Some(table)
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
    pub recognition_sequence: String,
    pub position: usize,
    pub cut_position: isize,
    pub overhang: String,
    pub strand: i8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RestrictionDigestFragment {
    pub sequence: String,
    pub length: usize,
    pub start_in_original: usize,
    pub end_in_original: usize,
    pub left_enzyme: Option<String>,
    pub right_enzyme: Option<String>,
    pub overhang5: String,
    pub overhang3: String,
    pub overhang5_type: String,
    pub overhang3_type: String,
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub restriction_hits_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restriction_digest_fragments: Option<Vec<RestrictionDigestFragment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Analysis {
    pub schema_version: u32,
    pub tool: String,
    pub tool_version: String,
    pub source_sha256: String,
    pub algorithm_sources: Vec<AlgorithmSource>,
    pub translation_table: TranslationTableSummary,
    pub restriction_topology: SequenceTopology,
    pub restriction_enzyme_count: usize,
    pub restriction_digest_enzymes: Vec<String>,
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
    analyze_with_options(records, source_bytes, &SeqAnalyzeOptions::default())
        .expect("the default translation table is always valid")
}

pub fn analyze_with_options(
    records: &[Record],
    source_bytes: &[u8],
    options: &SeqAnalyzeOptions,
) -> Result<Analysis, String> {
    let table = translation_table(options.translation_table_id).ok_or_else(|| {
        format!(
            "unsupported NCBI translation table {}; supported ids: {}",
            options.translation_table_id,
            SUPPORTED_TRANSLATION_TABLE_IDS
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    })?;
    let digest_enzymes = canonical_restriction_digest_enzymes(&options.restriction_digest_enzymes)?;
    if digest_enzymes != options.restriction_digest_enzymes {
        return Err("restriction digest enzymes must use canonical names and catalog order".into());
    }
    let source_sha256 = hex_sha256(source_bytes);
    let mut summaries = Vec::with_capacity(records.len());
    for r in records {
        summaries.push(summarize(r, table, options.topology, &digest_enzymes)?);
    }
    Ok(Analysis {
        schema_version: 6,
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
                "src/bio/restriction-sites.ts".into(),
                "src/bio/restriction-digest.ts".into(),
            ],
        }],
        translation_table: TranslationTableSummary {
            id: table.id,
            name: table.name.into(),
        },
        restriction_topology: options.topology,
        restriction_enzyme_count: RESTRICTION_ENZYMES.len(),
        restriction_digest_enzymes: digest_enzymes.clone(),
        records: summaries,
        notes: vec![
            "Deterministic offline analysis. Not a substitute for wet-lab validation.".into(),
            format!(
                "Restriction sites use the {} topology and Motif's 30-enzyme default panel; hits are bounded recognition-pattern predictions only.",
                options.topology.as_str()
            ),
            if digest_enzymes.is_empty() {
                "Restriction digest was not requested.".into()
            } else {
                format!(
                    "Restriction digest uses {} selected enzyme(s): {}.",
                    digest_enzymes.len(),
                    digest_enzymes.join(", ")
                )
            },
            format!(
                "Translation and ORFs use NCBI translation table {} ({}); ORF min length 30 aa.",
                table.id, table.name
            ),
            format!(
                "FASTA and sequence metrics are adapted from Motif {MOTIF_COMMIT} ({MOTIF_LICENSE})."
            ),
        ],
    })
}

pub fn markdown_report(a: &Analysis, source_label: &str) -> String {
    let mut b = String::new();
    b.push_str("# Sequence analysis report\n\n");
    b.push_str(&format!("- tool: `{}` {}\n", a.tool, a.tool_version));
    b.push_str(&format!("- source: `{source_label}`\n"));
    b.push_str(&format!("- source_sha256: `{}`\n", a.source_sha256));
    b.push_str(&format!(
        "- NCBI translation table: {} ({})\n",
        a.translation_table.id, a.translation_table.name
    ));
    b.push_str(&format!(
        "- restriction scan: {} topology; {} enzymes\n",
        a.restriction_topology.as_str(),
        a.restriction_enzyme_count
    ));
    if !a.restriction_digest_enzymes.is_empty() {
        b.push_str(&format!(
            "- restriction digest enzymes: {}\n",
            a.restriction_digest_enzymes.join(", ")
        ));
    }
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
            b.push_str(
                "| enzyme | recognition | match | cut | strand | overhang |\n\
                 |---|---|---:|---:|---:|---|\n",
            );
            for h in &r.restriction_hits {
                b.push_str(&format!(
                    "| {} | `{}` | {} | {} | {} | {} |\n",
                    h.enzyme,
                    h.recognition_sequence,
                    h.position,
                    h.cut_position,
                    h.strand,
                    h.overhang
                ));
            }
            if r.restriction_hits_truncated {
                b.push_str("\n_Result truncated at the Lumen 100-hit safety cap._\n");
            }
            b.push('\n');
        }
        if let Some(fragments) = &r.restriction_digest_fragments {
            b.push_str("### Restriction digest fragments\n\n");
            b.push_str(
                "| fragment | length | source span | left | right | 5′ end | 3′ end |\n\
                 |---:|---:|---|---|---|---|---|\n",
            );
            for (index, fragment) in fragments.iter().enumerate() {
                b.push_str(&format!(
                    "| {} | {} | {}..{} | {} | {} | `{}` ({}) | `{}` ({}) |\n",
                    index + 1,
                    fragment.length,
                    fragment.start_in_original,
                    fragment.end_in_original,
                    fragment.left_enzyme.as_deref().unwrap_or("—"),
                    fragment.right_enzyme.as_deref().unwrap_or("—"),
                    fragment.overhang5,
                    fragment.overhang5_type,
                    fragment.overhang3,
                    fragment.overhang3_type
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

fn summarize(
    r: &Record,
    table: TranslationTable,
    restriction_topology: SequenceTopology,
    digest_enzymes: &[String],
) -> Result<RecordSummary, String> {
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
        restriction_hits_truncated: false,
        restriction_digest_fragments: None,
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
            let pep = translate_with_table(&r.sequence, frame, table);
            let short = if pep.len() > 80 {
                format!("{}…", &pep[..80])
            } else {
                pep
            };
            s.translation_frames.insert(format!("+{frame}"), short);
            let rc = reverse_complement(&r.sequence, rna);
            let pep_n = translate_with_table(&rc, frame, table);
            let short_n = if pep_n.len() > 80 {
                format!("{}…", &pep_n[..80])
            } else {
                pep_n
            };
            s.translation_frames.insert(format!("-{frame}"), short_n);
        }
        s.orfs = find_orfs_with_table(&r.sequence, 30, table);
        if r.kind == "dna" {
            let (hits, truncated) = find_restriction_sites(&r.sequence, restriction_topology);
            s.restriction_hits = hits;
            s.restriction_hits_truncated = truncated;
            if !digest_enzymes.is_empty() {
                s.restriction_digest_fragments = Some(restriction_digest(
                    &r.sequence,
                    restriction_topology,
                    digest_enzymes,
                )?);
            }
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
    Ok(s)
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
#[cfg(test)]
fn translate(seq: &str, frame: usize) -> String {
    translate_with_table(
        seq,
        frame,
        translation_table(1).expect("standard translation table"),
    )
}

fn translate_with_table(seq: &str, frame: usize, table: TranslationTable) -> String {
    if !(1..=3).contains(&frame) {
        return String::new();
    }
    let dna = to_dna(seq);
    let bytes = dna.as_bytes();
    let mut out = String::new();
    let mut i = frame - 1;
    while i + 3 <= bytes.len() {
        let codon = std::str::from_utf8(&bytes[i..i + 3]).unwrap_or("NNN");
        out.push(translate_codon(codon, table));
        i += 3;
    }
    out
}

fn translate_codon(codon: &str, table: TranslationTable) -> char {
    table
        .overrides
        .iter()
        .find_map(|(candidate, amino_acid)| (*candidate == codon).then_some(*amino_acid))
        .unwrap_or_else(|| genetic_code(codon))
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

// Direct Rust adaptation of Motif's `src/bio/orf-detection.ts` and the NCBI
// table-1 start/stop sets in `src/bio/codon-tables.ts` at `MOTIF_COMMIT`.
// Lumen keeps the same six-frame scan, nested-start behavior, terminal
// no-stop ORFs, reverse-strand coordinate mapping, and length-descending sort.
// The only product-specific addition is a 50-record output cap.
#[cfg(test)]
fn find_orfs(seq: &str, min_aa: usize) -> Vec<Orf> {
    find_orfs_with_table(
        seq,
        min_aa,
        translation_table(1).expect("standard translation table"),
    )
}

fn find_orfs_with_table(seq: &str, min_aa: usize, table: TranslationTable) -> Vec<Orf> {
    let dna = to_dna(seq);
    let mut out = Vec::new();
    let sequence_len = dna.len();
    if sequence_len < 3 {
        return out;
    }

    for frame_offset in 0..3 {
        out.extend(orfs_in_frame(&dna, frame_offset, 1, min_aa, table));
    }

    let reverse = reverse_complement(&dna, false);
    for frame_offset in 0..3 {
        for mut orf in orfs_in_frame(&reverse, frame_offset, -1, min_aa, table) {
            let reverse_start = orf.start;
            let reverse_end = orf.end;
            orf.start = sequence_len - reverse_end;
            orf.end = sequence_len - reverse_start;
            out.push(orf);
        }
    }

    out.sort_by_key(|orf| std::cmp::Reverse(orf.length_bp));
    out.truncate(50);
    out
}

fn orfs_in_frame(
    seq: &str,
    frame_offset: usize,
    strand: i8,
    min_aa: usize,
    table: TranslationTable,
) -> Vec<Orf> {
    let mut start_positions = Vec::new();
    let mut stop_positions = Vec::new();
    let mut position = frame_offset;
    while position + 2 < seq.len() {
        let codon = &seq[position..position + 3];
        if table.starts.contains(&codon) {
            start_positions.push(position);
        }
        if table.stops.contains(&codon) {
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
            protein: translate_with_table(&seq[start..translated_end], 1, table),
            truncated,
        });
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct RestrictionEnzyme {
    name: &'static str,
    recognition_sequence: &'static str,
    cut_offset: isize,
    complement_cut_offset: isize,
    overhang: &'static str,
}

// Direct data and algorithm adaptation of Motif's default 30-enzyme panel and
// `findRestrictionSites` scanner in `src/bio/restriction-sites.ts` at
// `MOTIF_COMMIT`. Lumen keeps a bounded output and does not admit Motif's UI or
// runtime as an execution authority.
const RESTRICTION_ENZYMES: &[RestrictionEnzyme] = &[
    RestrictionEnzyme {
        name: "EcoRI",
        recognition_sequence: "GAATTC",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "BamHI",
        recognition_sequence: "GGATCC",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "HindIII",
        recognition_sequence: "AAGCTT",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "XbaI",
        recognition_sequence: "TCTAGA",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "SalI",
        recognition_sequence: "GTCGAC",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "PstI",
        recognition_sequence: "CTGCAG",
        cut_offset: 5,
        complement_cut_offset: 1,
        overhang: "3prime",
    },
    RestrictionEnzyme {
        name: "NotI",
        recognition_sequence: "GCGGCCGC",
        cut_offset: 2,
        complement_cut_offset: 6,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "XhoI",
        recognition_sequence: "CTCGAG",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "NcoI",
        recognition_sequence: "CCATGG",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "NdeI",
        recognition_sequence: "CATATG",
        cut_offset: 2,
        complement_cut_offset: 4,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "SpeI",
        recognition_sequence: "ACTAGT",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "KpnI",
        recognition_sequence: "GGTACC",
        cut_offset: 5,
        complement_cut_offset: 1,
        overhang: "3prime",
    },
    RestrictionEnzyme {
        name: "SacI",
        recognition_sequence: "GAGCTC",
        cut_offset: 5,
        complement_cut_offset: 1,
        overhang: "3prime",
    },
    RestrictionEnzyme {
        name: "SmaI",
        recognition_sequence: "CCCGGG",
        cut_offset: 3,
        complement_cut_offset: 3,
        overhang: "blunt",
    },
    RestrictionEnzyme {
        name: "BglII",
        recognition_sequence: "AGATCT",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "ClaI",
        recognition_sequence: "ATCGAT",
        cut_offset: 2,
        complement_cut_offset: 4,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "EcoRV",
        recognition_sequence: "GATATC",
        cut_offset: 3,
        complement_cut_offset: 3,
        overhang: "blunt",
    },
    RestrictionEnzyme {
        name: "AgeI",
        recognition_sequence: "ACCGGT",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "NheI",
        recognition_sequence: "GCTAGC",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "MluI",
        recognition_sequence: "ACGCGT",
        cut_offset: 1,
        complement_cut_offset: 5,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "BsaI",
        recognition_sequence: "GGTCTC",
        cut_offset: 7,
        complement_cut_offset: 11,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "BbsI",
        recognition_sequence: "GAAGAC",
        cut_offset: 8,
        complement_cut_offset: 12,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "ScaI",
        recognition_sequence: "AGTACT",
        cut_offset: 3,
        complement_cut_offset: 3,
        overhang: "blunt",
    },
    RestrictionEnzyme {
        name: "ApaI",
        recognition_sequence: "GGGCCC",
        cut_offset: 5,
        complement_cut_offset: 1,
        overhang: "3prime",
    },
    RestrictionEnzyme {
        name: "SphI",
        recognition_sequence: "GCATGC",
        cut_offset: 5,
        complement_cut_offset: 1,
        overhang: "3prime",
    },
    RestrictionEnzyme {
        name: "AluI",
        recognition_sequence: "AGCT",
        cut_offset: 2,
        complement_cut_offset: 2,
        overhang: "blunt",
    },
    RestrictionEnzyme {
        name: "HaeIII",
        recognition_sequence: "GGCC",
        cut_offset: 2,
        complement_cut_offset: 2,
        overhang: "blunt",
    },
    RestrictionEnzyme {
        name: "TaqI",
        recognition_sequence: "TCGA",
        cut_offset: 1,
        complement_cut_offset: 3,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "HpaII",
        recognition_sequence: "CCGG",
        cut_offset: 1,
        complement_cut_offset: 3,
        overhang: "5prime",
    },
    RestrictionEnzyme {
        name: "MspI",
        recognition_sequence: "CCGG",
        cut_offset: 1,
        complement_cut_offset: 3,
        overhang: "5prime",
    },
];

const MAX_RESTRICTION_HITS: usize = 100;
pub const MAX_RESTRICTION_DIGEST_ENZYMES: usize = 8;
const MAX_RESTRICTION_DIGEST_SEQUENCE_BYTES: usize = 1024 * 1024;

pub fn canonical_restriction_digest_enzymes(names: &[String]) -> Result<Vec<String>, String> {
    if names.len() > MAX_RESTRICTION_DIGEST_ENZYMES {
        return Err(format!(
            "restriction digest accepts at most {MAX_RESTRICTION_DIGEST_ENZYMES} enzymes"
        ));
    }
    let mut selected = BTreeSet::new();
    for requested in names {
        if requested.trim() != requested || requested.is_empty() {
            return Err("restriction digest enzyme names must be non-empty and unpadded".into());
        }
        let enzyme = RESTRICTION_ENZYMES
            .iter()
            .find(|enzyme| enzyme.name.eq_ignore_ascii_case(requested))
            .ok_or_else(|| format!("unsupported restriction digest enzyme {requested}"))?;
        if !selected.insert(enzyme.name) {
            return Err(format!(
                "duplicate restriction digest enzyme {}",
                enzyme.name
            ));
        }
    }
    Ok(RESTRICTION_ENZYMES
        .iter()
        .filter(|enzyme| selected.contains(enzyme.name))
        .map(|enzyme| enzyme.name.to_string())
        .collect())
}

fn iupac_recognition_matches(
    dna: &[u8],
    recognition: &[u8],
    position: usize,
    topology: SequenceTopology,
) -> bool {
    if dna.is_empty() {
        return false;
    }
    let virtual_len = match topology {
        SequenceTopology::Linear => dna.len(),
        SequenceTopology::Circular if recognition.len().saturating_sub(1) < dna.len() => {
            dna.len() + recognition.len().saturating_sub(1)
        }
        SequenceTopology::Circular => dna.len().saturating_mul(2),
    };
    if position.saturating_add(recognition.len()) > virtual_len {
        return false;
    }
    recognition.iter().enumerate().all(|(offset, expected)| {
        let actual = dna[(position + offset) % dna.len()];
        match *expected {
            b'A' => actual == b'A',
            b'C' => actual == b'C',
            b'G' => actual == b'G',
            b'T' => actual == b'T',
            b'R' => matches!(actual, b'A' | b'G'),
            b'Y' => matches!(actual, b'C' | b'T'),
            b'S' => matches!(actual, b'G' | b'C'),
            b'W' => matches!(actual, b'A' | b'T'),
            b'K' => matches!(actual, b'G' | b'T'),
            b'M' => matches!(actual, b'A' | b'C'),
            b'B' => matches!(actual, b'C' | b'G' | b'T'),
            b'D' => matches!(actual, b'A' | b'G' | b'T'),
            b'H' => matches!(actual, b'A' | b'C' | b'T'),
            b'V' => matches!(actual, b'A' | b'C' | b'G'),
            b'N' => matches!(actual, b'A' | b'C' | b'G' | b'T'),
            _ => false,
        }
    })
}

fn find_restriction_sites(seq: &str, topology: SequenceTopology) -> (Vec<RestrictionHit>, bool) {
    find_restriction_sites_with_enzymes(seq, topology, RESTRICTION_ENZYMES)
}

fn find_restriction_sites_with_enzymes(
    seq: &str,
    topology: SequenceTopology,
    enzymes: &[RestrictionEnzyme],
) -> (Vec<RestrictionHit>, bool) {
    let dna = to_dna(seq);
    let dna_bytes = dna.as_bytes();
    let mut hits = Vec::new();

    for enzyme in enzymes {
        let recognition = enzyme.recognition_sequence.as_bytes();
        let reverse = reverse_complement(enzyme.recognition_sequence, false);
        let palindrome = reverse == enzyme.recognition_sequence;
        let mut seen_positions = BTreeSet::new();

        for (pattern, strand) in [(recognition, 1_i8), (reverse.as_bytes(), -1_i8)] {
            if strand == -1 && palindrome {
                continue;
            }
            for position in 0..dna_bytes.len() {
                if !iupac_recognition_matches(dna_bytes, pattern, position, topology)
                    || !seen_positions.insert(position)
                {
                    continue;
                }
                if hits.len() == MAX_RESTRICTION_HITS {
                    hits.sort_by_key(|hit: &RestrictionHit| hit.position);
                    return (hits, true);
                }
                let raw_cut = if strand == 1 {
                    position as isize + enzyme.cut_offset
                } else {
                    position as isize + recognition.len() as isize - enzyme.complement_cut_offset
                };
                let cut_position = match topology {
                    SequenceTopology::Linear => raw_cut,
                    SequenceTopology::Circular => raw_cut.rem_euclid(dna_bytes.len() as isize),
                };
                hits.push(RestrictionHit {
                    enzyme: enzyme.name.into(),
                    recognition_sequence: enzyme.recognition_sequence.into(),
                    position,
                    cut_position,
                    overhang: enzyme.overhang.into(),
                    strand,
                });
            }
        }
    }
    hits.sort_by_key(|hit| hit.position);
    (hits, false)
}

#[derive(Debug, Clone)]
struct FragmentEnd {
    overhang: String,
    end_type: String,
}

fn blunt_end() -> FragmentEnd {
    FragmentEnd {
        overhang: String::new(),
        end_type: "blunt".into(),
    }
}

fn read_sense_overhang(seq: &str, start: isize, end: isize, topology: SequenceTopology) -> String {
    if end <= start || seq.is_empty() {
        return String::new();
    }
    match topology {
        SequenceTopology::Linear => {
            if start < 0 || end > seq.len() as isize {
                String::new()
            } else {
                seq[start as usize..end as usize].to_ascii_uppercase()
            }
        }
        SequenceTopology::Circular => {
            let bytes = seq.as_bytes();
            (start..end)
                .map(|position| {
                    bytes[position.rem_euclid(bytes.len() as isize) as usize].to_ascii_uppercase()
                        as char
                })
                .collect()
        }
    }
}

fn restriction_cut_ends(
    seq: &str,
    topology: SequenceTopology,
    hit: &RestrictionHit,
) -> (FragmentEnd, FragmentEnd) {
    let Some(enzyme) = RESTRICTION_ENZYMES
        .iter()
        .find(|enzyme| enzyme.name == hit.enzyme)
    else {
        return (blunt_end(), blunt_end());
    };
    if enzyme.overhang == "blunt" || enzyme.cut_offset == enzyme.complement_cut_offset {
        return (blunt_end(), blunt_end());
    }
    let gap = (enzyme.cut_offset - enzyme.complement_cut_offset).abs();
    let recognition_len = enzyme.recognition_sequence.len() as isize;
    let left_cut = if hit.strand == -1 {
        hit.position as isize + recognition_len
            - enzyme.cut_offset.max(enzyme.complement_cut_offset)
    } else {
        hit.position as isize + enzyme.cut_offset.min(enzyme.complement_cut_offset)
    };
    let sense_overhang = read_sense_overhang(seq, left_cut, left_cut + gap, topology);
    if sense_overhang.is_empty() {
        return (blunt_end(), blunt_end());
    }
    let sticky_overhang = if enzyme.overhang == "3prime" {
        reverse_complement(&sense_overhang, false)
    } else {
        sense_overhang
    };
    (
        FragmentEnd {
            overhang: sticky_overhang.clone(),
            end_type: enzyme.overhang.into(),
        },
        FragmentEnd {
            overhang: reverse_complement(&sticky_overhang, false),
            end_type: enzyme.overhang.into(),
        },
    )
}

fn restriction_digest(
    seq: &str,
    topology: SequenceTopology,
    enzyme_names: &[String],
) -> Result<Vec<RestrictionDigestFragment>, String> {
    if seq.len() > MAX_RESTRICTION_DIGEST_SEQUENCE_BYTES {
        return Err(format!(
            "restriction digest sequence exceeds {} byte cap",
            MAX_RESTRICTION_DIGEST_SEQUENCE_BYTES
        ));
    }
    let enzymes = RESTRICTION_ENZYMES
        .iter()
        .filter(|enzyme| enzyme_names.iter().any(|name| name == enzyme.name))
        .copied()
        .collect::<Vec<_>>();
    let (sites, truncated) = find_restriction_sites_with_enzymes(seq, topology, &enzymes);
    if truncated {
        return Err("restriction digest exceeds the 100-cut safety cap".into());
    }
    if sites.is_empty() {
        return Ok(vec![RestrictionDigestFragment {
            sequence: seq.into(),
            length: seq.len(),
            start_in_original: 0,
            end_in_original: seq.len(),
            left_enzyme: None,
            right_enzyme: None,
            overhang5: String::new(),
            overhang3: String::new(),
            overhang5_type: "blunt".into(),
            overhang3_type: "blunt".into(),
        }]);
    }

    let mut seen_cuts = BTreeSet::new();
    let mut cuts = Vec::new();
    for site in &sites {
        if seen_cuts.insert(site.cut_position) {
            cuts.push(site);
        }
    }
    cuts.sort_by_key(|site| site.cut_position);
    if topology == SequenceTopology::Linear
        && cuts
            .iter()
            .any(|site| !(0..=seq.len() as isize).contains(&site.cut_position))
    {
        return Err("linear restriction digest has a cut outside the sequence".into());
    }

    let mut fragments = Vec::new();
    match topology {
        SequenceTopology::Linear => {
            for index in 0..=cuts.len() {
                let start = if index == 0 {
                    0
                } else {
                    cuts[index - 1].cut_position as usize
                };
                let end = if index == cuts.len() {
                    seq.len()
                } else {
                    cuts[index].cut_position as usize
                };
                if end <= start {
                    continue;
                }
                let left_end = if index == 0 {
                    blunt_end()
                } else {
                    restriction_cut_ends(seq, topology, cuts[index - 1]).0
                };
                let right_end = if index == cuts.len() {
                    blunt_end()
                } else {
                    restriction_cut_ends(seq, topology, cuts[index]).1
                };
                fragments.push(RestrictionDigestFragment {
                    sequence: seq[start..end].into(),
                    length: end - start,
                    start_in_original: start,
                    end_in_original: end,
                    left_enzyme: (index > 0).then(|| cuts[index - 1].enzyme.clone()),
                    right_enzyme: (index < cuts.len()).then(|| cuts[index].enzyme.clone()),
                    overhang5: left_end.overhang,
                    overhang3: right_end.overhang,
                    overhang5_type: left_end.end_type,
                    overhang3_type: right_end.end_type,
                });
            }
        }
        SequenceTopology::Circular => {
            for index in 0..cuts.len() {
                let start = cuts[index].cut_position as usize;
                let next = (index + 1) % cuts.len();
                let end = cuts[next].cut_position as usize;
                let (sequence, length, end_in_original) = if end > start {
                    (seq[start..end].into(), end - start, end)
                } else {
                    (
                        format!("{}{}", &seq[start..], &seq[..end]),
                        seq.len() - start + end,
                        end + seq.len(),
                    )
                };
                if length == 0 {
                    continue;
                }
                let left_end = restriction_cut_ends(seq, topology, cuts[index]).0;
                let right_end = restriction_cut_ends(seq, topology, cuts[next]).1;
                fragments.push(RestrictionDigestFragment {
                    sequence,
                    length,
                    start_in_original: start,
                    end_in_original,
                    left_enzyme: Some(cuts[index].enzyme.clone()),
                    right_enzyme: Some(cuts[next].enzyme.clone()),
                    overhang5: left_end.overhang,
                    overhang3: right_end.overhang,
                    overhang5_type: left_end.end_type,
                    overhang3_type: right_end.end_type,
                });
            }
        }
    }
    Ok(fragments)
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

        assert_eq!(analysis.schema_version, 6);
        assert_eq!(analysis.tool_version, "1.5.0");
        assert_eq!(analysis.algorithm_sources[0].commit, MOTIF_COMMIT);
        assert_eq!(analysis.translation_table.id, 1);
        assert_eq!(analysis.translation_table.name, "Standard");
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

    #[test]
    fn motif_translation_table_registry_is_complete_and_internally_coherent() {
        use std::collections::BTreeSet;

        assert_eq!(
            SUPPORTED_TRANSLATION_TABLE_IDS,
            &[
                1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14, 15, 16, 21, 22, 23, 24, 25, 26, 29, 30,
                32, 33,
            ]
        );
        assert!(!is_supported_translation_table(27));
        assert!(!is_supported_translation_table(28));
        assert!(!is_supported_translation_table(31));

        let bases = ['T', 'C', 'A', 'G'];
        for id in SUPPORTED_TRANSLATION_TABLE_IDS {
            let table = translation_table(*id).unwrap();
            assert_eq!(table.id, *id);
            assert!(!table.name.is_empty());
            assert_eq!(
                table.starts.iter().copied().collect::<BTreeSet<_>>().len(),
                table.starts.len()
            );
            assert_eq!(
                table.stops.iter().copied().collect::<BTreeSet<_>>().len(),
                table.stops.len()
            );

            let mut encoded_stops = BTreeSet::new();
            let mut codon_count = 0;
            for first in bases {
                for second in bases {
                    for third in bases {
                        codon_count += 1;
                        let codon = format!("{first}{second}{third}");
                        if translate_codon(&codon, table) == '*' {
                            encoded_stops.insert(codon);
                        }
                    }
                }
            }
            assert_eq!(codon_count, 64);
            assert_eq!(
                encoded_stops,
                table
                    .stops
                    .iter()
                    .map(|codon| (*codon).to_string())
                    .collect()
            );
        }
    }

    #[test]
    fn motif_translation_tables_drive_translation_and_orf_stops() {
        let table_2 = translation_table(2).unwrap();
        let standard = translation_table(1).unwrap();
        let sequence = "ATGAAAAGATTTTAA";

        let standard_orf = find_orfs_with_table(sequence, 1, standard)
            .into_iter()
            .find(|orf| orf.strand == 1 && orf.start == 0)
            .unwrap();
        let mitochondrial_orf = find_orfs_with_table(sequence, 1, table_2)
            .into_iter()
            .find(|orf| orf.strand == 1 && orf.start == 0)
            .unwrap();
        assert_eq!(standard_orf.end, 15);
        assert_eq!(standard_orf.stop_codon, "TAA");
        assert_eq!(mitochondrial_orf.end, 9);
        assert_eq!(mitochondrial_orf.stop_codon, "AGA");
        assert_eq!(translate_with_table("ATATGAAGA", 1, table_2), "MW*");
        assert_eq!(
            translate_with_table("ATGTAGTGA", 1, translation_table(15).unwrap()),
            "MQ*"
        );
        assert_eq!(
            translate_with_table("ATGTAGTGA", 1, translation_table(32).unwrap()),
            "MW*"
        );
    }

    #[test]
    fn motif_restriction_panel_scans_reverse_type_iis_and_circular_origin() {
        assert_eq!(RESTRICTION_ENZYMES.len(), 30);
        let catalog_fingerprint = RESTRICTION_ENZYMES
            .iter()
            .map(|enzyme| {
                format!(
                    "{}|{}|{}|{}|{}\n",
                    enzyme.name,
                    enzyme.recognition_sequence,
                    enzyme.cut_offset,
                    enzyme.complement_cut_offset,
                    enzyme.overhang
                )
            })
            .collect::<String>();
        assert_eq!(
            hex_sha256(catalog_fingerprint.as_bytes()),
            "12598695d6a6476911608991c27370d95969dde13c08e219f941310a221d841a"
        );

        let (reverse_hits, reverse_truncated) =
            find_restriction_sites("AAAAAAGAGACCTTTTT", SequenceTopology::Linear);
        let bsai = reverse_hits
            .iter()
            .find(|hit| hit.enzyme == "BsaI")
            .expect("reverse BsaI site");
        assert_eq!(bsai.position, 6);
        assert_eq!(bsai.cut_position, 1);
        assert_eq!(bsai.strand, -1);
        assert_eq!(bsai.recognition_sequence, "GGTCTC");
        assert!(!reverse_truncated);

        let (linear_hits, _) = find_restriction_sites("AATTCCCCCG", SequenceTopology::Linear);
        assert!(!linear_hits.iter().any(|hit| hit.enzyme == "EcoRI"));
        let (circular_hits, circular_truncated) =
            find_restriction_sites("AATTCCCCCG", SequenceTopology::Circular);
        let eco_ri = circular_hits
            .iter()
            .find(|hit| hit.enzyme == "EcoRI")
            .expect("origin-spanning EcoRI site");
        assert_eq!(eco_ri.position, 9);
        assert_eq!(eco_ri.cut_position, 0);
        assert_eq!(eco_ri.strand, 1);
        assert!(!circular_truncated);
    }

    #[test]
    fn motif_restriction_iupac_matching_and_lumen_hit_cap_are_explicit() {
        assert!(iupac_recognition_matches(
            b"AGATCC",
            b"RGATCY",
            0,
            SequenceTopology::Linear
        ));
        assert!(!iupac_recognition_matches(
            b"NGATCC",
            b"RGATCY",
            0,
            SequenceTopology::Linear
        ));

        let repeated = "GAATTC".repeat(MAX_RESTRICTION_HITS + 1);
        let (hits, truncated) = find_restriction_sites(&repeated, SequenceTopology::Linear);
        assert_eq!(hits.len(), MAX_RESTRICTION_HITS);
        assert!(truncated);
    }

    #[test]
    fn motif_restriction_digest_preserves_linear_and_circular_fragment_ends() {
        let enzymes = vec!["EcoRI".to_string()];
        let linear =
            restriction_digest("AAAAGAATTCTTTT", SequenceTopology::Linear, &enzymes).unwrap();
        assert_eq!(linear.len(), 2);
        assert_eq!(linear[0].sequence, "AAAAG");
        assert_eq!(linear[0].right_enzyme.as_deref(), Some("EcoRI"));
        assert_eq!(linear[0].overhang3, "AATT");
        assert_eq!(linear[0].overhang3_type, "5prime");
        assert_eq!(linear[1].sequence, "AATTCTTTT");
        assert_eq!(linear[1].left_enzyme.as_deref(), Some("EcoRI"));
        assert_eq!(linear[1].overhang5, "AATT");
        assert_eq!(linear[1].overhang5_type, "5prime");

        let circular =
            restriction_digest("GAATTCTTTTAAAA", SequenceTopology::Circular, &enzymes).unwrap();
        assert_eq!(circular.len(), 1);
        assert_eq!(circular[0].sequence, "AATTCTTTTAAAAG");
        assert_eq!(circular[0].start_in_original, 1);
        assert_eq!(circular[0].end_in_original, 15);
        assert_eq!(circular[0].left_enzyme.as_deref(), Some("EcoRI"));
        assert_eq!(circular[0].right_enzyme.as_deref(), Some("EcoRI"));
        assert_eq!(circular[0].overhang5, "AATT");
        assert_eq!(circular[0].overhang3, "AATT");

        let bsai = vec!["BsaI".to_string()];
        let forward =
            restriction_digest("TTTTTGGTCTCACAGTGGGGGGGG", SequenceTopology::Linear, &bsai)
                .unwrap();
        assert_eq!(forward[0].sequence, "TTTTTGGTCTCA");
        assert_eq!(forward[0].overhang3, "ACTG");
        assert_eq!(forward[1].sequence, "CAGTGGGGGGGG");
        assert_eq!(forward[1].overhang5, "CAGT");

        let reverse =
            restriction_digest("AAAACCCCAGAGACCTTTTTTTT", SequenceTopology::Linear, &bsai).unwrap();
        assert_eq!(reverse[0].sequence, "AAAA");
        assert_eq!(reverse[0].overhang3, "GGGG");
        assert_eq!(reverse[1].sequence, "CCCCAGAGACCTTTTTTTT");
        assert_eq!(reverse[1].overhang5, "CCCC");
    }

    #[test]
    fn restriction_digest_selection_and_incomplete_cuts_fail_closed() {
        assert_eq!(
            canonical_restriction_digest_enzymes(&["BsaI".to_string(), "EcoRI".to_string()])
                .unwrap(),
            vec!["EcoRI".to_string(), "BsaI".to_string()]
        );
        assert!(
            canonical_restriction_digest_enzymes(&["EcoRI".to_string(), "ecori".to_string()])
                .is_err()
        );
        assert!(canonical_restriction_digest_enzymes(&["UnknownI".to_string()]).is_err());
        assert!(
            restriction_digest("GGTCTC", SequenceTopology::Linear, &["BsaI".to_string()])
                .unwrap_err()
                .contains("cut outside")
        );

        let repeated = "GAATTC".repeat(MAX_RESTRICTION_HITS + 1);
        assert!(
            restriction_digest(&repeated, SequenceTopology::Linear, &["EcoRI".to_string()])
                .unwrap_err()
                .contains("100-cut")
        );
    }

    #[test]
    fn unsupported_translation_table_fails_closed() {
        let raw = ">seq\nATGAAATAA\n";
        let records = parse_fasta(raw).unwrap();
        let error = analyze_with_options(
            &records,
            raw.as_bytes(),
            &SeqAnalyzeOptions {
                translation_table_id: 27,
                topology: SequenceTopology::Linear,
                restriction_digest_enzymes: Vec::new(),
            },
        )
        .unwrap_err();
        assert!(error.contains("unsupported NCBI translation table 27"));
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

const ANALYSIS_ARTIFACT_PATH: &str = "analysis.json";
const REPORT_ARTIFACT_PATH: &str = "report.md";

/// Phase one: create the durable run and its pending approval BEFORE the
/// permission manager is awaited, so every allow/deny/timeout/cancel has a
/// record to finish rather than vanishing.
pub fn begin_analysis(
    store: &ScienceStore,
    context: RunContext,
) -> crate::Result<ScienceRunTicket> {
    begin_analysis_with_options(store, context, &SeqAnalyzeOptions::default())
}

pub fn begin_analysis_with_options(
    store: &ScienceStore,
    mut context: RunContext,
    options: &SeqAnalyzeOptions,
) -> crate::Result<ScienceRunTicket> {
    let table = translation_table(options.translation_table_id).ok_or_else(|| {
        ScienceError::Invalid(format!(
            "unsupported NCBI translation table {}",
            options.translation_table_id
        ))
    })?;
    let digest_enzymes = canonical_restriction_digest_enzymes(&options.restriction_digest_enzymes)
        .map_err(ScienceError::Invalid)?;
    if digest_enzymes != options.restriction_digest_enzymes {
        return Err(ScienceError::Invalid(
            "restriction digest enzymes must use canonical names and catalog order".into(),
        ));
    }
    context.environment.insert(
        "translation_table_id".into(),
        options.translation_table_id.to_string(),
    );
    context
        .environment
        .insert("translation_table_name".into(), table.name.into());
    context.environment.insert(
        "restriction_topology".into(),
        options.topology.as_str().into(),
    );
    context.environment.insert(
        "restriction_digest_enzymes".into(),
        digest_enzymes.join(","),
    );
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
        serde_json::json!({
            "kind": "seq_analyze",
            "translation_table_id": table.id,
            "translation_table_name": table.name,
            "restriction_topology": options.topology,
            "restriction_digest_enzymes": digest_enzymes,
        }),
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
    finish_analysis_with_options(
        store,
        ticket,
        source_path,
        source_bytes,
        &SeqAnalyzeOptions::default(),
    )
}

pub fn finish_analysis_with_options(
    store: &ScienceStore,
    ticket: ScienceRunTicket,
    source_path: &Path,
    source_bytes: &[u8],
    options: &SeqAnalyzeOptions,
) -> crate::Result<SeqAnalyzeResult> {
    // Authorization is the first finish-side operation. Validation, parsing,
    // analysis and persistence are all execution and must not happen for a
    // forged ticket or before the durable Allow decision.
    let run = store.load_run(&ticket.run_id)?;
    if ticket.project_id != run.context.project_id
        || ticket.owner_id != run.context.owner_id
        || run.state != RunState::Running
        || store
            .approvals(&ticket.run_id)?
            .iter()
            .find(|approval| approval.call_id == ticket.call_id)
            .is_none_or(|approval| approval.decision != ApprovalDecision::Allow)
    {
        return Err(ScienceError::Invalid(
            "seq analysis requires an allowed running run".into(),
        ));
    }

    match finish_allowed_analysis(store, &ticket, source_path, source_bytes, options) {
        Ok(result) => Ok(result),
        Err(error) => {
            let detail = error.to_string();
            let cleanup = discard_failed_analysis_outputs(store, &ticket);
            let failure_detail = match &cleanup {
                Ok(()) => detail.clone(),
                Err(cleanup_error) => {
                    format!("{detail}; failed to discard partial outputs: {cleanup_error}")
                }
            };
            let _ = store.append_event(
                &ticket.run_id,
                "SessionActor",
                "analysis.failed",
                serde_json::json!({ "reason": failure_detail }),
            );
            let run = store.load_run(&ticket.run_id)?;
            if run.state != RunState::Failed {
                store
                    .transition(
                        &ticket.run_id,
                        RunState::Failed,
                        Some(failure_detail.clone()),
                    )
                    .map_err(|terminal_error| {
                        ScienceError::Invalid(format!(
                            "seq analysis failed ({error}) and its Failed terminal could not be persisted: {terminal_error}"
                        ))
                    })?;
            }
            cleanup.map_err(|cleanup_error| {
                ScienceError::Invalid(format!(
                    "seq analysis failed ({error}) and partial outputs could not be discarded: {cleanup_error}"
                ))
            })?;
            Err(error)
        }
    }
}

fn finish_allowed_analysis(
    store: &ScienceStore,
    ticket: &ScienceRunTicket,
    source_path: &Path,
    source_bytes: &[u8],
    options: &SeqAnalyzeOptions,
) -> crate::Result<SeqAnalyzeResult> {
    let table = translation_table(options.translation_table_id).ok_or_else(|| {
        ScienceError::Invalid(format!(
            "unsupported NCBI translation table {}",
            options.translation_table_id
        ))
    })?;
    let digest_enzymes = canonical_restriction_digest_enzymes(&options.restriction_digest_enzymes)
        .map_err(ScienceError::Invalid)?;
    if digest_enzymes != options.restriction_digest_enzymes {
        return Err(ScienceError::Invalid(
            "restriction digest enzymes must use canonical names and catalog order".into(),
        ));
    }
    let run = store.load_run(&ticket.run_id)?;
    if run.context.environment.get("translation_table_id")
        != Some(&options.translation_table_id.to_string())
        || run.context.environment.get("translation_table_name") != Some(&table.name.to_string())
        || run.context.environment.get("restriction_topology")
            != Some(&options.topology.as_str().to_string())
        || run.context.environment.get("restriction_digest_enzymes")
            != Some(&digest_enzymes.join(","))
    {
        return Err(ScienceError::Invalid(
            "seq analysis options do not match the durably approved run".into(),
        ));
    }

    let records = parse_analysis_input(source_path, source_bytes).map_err(ScienceError::Invalid)?;
    let analysis =
        analyze_with_options(&records, source_bytes, options).map_err(ScienceError::Invalid)?;
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
        Path::new(ANALYSIS_ARTIFACT_PATH),
        &analysis_json,
        "application/json",
        "table",
    )?;
    let report_artifact = store.put_artifact(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        ticket.call_id.clone(),
        Path::new(REPORT_ARTIFACT_PATH),
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
            ("algorithm".into(), "seqbench-v6".into()),
            (
                "algorithm_source_repository".into(),
                MOTIF_REPOSITORY.into(),
            ),
            ("algorithm_source_commit".into(), MOTIF_COMMIT.into()),
            ("algorithm_source_license".into(), MOTIF_LICENSE.into()),
            ("authority".into(), "SessionActor".into()),
            ("network".into(), "disabled".into()),
            (
                "translation_table_id".into(),
                options.translation_table_id.to_string(),
            ),
            ("translation_table_name".into(), table.name.into()),
            (
                "restriction_topology".into(),
                options.topology.as_str().into(),
            ),
            (
                "restriction_digest_enzymes".into(),
                digest_enzymes.join(","),
            ),
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
            "translation_table_id": table.id,
            "translation_table_name": table.name,
            "restriction_topology": options.topology,
            "restriction_digest_enzymes": digest_enzymes,
            "artifacts": [
                analysis_artifact.sha256,
                report_artifact.sha256,
            ],
        }),
    )?;
    // Gather the complete response before the terminal write. After Succeeded
    // is durable no fallible operation remains, so success cannot be returned
    // for a partially assembled result and a late error cannot strand Running.
    let artifacts = store.artifacts(&ticket.run_id)?;
    let evidence = store.evidence(&ticket.run_id)?;
    let provenance = store.provenance(&ticket.run_id)?;
    let approvals = store.approvals(&ticket.run_id)?;
    let events = store.events_after(&ticket.run_id, 0, 1_000)?;
    let replay_after = events.last().map_or(0, |event| event.seq);
    let records = analysis.records.len();
    let run = store.transition(&ticket.run_id, RunState::Succeeded, None)?;

    Ok(SeqAnalyzeResult {
        records,
        artifacts,
        evidence,
        provenance,
        approvals,
        replay_after,
        run,
        analysis,
    })
}

fn parse_analysis_input(source_path: &Path, source_bytes: &[u8]) -> Result<Vec<Record>, String> {
    let text = std::str::from_utf8(source_bytes)
        .map_err(|error| format!("sequence input is not valid UTF-8: {error}"))?;
    let is_fastq_path = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("fastq") || extension.eq_ignore_ascii_case("fq")
        });
    if is_fastq_path || text.trim_start().starts_with('@') {
        parse_fastq(text)
    } else {
        parse_fasta(text)
    }
}

fn parse_fastq(raw: &str) -> Result<Vec<Record>, String> {
    let lines: Vec<&str> = raw.lines().collect();
    if lines.is_empty() || !lines.len().is_multiple_of(4) {
        return Err("malformed FASTQ: expected complete four-line records".into());
    }
    let mut records = Vec::with_capacity(lines.len() / 4);
    for (index, record) in lines.chunks_exact(4).enumerate() {
        let header = record[0]
            .strip_prefix('@')
            .ok_or_else(|| format!("malformed FASTQ record {}: missing @ header", index + 1))?
            .trim();
        if header.is_empty() {
            return Err(format!(
                "malformed FASTQ record {}: empty header",
                index + 1
            ));
        }
        if !record[2].starts_with('+') {
            return Err(format!(
                "malformed FASTQ record {}: missing + separator",
                index + 1
            ));
        }
        if record[1].chars().count() != record[3].chars().count() {
            return Err(format!(
                "malformed FASTQ record {}: sequence and quality lengths differ",
                index + 1
            ));
        }
        let (sequence, gaps_removed) = normalize_seq_with_gaps(record[1]);
        if sequence.is_empty() {
            return Err(format!(
                "malformed FASTQ record {}: empty sequence",
                index + 1
            ));
        }
        let (id, description) =
            if let Some((offset, ch)) = header.char_indices().find(|(_, ch)| ch.is_whitespace()) {
                (
                    header[..offset].to_string(),
                    header[offset + ch.len_utf8()..].trim().to_string(),
                )
            } else {
                (header.to_string(), String::new())
            };
        records.push(Record {
            id,
            description,
            kind: detect_kind(&sequence),
            sequence,
            gaps_removed,
        });
    }
    Ok(records)
}

/// Remove bytes and registrations for artifacts this finish created before
/// failing. The Failed run and `analysis.failed` event remain the audit trace,
/// while artifact listing and byte service both see no successful output.
fn discard_failed_analysis_outputs(
    store: &ScienceStore,
    ticket: &ScienceRunTicket,
) -> crate::Result<()> {
    store.discard_running_outputs(
        &ticket.project_id,
        &ticket.run_id,
        &ticket.owner_id,
        &ticket.call_id,
        &[
            Path::new(ANALYSIS_ARTIFACT_PATH),
            Path::new(REPORT_ARTIFACT_PATH),
        ],
    )
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
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::AwaitingApproval
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
    fn malformed_fasta_and_fastq_fail_without_outputs() {
        for (source_path, source_bytes) in [
            (Path::new("malformed.fasta"), b">empty\n".as_slice()),
            (
                Path::new("malformed.fastq"),
                b"@seq\nACGT\n+\n!!!\n".as_slice(),
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let store = ScienceStore::new(temp.path().join("science-store"));
            let ticket =
                begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
            crate::csv::mark_allowed(&store, &ticket).unwrap();

            assert!(
                finish_analysis(&store, ticket.clone(), source_path, source_bytes,).is_err(),
                "{} was accepted",
                source_path.display()
            );
            assert_eq!(
                store.load_run(&ticket.run_id).unwrap().state,
                RunState::Failed
            );
            assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
            assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
            assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
        }
    }

    #[test]
    fn artifact_write_failure_fails_run_and_disables_partial_output_service() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let ticket = begin_analysis(&store, context(temp.path(), "project-a", "alice")).unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();

        // The first artifact is committed, then publishing report.md fails
        // because its target is a directory. This exercises the real
        // mid-commit failure boundary rather than a preflight error.
        let artifact_root = store
            .root()
            .join("runs")
            .join(&ticket.run_id.0)
            .join("artifacts");
        std::fs::create_dir(artifact_root.join(REPORT_ARTIFACT_PATH)).unwrap();

        assert!(finish_analysis(&store, ticket.clone(), Path::new("input.fa"), FASTA).is_err());
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(
            store
                .artifact_bytes(
                    &ticket.project_id,
                    &ticket.run_id,
                    &ticket.owner_id,
                    Path::new(ANALYSIS_ARTIFACT_PATH),
                )
                .is_err(),
            "a failed run's partial analysis artifact remained serviceable"
        );
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
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

    #[test]
    fn approved_translation_table_cannot_be_swapped_before_finish() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let approved = SeqAnalyzeOptions {
            translation_table_id: 2,
            topology: SequenceTopology::Linear,
            restriction_digest_enzymes: Vec::new(),
        };
        let ticket = begin_analysis_with_options(
            &store,
            context(temp.path(), "project-a", "alice"),
            &approved,
        )
        .unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();

        let swapped = SeqAnalyzeOptions {
            translation_table_id: 1,
            topology: SequenceTopology::Linear,
            restriction_digest_enzymes: Vec::new(),
        };
        let error = finish_analysis_with_options(
            &store,
            ticket.clone(),
            Path::new("input.fa"),
            FASTA,
            &swapped,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("options do not match the durably approved run")
        );
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[test]
    fn approved_restriction_topology_cannot_be_swapped_before_finish() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let approved = SeqAnalyzeOptions {
            translation_table_id: 1,
            topology: SequenceTopology::Circular,
            restriction_digest_enzymes: Vec::new(),
        };
        let ticket = begin_analysis_with_options(
            &store,
            context(temp.path(), "project-a", "alice"),
            &approved,
        )
        .unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();

        let swapped = SeqAnalyzeOptions {
            translation_table_id: 1,
            topology: SequenceTopology::Linear,
            restriction_digest_enzymes: Vec::new(),
        };
        let error = finish_analysis_with_options(
            &store,
            ticket.clone(),
            Path::new("input.fa"),
            FASTA,
            &swapped,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("options do not match the durably approved run")
        );
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[test]
    fn approved_restriction_digest_enzymes_cannot_be_swapped_before_finish() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let approved = SeqAnalyzeOptions {
            translation_table_id: 1,
            topology: SequenceTopology::Linear,
            restriction_digest_enzymes: vec!["EcoRI".into()],
        };
        let ticket = begin_analysis_with_options(
            &store,
            context(temp.path(), "project-a", "alice"),
            &approved,
        )
        .unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();

        let swapped = SeqAnalyzeOptions {
            translation_table_id: 1,
            topology: SequenceTopology::Linear,
            restriction_digest_enzymes: vec!["BamHI".into()],
        };
        let error = finish_analysis_with_options(
            &store,
            ticket.clone(),
            Path::new("input.fa"),
            FASTA,
            &swapped,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("options do not match the durably approved run")
        );
        assert_eq!(
            store.load_run(&ticket.run_id).unwrap().state,
            RunState::Failed
        );
        assert!(store.artifacts(&ticket.run_id).unwrap().is_empty());
        assert!(store.evidence(&ticket.run_id).unwrap().is_empty());
        assert!(store.provenance(&ticket.run_id).unwrap().is_empty());
    }

    #[test]
    fn allowed_translation_table_is_durable_in_output_and_provenance() {
        let temp = tempfile::tempdir().unwrap();
        let store = ScienceStore::new(temp.path().join("science-store"));
        let options = SeqAnalyzeOptions {
            translation_table_id: 2,
            topology: SequenceTopology::Circular,
            restriction_digest_enzymes: vec!["EcoRI".into()],
        };
        let ticket = begin_analysis_with_options(
            &store,
            context(temp.path(), "project-a", "alice"),
            &options,
        )
        .unwrap();
        crate::csv::mark_allowed(&store, &ticket).unwrap();
        let sequence = format!(">mitochondrial\nATG{}AGA\n", "AAA".repeat(29));
        let result = finish_analysis_with_options(
            &store,
            ticket,
            Path::new("input.fa"),
            sequence.as_bytes(),
            &options,
        )
        .unwrap();

        assert_eq!(result.analysis.translation_table.id, 2);
        assert_eq!(
            result.analysis.restriction_topology,
            SequenceTopology::Circular
        );
        assert_eq!(
            result.analysis.restriction_digest_enzymes,
            vec!["EcoRI".to_string()]
        );
        assert_eq!(result.analysis.records[0].orfs[0].stop_codon, "AGA");
        assert_eq!(
            result.provenance[0]
                .environment
                .get("translation_table_id")
                .map(String::as_str),
            Some("2")
        );
        assert_eq!(
            result.provenance[0]
                .environment
                .get("restriction_topology")
                .map(String::as_str),
            Some("circular")
        );
        assert_eq!(
            result.provenance[0]
                .environment
                .get("restriction_digest_enzymes")
                .map(String::as_str),
            Some("EcoRI")
        );
    }
}
