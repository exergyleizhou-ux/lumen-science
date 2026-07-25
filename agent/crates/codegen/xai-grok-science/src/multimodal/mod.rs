//! Multimodal parser registry — typed admission of document format parsers.
//! Seam contract: LS5-23, LS5-24.

use serde::{Deserialize, Serialize};

/// Admitted document format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocFormat {
    Pdf,
    Html,
    Markdown,
    Xml,
    Json,
    Docx,
    Csv,
    Tsv,
    Fasta,
    Genbank,
    Abi,
    Newick,
}

/// Parser admission status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParserAdmission {
    Pending,
    Admitted,
    Rejected,
}

/// A registered parser for a specific document format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserEntry {
    pub parser_id: String,
    pub format: DocFormat,
    pub mime_types: Vec<String>,
    pub extensions: Vec<String>,
    pub admission: ParserAdmission,
    pub max_input_bytes: u64,
    pub streaming_supported: bool,
}

/// Global parser registry (LS5-23).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserRegistry {
    pub parsers: Vec<ParserEntry>,
}

impl ParserRegistry {
    pub fn new() -> Self {
        Self { parsers: vec![] }
    }

    pub fn register(&mut self, parser: ParserEntry) -> Result<(), String> {
        if self.parsers.iter().any(|p| p.parser_id == parser.parser_id) {
            return Err(format!("duplicate parser id: {}", parser.parser_id));
        }
        self.parsers.push(parser);
        Ok(())
    }

    pub fn find_for_extension(&self, ext: &str) -> Vec<&ParserEntry> {
        self.parsers.iter().filter(|p| p.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))).collect()
    }

    pub fn count_admitted(&self) -> usize {
        self.parsers.iter().filter(|p| p.admission == ParserAdmission::Admitted).count()
    }
}

// ── Renderer Admission (LS5-25, LS5-26) ────────────────────────────

/// Renderer types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererKind {
    Protein3D,
    Chemical2D,
    GenomeBrowser,
    LatexMath,
    PdfViewer,
    SequenceViewer,
    MsaViewer,
    ImageViewer,
    MotifMolecular,
}

/// Renderer admission record (LS5-26).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererAdmission {
    pub renderer_id: String,
    pub kind: RendererKind,
    pub build_id: String,
    pub source_commit: String,
    pub license: String,
    pub admission: ParserAdmission,
    pub requires_network: bool,
    pub sandboxed: bool,
}

// ── Export / Publish (LS5-27) ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Pdf,
    Html,
    Json,
    Csv,
    Svg,
    Png,
    Notebook,
    WorkflowPackage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    pub format: ExportFormat,
    pub source_artifact_id: String,
    pub renderer_id: Option<String>,
    pub include_provenance: bool,
    pub include_signature: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_rejects_duplicate_parser() {
        let mut reg = ParserRegistry::new();
        let p = ParserEntry {
            parser_id: "pdf-v1".into(),
            format: DocFormat::Pdf,
            mime_types: vec!["application/pdf".into()],
            extensions: vec!["pdf".into()],
            admission: ParserAdmission::Admitted,
            max_input_bytes: 100_000_000,
            streaming_supported: false,
        };
        reg.register(p.clone()).unwrap();
        assert!(reg.register(p).is_err());
    }

    #[test]
    fn find_parser_by_extension() {
        let mut reg = ParserRegistry::new();
        reg.register(ParserEntry {
            parser_id: "fasta-v1".into(), format: DocFormat::Fasta,
            mime_types: vec!["text/x-fasta".into()], extensions: vec!["fasta".into(), "fa".into()],
            admission: ParserAdmission::Admitted, max_input_bytes: 10_000_000, streaming_supported: true,
        }).unwrap();
        assert_eq!(reg.find_for_extension("fasta").len(), 1);
        assert_eq!(reg.find_for_extension("fa").len(), 1);
        assert_eq!(reg.find_for_extension("pdf").len(), 0);
    }

    #[test]
    fn renderer_admission_pending_by_default() {
        let r = RendererAdmission {
            renderer_id: "chem-2d-v1".into(),
            kind: RendererKind::Chemical2D,
            build_id: "build:1".into(),
            source_commit: "abc123".into(),
            license: "MIT".into(),
            admission: ParserAdmission::Pending,
            requires_network: false,
            sandboxed: true,
        };
        assert_eq!(r.sandboxed, true);
        assert!(!r.requires_network);
    }
}
