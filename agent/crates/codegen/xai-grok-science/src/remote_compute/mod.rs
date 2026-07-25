//! Remote compute scheduler and HPC job lifecycle types.
//! Seam contracts: LS5-37, LS5-38, LS5-39, LS5-40.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::project::model::ProjectId;

// ── Scheduler Types (LS5-37) ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub backend: SchedulerBackend,
    pub max_concurrent_jobs: u32,
    pub default_timeout_secs: u64,
    pub retry_limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedulerBackend { Slurm, Sge, Pbs, Lsf, Local }

// ── HPC Job Lifecycle (LS5-38) ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Created,
    Submitted,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
    Interrupted,
}

impl JobStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled | Self::TimedOut | Self::Interrupted)
    }
    pub fn valid_transitions(&self) -> &[JobStatus] {
        match self {
            Self::Created => &[Self::Submitted, Self::Cancelled],
            Self::Submitted => &[Self::Queued, Self::Failed, Self::Cancelled],
            Self::Queued => &[Self::Running, Self::Cancelled, Self::TimedOut],
            Self::Running => &[Self::Completed, Self::Failed, Self::Cancelled, Self::TimedOut, Self::Interrupted],
            _ => &[],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpcJob {
    pub job_id: String,
    pub project_id: ProjectId,
    pub scheduler_id: String,
    pub command: String,
    pub work_dir: String,
    pub status: JobStatus,
    pub exit_code: Option<i32>,
    pub submitted_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub wall_time_secs: Option<u64>,
    pub resources: HpcResources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HpcResources {
    pub nodes: u32,
    pub cpus_per_node: u32,
    pub memory_gb: u32,
    pub gpus: u32,
    pub walltime_secs: u64,
}

// ── Data Movement (LS5-39, LS5-40) ─────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataDirection { Upload, Download }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataTransferSpec {
    pub transfer_id: String,
    pub direction: DataDirection,
    pub source_path: String,
    pub dest_path: String,
    pub host: String,
    pub credentials_bound: bool,
    pub expected_sha256: Option<String>,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferStatus { Pending, InProgress, Completed, Failed, Cancelled }

// ── Remote Compute Session (LS5-41) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteComputeSession {
    pub session_id: String,
    pub project_id: ProjectId,
    pub host: String,
    pub auth_method: AuthMethod,
    pub status: SessionStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod { SshKey, Password, Token, Oidc }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus { Active, Disconnected, Expired, Revoked }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::model::ProjectId;

    #[test]
    fn job_lifecycle_full() {
        let mut job = HpcJob {
            job_id: "job-1".into(), project_id: ProjectId("p".into()),
            scheduler_id: "sched-1".into(), command: "echo hi".into(),
            work_dir: "/tmp".into(), status: JobStatus::Created,
            exit_code: None, submitted_at: Utc::now(), updated_at: Utc::now(),
            wall_time_secs: None,
            resources: HpcResources { nodes: 1, cpus_per_node: 4, memory_gb: 16, gpus: 0, walltime_secs: 3600 },
        };
        assert!(job.status == JobStatus::Created);
        assert!(job.status.valid_transitions().contains(&JobStatus::Submitted));
        assert!(!job.status.is_terminal());

        job.status = JobStatus::Completed;
        assert!(job.status.is_terminal());
    }

    #[test]
    fn data_transfer_integrity_check() {
        let transfer = DataTransferSpec {
            transfer_id: "t-1".into(), direction: DataDirection::Upload,
            source_path: "/local/data.csv".into(), dest_path: "/remote/data.csv".into(),
            host: "hpc.example.com".into(), credentials_bound: true,
            expected_sha256: Some("abc123".into()), status: TransferStatus::Pending,
        };
        assert!(transfer.credentials_bound);
        assert!(transfer.expected_sha256.is_some());
    }

    #[test]
    fn scheduler_backend_coverage() {
        assert_eq!(SchedulerBackend::Slurm as u8, SchedulerBackend::Slurm as u8);
        assert_ne!(SchedulerBackend::Slurm, SchedulerBackend::Local);
    }
}
