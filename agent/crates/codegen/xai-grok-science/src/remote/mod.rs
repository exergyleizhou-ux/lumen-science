//! Remote compute scheduler and HPC job lifecycle types.
//! Seam contracts: LS5-37, LS5-38, LS5-39.

use serde::{Deserialize, Serialize};

/// HPC scheduler types supported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedulerKind {
    Slurm,
    Pbs,
    Lsf,
    Sge,
    Local,
    Custom(String),
}

/// An HPC cluster identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterIdentity {
    pub cluster_id: String,
    pub hostname: String,
    pub scheduler: SchedulerKind,
    pub host_key_sha256: String,
    pub egress_hosts: Vec<String>,
    pub max_jobs_per_user: u32,
    pub requires_2fa: bool,
}

/// Resources requested for a remote job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequest {
    pub nodes: u32,
    pub cpus_per_node: u32,
    pub memory_gb: u32,
    pub gpu_count: u32,
    pub walltime_seconds: u64,
    pub partition: Option<String>,
    pub account: Option<String>,
}

/// A remote compute job submitted to an HPC scheduler.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteJob {
    pub job_id: String,
    pub cluster_id: String,
    pub project_id: String,
    pub command: String,
    pub work_dir: String,
    pub resources: ResourceRequest,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub submitted_at: String,
    pub completed_at: Option<String>,
    pub output_artifact_sha256: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Timeout,
    Cancelled,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            JobStatus::Completed | JobStatus::Failed | JobStatus::Timeout | JobStatus::Cancelled
        )
    }
}

/// Data movement specification for remote↔local transfers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataMovementSpec {
    pub direction: DataDirection,
    pub source_path: String,
    pub dest_path: String,
    pub sha256_expected: Option<String>,
    pub max_bytes: u64,
    pub timeout_seconds: u64,
    pub operator_authorized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataDirection {
    Upload,
    Download,
}

impl DataMovementSpec {
    /// Verify that paths are within allowed boundaries.
    pub fn validate_boundaries(&self, allowed_prefixes: &[&str]) -> Result<(), String> {
        let path = match self.direction {
            DataDirection::Upload => &self.source_path,
            DataDirection::Download => &self.dest_path,
        };
        let allowed = allowed_prefixes.iter().any(|prefix| path.starts_with(prefix));
        if !allowed {
            return Err(format!("path '{}' is outside allowed boundaries", path));
        }
        if self.max_bytes == 0 {
            return Err("max_bytes must be greater than zero".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_terminal_states() {
        assert!(JobStatus::Completed.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Timeout.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
        assert!(!JobStatus::Pending.is_terminal());
    }

    #[test]
    fn data_movement_boundary_validation() {
        let spec = DataMovementSpec {
            direction: DataDirection::Upload,
            source_path: "/workspace/project/data.csv".into(),
            dest_path: "/remote/output/".into(),
            sha256_expected: None,
            max_bytes: 1_000_000,
            timeout_seconds: 300,
            operator_authorized: true,
        };
        assert!(spec.validate_boundaries(&["/workspace/"]).is_ok());
        assert!(spec.validate_boundaries(&["/restricted/"]).is_err());
    }

    #[test]
    fn data_movement_rejects_zero_max_bytes() {
        let spec = DataMovementSpec {
            direction: DataDirection::Download,
            source_path: "/remote/data.csv".into(),
            dest_path: "/workspace/".into(),
            sha256_expected: None,
            max_bytes: 0,
            timeout_seconds: 300,
            operator_authorized: false,
        };
        assert!(spec.validate_boundaries(&["/remote/"]).is_err());
    }
}
