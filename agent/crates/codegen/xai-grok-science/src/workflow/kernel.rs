//! Multi-kernel support and reproduction levels.
//! Seam contracts: LS5-19, LS5-21.

use serde::{Deserialize, Serialize};

// ── Kernel (LS5-19) ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KernelKind {
    Python,
    R,
    Julia,
}

/// Kernel admission record. A kernel must pass independent admission
/// before it can be used in any workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelAdmission {
    pub kernel_id: String,
    pub kind: KernelKind,
    pub exact_version: String,
    pub executable_hash: String,
    pub package_lock_hash: String,
    pub default_no_network: bool,
    pub process_isolation: bool,
    pub resource_cap: ResourceCap,
    pub artifact_only_io: bool,
    pub admission_status: AdmissionStatus,
    pub admitted_at: Option<String>,
    pub admitted_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceCap {
    pub max_memory_mb: u64,
    pub max_cpu_seconds: u64,
    pub max_output_bytes: u64,
    pub max_file_descriptors: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionStatus {
    Pending,
    Admitted,
    Rejected,
    Unavailable,
}

impl KernelAdmission {
    /// Check if this kernel is safe to execute.
    pub fn is_safe(&self) -> bool {
        self.admission_status == AdmissionStatus::Admitted
            && self.process_isolation
            && self.artifact_only_io
    }

    /// Verify kernel identity matches the admission record.
    pub fn verify_identity(&self, executable_hash: &str, package_lock_hash: &str) -> bool {
        self.executable_hash == executable_hash
            && self.package_lock_hash == package_lock_hash
    }
}

// ── Kernel Manifest ────────────────────────────────────────────────

/// A record of all admitted kernels for a project or session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelManifest {
    pub kernels: Vec<KernelAdmission>,
    pub default_python: Option<String>,
    pub default_r: Option<String>,
    pub default_julia: Option<String>,
}

impl KernelManifest {
    /// Find an admitted kernel by kind.
    pub fn find_admitted(&self, kind: KernelKind) -> Option<&KernelAdmission> {
        self.kernels
            .iter()
            .find(|k| k.kind == kind && k.is_safe())
    }
}

// ── Reproduction Levels (LS5-21) ───────────────────────────────────

/// Three levels of scientific reproduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReproductionLevel {
    /// R1: Replay only — replay events and existing artifacts.
    /// Fastest, no recomputation. Verifies event log integrity.
    R1ReplayOnly,
    /// R2: Deterministic rerun — same environment, fixed fixtures.
    /// Recomputes results and verifies they match.
    R2Deterministic,
    /// R3: Independent reproduction — new session/environment.
    /// From approved inputs only. Gold standard.
    R3Independent,
}

impl ReproductionLevel {
    pub fn requires_environment_match(&self) -> bool {
        matches!(self, ReproductionLevel::R2Deterministic | ReproductionLevel::R3Independent)
    }

    pub fn allows_live_providers(&self) -> bool {
        false // Never. Replay must use fixtures.
    }

    pub fn requires_independent_reviewer(&self) -> bool {
        matches!(self, ReproductionLevel::R3Independent)
    }
}

/// A reproduction attempt for a workflow or claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproductionAttempt {
    pub attempt_id: String,
    pub target_run_id: String,
    pub level: ReproductionLevel,
    pub environment_hash: String,
    pub outcome: ReproductionResult,
    pub deviations: Vec<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReproductionResult {
    Pending,
    Running,
    ExactMatch,
    AcceptableDeviation,
    Failed,
    EnvironmentMismatch,
    LiveProviderBlocked,
}

impl ReproductionAttempt {
    /// Whether the reproduction was successful.
    pub fn is_successful(&self) -> bool {
        matches!(
            self.outcome,
            ReproductionResult::ExactMatch | ReproductionResult::AcceptableDeviation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_kernel() -> KernelAdmission {
        KernelAdmission {
            kernel_id: "py-3.12".into(),
            kind: KernelKind::Python,
            exact_version: "3.12.0".into(),
            executable_hash: "sha256:py".into(),
            package_lock_hash: "sha256:pkg".into(),
            default_no_network: true,
            process_isolation: true,
            resource_cap: ResourceCap {
                max_memory_mb: 4096,
                max_cpu_seconds: 300,
                max_output_bytes: 10_485_760,
                max_file_descriptors: 64,
            },
            artifact_only_io: true,
            admission_status: AdmissionStatus::Admitted,
            admitted_at: Some("2026-07-25".into()),
            admitted_by: Some("admin".into()),
        }
    }

    #[test]
    fn admitted_kernel_is_safe() {
        let k = sample_kernel();
        assert!(k.is_safe());
    }

    #[test]
    fn pending_kernel_is_unsafe() {
        let mut k = sample_kernel();
        k.admission_status = AdmissionStatus::Pending;
        assert!(!k.is_safe());
    }

    #[test]
    fn kernel_identity_verification() {
        let k = sample_kernel();
        assert!(k.verify_identity("sha256:py", "sha256:pkg"));
        assert!(!k.verify_identity("wrong", "sha256:pkg"));
    }

    #[test]
    fn replay_never_allows_live_providers() {
        assert!(!ReproductionLevel::R1ReplayOnly.allows_live_providers());
        assert!(!ReproductionLevel::R2Deterministic.allows_live_providers());
        assert!(!ReproductionLevel::R3Independent.allows_live_providers());
    }

    #[test]
    fn r3_requires_independent_reviewer() {
        assert!(ReproductionLevel::R3Independent.requires_independent_reviewer());
        assert!(!ReproductionLevel::R1ReplayOnly.requires_independent_reviewer());
    }
}
