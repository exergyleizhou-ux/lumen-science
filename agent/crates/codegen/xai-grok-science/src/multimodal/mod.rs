//! Multimodal parser registry and renderer admission types.
//! Seam contracts: LS5-23, LS5-24, LS5-25.

use serde::{Deserialize, Serialize};

/// File format categories for multimodal scientific data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormatCategory {
    Text,
    Tabular,
    Image,
    Sequence,
    Structure,
    Spectrum,
    Geospatial,
    Office,
    Archive,
    Unknown,
}

/// Parser admission record. Each parser must pass independent review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserAdmission {
    pub parser_id: String,
    pub mime_types: Vec<String>,
    pub file_extensions: Vec<String>,
    pub category: FormatCategory,
    pub max_file_size_bytes: u64,
    pub streaming_supported: bool,
    pub admission_status: AdmissionStatus,
    pub security_review_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionStatus {
    Pending,
    Admitted,
    Rejected,
    SecurityReviewRequired,
}

/// Renderer admission — each renderer must be independently reviewed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererAdmission {
    pub renderer_id: String,
    pub renderer_type: RendererType,
    pub source_url: String,
    pub exact_commit: String,
    pub license: String,
    pub category: FormatCategory,
    pub sandbox_required: bool,
    pub network_policy: RendererNetworkPolicy,
    pub admission_status: AdmissionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererType {
    HtmlEmbedded,
    WasmModule,
    NativeCommand,
    PythonScript,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RendererNetworkPolicy {
    None,
    Allowlisted(Vec<String>),
}

/// Registry of all admitted file parsers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserRegistry {
    pub parsers: Vec<ParserAdmission>,
    pub total_admitted: usize,
}

impl ParserRegistry {
    pub fn find_by_extension(&self, ext: &str) -> Option<&ParserAdmission> {
        self.parsers.iter().find(|p| {
            p.file_extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
                && p.admission_status == AdmissionStatus::Admitted
        })
    }

    pub fn find_by_mime(&self, mime: &str) -> Option<&ParserAdmission> {
        self.parsers.iter().find(|p| {
            p.mime_types.iter().any(|m| m.eq_ignore_ascii_case(mime))
                && p.admission_status == AdmissionStatus::Admitted
        })
    }
}

/// Registry of all admitted renderers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererRegistry {
    pub renderers: Vec<RendererAdmission>,
    pub total_admitted: usize,
}

impl RendererRegistry {
    pub fn find_by_category(&self, category: FormatCategory) -> Vec<&RendererAdmission> {
        self.renderers.iter()
            .filter(|r| r.category == category && r.admission_status == AdmissionStatus::Admitted)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry() -> ParserRegistry {
        ParserRegistry {
            parsers: vec![
                ParserAdmission {
                    parser_id: "csv-parser".into(),
                    mime_types: vec!["text/csv".into()],
                    file_extensions: vec!["csv".into(), "tsv".into()],
                    category: FormatCategory::Tabular,
                    max_file_size_bytes: 100_000_000,
                    streaming_supported: true,
                    admission_status: AdmissionStatus::Admitted,
                    security_review_url: None,
                },
                ParserAdmission {
                    parser_id: "fasta-parser".into(),
                    mime_types: vec!["text/x-fasta".into()],
                    file_extensions: vec!["fasta".into(), "fa".into()],
                    category: FormatCategory::Sequence,
                    max_file_size_bytes: 500_000_000,
                    streaming_supported: false,
                    admission_status: AdmissionStatus::Admitted,
                    security_review_url: None,
                },
            ],
            total_admitted: 2,
        }
    }

    #[test]
    fn parser_find_by_extension() {
        let reg = sample_registry();
        assert!(reg.find_by_extension("csv").is_some());
        assert!(reg.find_by_extension("CSV").is_some()); // case-insensitive
        assert!(reg.find_by_extension("xyz").is_none());
    }

    #[test]
    fn parser_find_by_mime() {
        let reg = sample_registry();
        assert!(reg.find_by_mime("text/csv").is_some());
        assert!(reg.find_by_mime("application/pdf").is_none());
    }

    #[test]
    fn renderer_find_by_category() {
        let reg = RendererRegistry {
            renderers: vec![RendererAdmission {
                renderer_id: "protein-3d".into(),
                renderer_type: RendererType::HtmlEmbedded,
                source_url: "https://molstar.org".into(),
                exact_commit: "abc123".into(),
                license: "MIT".into(),
                category: FormatCategory::Structure,
                sandbox_required: true,
                network_policy: RendererNetworkPolicy::None,
                admission_status: AdmissionStatus::Admitted,
            }],
            total_admitted: 1,
        };
        let found = reg.find_by_category(FormatCategory::Structure);
        assert_eq!(found.len(), 1);
        assert!(reg.find_by_category(FormatCategory::Text).is_empty());
    }
}
