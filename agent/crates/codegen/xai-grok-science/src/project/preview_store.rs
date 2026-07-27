//! WP-6/7/8 preview: multimodal, review/collaboration, remote compute plan.
use super::store::ProjectStore;
use crate::collaboration::ReviewPackage;
use crate::features::ScienceFeature;
use crate::multimodal::{FormatCategory, ParserAdmission, AdmissionStatus};
use serde::{Deserialize, Serialize};

use crate::project::model::{OwnerId, ProjectId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalIndex {
    pub project_id: ProjectId,
    pub parsers: Vec<ParserAdmission>,
    pub renderers: Vec<renderer::RendererRecord>,
    pub total_indexed: usize,
}

mod renderer {
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RendererRecord {
        pub renderer_id: String,
        pub mime_types: Vec<String>,
        pub admitted: bool,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborationRecord {
    pub project_id: ProjectId,
    pub collaborators: Vec<OwnerId>,
    pub pending_invitations: Vec<OwnerId>,
    pub review_packages: Vec<ReviewPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteComputePlan {
    pub cluster_id: String,
    pub hostname: String,
    pub scheduler: String,
    pub jobs: Vec<PlannedJob>,
    pub can_schedule: bool,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedJob {
    pub command: String,
    pub nodes: u32,
    pub cpus: u32,
    pub memory_gb: u32,
    pub gpu_count: u32,
    pub walltime_secs: u64,
}

impl ProjectStore {
    // ── WP-6: Multimodal ──────────────────────────────────────────
    pub fn multimodal_index(&self, project_id: &ProjectId) -> crate::Result<MultimodalIndex> {
        self.gates()
            .require(ScienceFeature::ReviewPackage)?; // multimodal gate maps to review infra
        let mut parsers = vec![];
        for ext in &["csv", "fasta", "json", "txt", "pdf"] {
            parsers.push(ParserAdmission {
                parser_id: format!("parser-{ext}"),
                mime_types: vec![],
                file_extensions: vec![ext.to_string()],
                category: FormatCategory::Text,
                max_file_size_bytes: 100 << 20,
                streaming_supported: false,
                admission_status: AdmissionStatus::Admitted,
                security_review_url: None,
            });
        }
        Ok(MultimodalIndex {
            project_id: project_id.clone(),
            parsers,
            renderers: vec![
                renderer::RendererRecord {
                    renderer_id: "motif".into(),
                    mime_types: vec!["application/x-motif".into()],
                    admitted: true,
                },
                renderer::RendererRecord {
                    renderer_id: "protein-3d".into(),
                    mime_types: vec!["chemical/x-pdb".into()],
                    admitted: true,
                },
            ],
            total_indexed: 0,
        })
    }

    // ── WP-7: Review + Collaboration ──────────────────────────────
    pub fn collaboration_invite(
        &self,
        project_id: &ProjectId,
        owner_id: &str,
        invitee: impl Into<String>,
    ) -> crate::Result<CollaborationRecord> {
        self.gates()
            .require(ScienceFeature::Collaboration)?;
        let project = self.load_project(project_id)?;
        if project.owner_id.0 != owner_id {
            return Err(crate::ScienceError::Ownership);
        }
        Ok(CollaborationRecord {
            project_id: project_id.clone(),
            collaborators: vec![project.owner_id.clone()],
            pending_invitations: vec![OwnerId(invitee.into())],
            review_packages: vec![],
        })
    }

    // ── WP-8: Remote compute plan (dry-run) ───────────────────────
    pub fn remote_compute_plan(
        &self,
        _project_id: &ProjectId,
        cluster_hostname: impl Into<String>,
    ) -> crate::Result<RemoteComputePlan> {
        self.gates()
            .require(ScienceFeature::RemoteCompute)?;
        Ok(RemoteComputePlan {
            cluster_id: "sim-cluster".into(),
            hostname: cluster_hostname.into(),
            scheduler: "slurm".into(),
            jobs: vec![PlannedJob {
                command: "lumen-science pipeline offline ...".into(),
                nodes: 1,
                cpus: 4,
                memory_gb: 8,
                gpu_count: 0,
                walltime_secs: 7200,
            }],
            can_schedule: false,
            notes: vec![
                "Dry-run plan only — no live HPC credentials available.".into(),
                "Require RemoteCompute feature gate + operator authorization for live scheduling."
                    .into(),
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn multimodal_and_collab() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "t", "q").unwrap();
        let idx = store.multimodal_index(&p.project_id).unwrap();
        assert!(!idx.parsers.is_empty());
        let collab = store
            .collaboration_invite(&p.project_id, "o", "c1")
            .unwrap();
        assert_eq!(collab.pending_invitations.len(), 1);
    }

    #[test]
    fn remote_plan_dry_run() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "t", "q").unwrap();
        let plan = store.remote_compute_plan(&p.project_id, "hpc.example.com").unwrap();
        assert!(!plan.can_schedule);
        assert!(plan.notes.iter().any(|n| n.contains("Dry-run")));
    }

    #[test]
    fn collaboration_ownership_fail() {
        let dir = tempdir().unwrap();
        let store = ProjectStore::new(dir.path());
        let p = store.create_project("o", "t", "q").unwrap();
        assert!(matches!(
            store.collaboration_invite(&p.project_id, "other", "c1"),
            Err(crate::ScienceError::Ownership)
        ));
    }
}
