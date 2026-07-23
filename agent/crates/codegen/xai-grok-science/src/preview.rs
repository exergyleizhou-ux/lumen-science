//! Scientific file preview (v1). Seam contract: S2.
//!
//! Content-sniffed, size-capped structured previews for artifact records.
//! MIME is derived from bytes, never from the file name, so a mislabeled
//! artifact cannot be routed to the wrong renderer. v1 covers tabular text,
//! JSON, plain text, PNG/JPEG/SVG, and FASTA. PDF/DOCX/XLSX/PPTX/Notebook are
//! deliberately out of scope until their converter supply chain is audited.
//!
//! This module performs no I/O: callers read the artifact bytes through the
//! store and pass them here. Preview records carry provenance fields so the
//! artifact/evidence chain stays complete.

use crate::{CallId, RunId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Default hard cap for previewable content (50 MiB).
pub const DEFAULT_MAX_BYTES: u64 = 50 * 1024 * 1024;

/// Rows/lines sampled for stats. Larger inputs are marked `truncated`.
const SAMPLE_LIMIT: u64 = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewKind {
    Csv,
    Tsv,
    Json,
    Text,
    Png,
    Jpeg,
    Svg,
    Fasta,
    /// Valid bytes but not UTF-8 text or a known binary format.
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum PreviewStats {
    Tabular { rows: u64, columns: u64, ragged: bool },
    Text { lines: u64 },
    Image { width: Option<u32>, height: Option<u32> },
    Fasta {
        sequences: u64,
        total_residues: u64,
        min_len: u64,
        max_len: u64,
        warnings: Vec<String>,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preview {
    pub kind: PreviewKind,
    /// Sniffed MIME type, e.g. "text/csv", "image/png".
    pub mime: String,
    pub bytes: u64,
    /// True when stats were computed on a sample rather than the full input.
    pub truncated: bool,
    pub stats: PreviewStats,
}

/// Durable preview record bound to an artifact hash. The artifact/evidence
/// chain stays complete: every preview names its source artifact digest, the
/// tool identity that produced it, and when it was generated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PreviewRecord {
    pub run_id: RunId,
    pub call_id: CallId,
    pub relative_path: PathBuf,
    pub artifact_sha256: String,
    pub preview: Preview,
    pub generated_at: DateTime<Utc>,
    pub tool: String,
}

/// One-line human summary stored on the artifact record's `preview` field.
pub fn summarize(preview: &Preview) -> String {
    let truncated = if preview.truncated { ", truncated" } else { "" };
    match &preview.stats {
        PreviewStats::Tabular { rows, columns, ragged } => {
            let ragged = if *ragged { ", ragged" } else { "" };
            format!(
                "{}: {rows} rows x {columns} columns{ragged}{truncated}",
                preview.mime
            )
        }
        PreviewStats::Text { lines } => format!("{}: {lines} lines{truncated}", preview.mime),
        PreviewStats::Image { width, height } => match (width, height) {
            (Some(w), Some(h)) => format!("{}: {w}x{h}", preview.mime),
            _ => preview.mime.clone(),
        },
        PreviewStats::Fasta {
            sequences,
            total_residues,
            warnings,
            ..
        } => format!(
            "{}: {sequences} sequences, {total_residues} residues, {} warnings{truncated}",
            preview.mime,
            warnings.len()
        ),
        PreviewStats::None => format!("{}: {} bytes", preview.mime, preview.bytes),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewError {
    Oversized { bytes: u64, max: u64 },
    Malformed { kind: &'static str, detail: String },
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewError::Oversized { bytes, max } => {
                write!(f, "content too large for preview: {bytes} bytes (max {max})")
            }
            PreviewError::Malformed { kind, detail } => {
                write!(f, "malformed {kind}: {detail}")
            }
        }
    }
}

impl std::error::Error for PreviewError {}

/// Sniff a MIME type from content bytes. Never trusts file names.
pub fn sniff_mime(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return "image/png";
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return "image/jpeg";
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return "application/octet-stream";
    };
    let trimmed = text.trim_start();
    if trimmed.starts_with("<svg") || (trimmed.starts_with("<?xml") && trimmed.contains("<svg")) {
        return "image/svg+xml";
    }
    if trimmed.starts_with('>') {
        return "text/x-fasta";
    }
    if (trimmed.starts_with('{') || trimmed.starts_with('['))
        && serde_json::from_str::<serde_json::Value>(trimmed).is_ok()
    {
        return "application/json";
    }
    if looks_like_table(text, b'\t') {
        return "text/tab-separated-values";
    }
    if looks_like_table(text, b',') {
        return "text/csv";
    }
    "text/plain"
}

fn looks_like_table(text: &str, delimiter: u8) -> bool {
    let d = delimiter as char;
    let mut lines = text.lines().filter(|l| !l.trim().is_empty()).take(5);
    let Some(first) = lines.next() else { return false };
    // Ragged tables are still tables: require a delimiter on every sampled
    // line, not an identical column count. Raggedness is reported in stats.
    first.matches(d).count() >= 1 && lines.all(|l| l.matches(d).count() >= 1)
}

/// PNG IHDR dimensions (bytes 16..24 of a valid PNG).
fn png_dimensions(bytes: &[u8]) -> (Option<u32>, Option<u32>) {
    if bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        (Some(w), Some(h))
    } else {
        (None, None)
    }
}

/// Build a structured preview from artifact bytes. Fails closed on oversized
/// content; malformed content of a known kind is reported, not guessed at.
pub fn build_preview(bytes: &[u8], max_bytes: u64) -> Result<Preview, PreviewError> {
    let len = bytes.len() as u64;
    if len > max_bytes {
        return Err(PreviewError::Oversized { bytes: len, max: max_bytes });
    }
    let mime = sniff_mime(bytes);
    let truncated = len > SAMPLE_LIMIT * 200;
    let kinded = |kind: PreviewKind, stats: PreviewStats| Preview {
        kind,
        mime: mime.to_owned(),
        bytes: len,
        truncated,
        stats,
    };
    Ok(match mime {
        "image/png" => {
            let (width, height) = png_dimensions(bytes);
            kinded(PreviewKind::Png, PreviewStats::Image { width, height })
        }
        "image/jpeg" => kinded(
            PreviewKind::Jpeg,
            PreviewStats::Image { width: None, height: None },
        ),
        "image/svg+xml" => kinded(
            PreviewKind::Svg,
            PreviewStats::Image { width: None, height: None },
        ),
        "application/json" => kinded(PreviewKind::Json, PreviewStats::None),
        "text/x-fasta" => kinded(PreviewKind::Fasta, fasta_stats(bytes)?),
        "text/csv" => kinded(PreviewKind::Csv, tabular_stats(bytes, b',')),
        "text/tab-separated-values" => kinded(PreviewKind::Tsv, tabular_stats(bytes, b'\t')),
        "text/plain" => {
            let lines = bytes.iter().filter(|&&b| b == b'\n').count() as u64 + 1;
            kinded(PreviewKind::Text, PreviewStats::Text { lines })
        }
        _ => kinded(PreviewKind::Binary, PreviewStats::None),
    })
}

fn tabular_stats(bytes: &[u8], delimiter: u8) -> PreviewStats {
    let d = delimiter as char;
    let text = String::from_utf8_lossy(bytes);
    let mut rows = 0u64;
    let mut columns = 0u64;
    let mut ragged = false;
    for line in text.lines().filter(|l| !l.trim().is_empty()).take(SAMPLE_LIMIT as usize) {
        let cols = line.matches(d).count() as u64 + 1;
        if rows == 0 {
            columns = cols;
        } else if cols != columns {
            ragged = true;
        }
        rows += 1;
    }
    PreviewStats::Tabular { rows, columns, ragged }
}

/// FASTA statistics with scientific warnings. Sequence characters are not
/// alphabet-validated beyond control characters: nucleotide and protein
/// records share the format and belong to the caller's domain check.
fn fasta_stats(bytes: &[u8]) -> Result<PreviewStats, PreviewError> {
    let text = std::str::from_utf8(bytes).map_err(|_| PreviewError::Malformed {
        kind: "fasta",
        detail: "not valid UTF-8".to_owned(),
    })?;
    let mut sequences = 0u64;
    let mut total = 0u64;
    let mut min_len = u64::MAX;
    let mut max_len = 0u64;
    let mut current = 0u64;
    let mut warnings = Vec::new();
    let mut seen_content = false;
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if seen_content {
                if current == 0 {
                    warnings.push("empty sequence record".to_owned());
                }
                min_len = min_len.min(current);
                max_len = max_len.max(current);
                total += current;
            }
            if header.trim().is_empty() {
                warnings.push("empty FASTA header".to_owned());
            }
            sequences += 1;
            current = 0;
            seen_content = true;
        } else if seen_content {
            if line.chars().any(|c| c.is_control() && c != '\t') {
                // Control bytes in sequence data mean corruption: fail closed.
                return Err(PreviewError::Malformed {
                    kind: "fasta",
                    detail: "control character in sequence data".to_owned(),
                });
            }
            current += line.chars().filter(|c| !c.is_whitespace()).count() as u64;
        } else if !line.trim().is_empty() {
            return Err(PreviewError::Malformed {
                kind: "fasta",
                detail: "content before first header".to_owned(),
            });
        }
    }
    if seen_content {
        min_len = min_len.min(current);
        max_len = max_len.max(current);
        total += current;
    }
    if sequences == 0 {
        return Err(PreviewError::Malformed {
            kind: "fasta",
            detail: "no sequence records".to_owned(),
        });
    }
    warnings.dedup();
    Ok(PreviewStats::Fasta {
        sequences,
        total_residues: total,
        min_len,
        max_len,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview(bytes: &[u8]) -> Preview {
        build_preview(bytes, DEFAULT_MAX_BYTES).expect("preview")
    }

    #[test]
    fn sniffs_png_by_magic_not_name() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&[0, 0, 0, 13]);
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&640u32.to_be_bytes());
        png.extend_from_slice(&480u32.to_be_bytes());
        let p = preview(&png);
        assert_eq!(p.kind, PreviewKind::Png);
        assert_eq!(p.mime, "image/png");
        assert_eq!(
            p.stats,
            PreviewStats::Image { width: Some(640), height: Some(480) }
        );
    }

    #[test]
    fn csv_stats_and_ragged_detection() {
        let p = preview(b"a,b,c\n1,2,3\n4,5\n");
        assert_eq!(p.kind, PreviewKind::Csv);
        assert_eq!(
            p.stats,
            PreviewStats::Tabular { rows: 3, columns: 3, ragged: true }
        );
    }

    #[test]
    fn tsv_is_distinguished_from_csv() {
        let p = preview(b"a\tb\n1\t2\n");
        assert_eq!(p.kind, PreviewKind::Tsv);
        assert_eq!(
            p.stats,
            PreviewStats::Tabular { rows: 2, columns: 2, ragged: false }
        );
    }

    #[test]
    fn json_must_parse_to_be_json() {
        assert_eq!(preview(b"{\"a\": 1}").kind, PreviewKind::Json);
        assert_eq!(preview(b"{not json").kind, PreviewKind::Text);
    }

    #[test]
    fn non_utf8_is_binary_not_text() {
        let p = preview(&[0xff, 0xfe, 0x00, 0x01, 0x80]);
        assert_eq!(p.kind, PreviewKind::Binary);
        assert_eq!(p.mime, "application/octet-stream");
    }

    #[test]
    fn fasta_stats_and_warnings() {
        let p = preview(b">seq1\nACGT\nAC\n>seq2\nTT\n");
        assert_eq!(p.kind, PreviewKind::Fasta);
        match p.stats {
            PreviewStats::Fasta { sequences, total_residues, min_len, max_len, .. } => {
                assert_eq!((sequences, total_residues, min_len, max_len), (2, 8, 2, 6));
            }
            other => panic!("unexpected stats: {other:?}"),
        }
    }

    #[test]
    fn fasta_malformed_fails_closed() {
        // Control bytes in sequence data mean corruption: refuse the preview.
        assert!(matches!(
            build_preview(b">s\nAC\x01GT\n", DEFAULT_MAX_BYTES),
            Err(PreviewError::Malformed { kind: "fasta", .. })
        ));
        // Content before the first header is not recognized as FASTA at all.
        assert_eq!(preview(b"ACGT\n>s\nTT").kind, PreviewKind::Text);
    }

    #[test]
    fn oversized_content_is_rejected() {
        let big = vec![b'a'; 1024];
        assert!(matches!(
            build_preview(&big, 512),
            Err(PreviewError::Oversized { bytes: 1024, max: 512 })
        ));
    }

    #[test]
    fn utf8_boundary_multibyte_text() {
        let p = preview("中文科研记录\n第二行\n".as_bytes());
        assert_eq!(p.kind, PreviewKind::Text);
        assert_eq!(p.stats, PreviewStats::Text { lines: 3 });
    }

    #[test]
    fn svg_detected_as_image() {
        let p = preview(br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>"#);
        assert_eq!(p.kind, PreviewKind::Svg);
        assert_eq!(p.mime, "image/svg+xml");
    }
}
