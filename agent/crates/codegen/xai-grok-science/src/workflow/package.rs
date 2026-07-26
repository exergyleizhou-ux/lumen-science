//! Workflow package — self-contained reproducible export.
//! Seam contract: LS5-20.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A self-contained workflow package for sharing and reproduction.
/// Contains all inputs, artifacts, manifests, and evidence needed
/// to reproduce results. Never includes credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPackage {
    pub package_id: String,
    pub workflow_id: String,
    pub created_at: String,
    pub schema_version: u32,

    /// The workflow specification.
    pub workflow_json: String,

    /// Input artifact manifest with SHA-256 hashes.
    pub inputs_manifest: InputManifest,

    /// Output artifact manifest.
    pub artifacts_manifest: ArtifactManifest,

    /// Evidence graph snapshot.
    pub evidence_graph_json: Option<String>,

    /// Event log for replay.
    pub events_jsonl: String,

    /// License and notice files included.
    pub licenses: Vec<String>,

    /// Cryptographic signature over the package.
    pub signature: Option<String>,

    /// Environment identity at creation time.
    pub environment_hash: String,
}

impl WorkflowPackage {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Verify package integrity: shared artifacts must have matching hashes.
    /// New artifacts (output-only) are allowed — they were produced by the workflow.
    pub fn verify_integrity(&self) -> Result<(), String> {
        for (path, output_hash) in &self.artifacts_manifest.files {
            if let Some(input_hash) = self.inputs_manifest.files.get(path)
                && input_hash != output_hash {
                    return Err(format!(
                        "hash mismatch for '{}': input={} output={}",
                        path, input_hash, output_hash
                    ));
                }
            // Artifact only in outputs is valid — it was produced by the workflow.
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputManifest {
    pub files: BTreeMap<String, String>, // path → SHA-256
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub files: BTreeMap<String, String>, // path → SHA-256
    pub total_artifacts: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_integrity_verification() {
        let mut inputs = BTreeMap::new();
        inputs.insert("data.csv".into(), "sha:abc".into());
        let mut outputs = BTreeMap::new();
        outputs.insert("data.csv".into(), "sha:abc".into());

        let pkg = WorkflowPackage {
            package_id: "pkg-1".into(),
            workflow_id: "wf-1".into(),
            created_at: "2026-07-25".into(),
            schema_version: 1,
            workflow_json: "{}".into(),
            inputs_manifest: InputManifest { files: inputs, total_bytes: 100 },
            artifacts_manifest: ArtifactManifest { files: outputs, total_artifacts: 1 },
            evidence_graph_json: None,
            events_jsonl: "[]".into(),
            licenses: vec!["MIT".into()],
            signature: None,
            environment_hash: "env:1".into(),
        };

        assert!(pkg.verify_integrity().is_ok());
    }

    #[test]
    fn package_detects_hash_mismatch() {
        let mut inputs = BTreeMap::new();
        inputs.insert("data.csv".into(), "sha:abc".into());
        let mut outputs = BTreeMap::new();
        outputs.insert("data.csv".into(), "sha:xyz".into()); // mismatch

        let pkg = WorkflowPackage {
            package_id: "pkg-1".into(),
            workflow_id: "wf-1".into(),
            created_at: "2026-07-25".into(),
            schema_version: 1,
            workflow_json: "{}".into(),
            inputs_manifest: InputManifest { files: inputs, total_bytes: 100 },
            artifacts_manifest: ArtifactManifest { files: outputs, total_artifacts: 1 },
            evidence_graph_json: None,
            events_jsonl: "[]".into(),
            licenses: vec!["MIT".into()],
            signature: None,
            environment_hash: "env:1".into(),
        };

        assert!(pkg.verify_integrity().is_err());
    }

    #[test]
    fn package_allows_new_output_artifacts() {
        let mut inputs = BTreeMap::new();
        inputs.insert("data.csv".into(), "sha:abc".into());
        let mut outputs = BTreeMap::new();
        outputs.insert("data.csv".into(), "sha:abc".into());
        outputs.insert("result.json".into(), "sha:new".into()); // new, not in inputs

        let pkg = WorkflowPackage {
            package_id: "pkg-1".into(),
            workflow_id: "wf-1".into(),
            created_at: "2026-07-25".into(),
            schema_version: 1,
            workflow_json: "{}".into(),
            inputs_manifest: InputManifest { files: inputs, total_bytes: 100 },
            artifacts_manifest: ArtifactManifest { files: outputs, total_artifacts: 2 },
            evidence_graph_json: None,
            events_jsonl: "[]".into(),
            licenses: vec!["MIT".into()],
            signature: None,
            environment_hash: "env:1".into(),
        };

        assert!(pkg.verify_integrity().is_ok());
    }
}
