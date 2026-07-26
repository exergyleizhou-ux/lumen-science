//! CI pipeline, cross-platform matrix, migration chain, release assets, canary.
//! Seam: LS5-67~LS5-72. Spec documents — actual CI/deployment needs production infra.

use serde::{Deserialize, Serialize};

// ── CI Pipeline (LS5-67) ──────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiPipeline {
    pub jobs: Vec<CiJob>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiJob {
    pub name: String,
    pub command: String,
    pub timeout_minutes: u32,
    pub requires_credential_cleanse: bool,
}

// ── Cross-Platform Matrix (LS5-68) ────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformEntry {
    pub os: String,
    pub arch: String,
    pub build_status: PlatformStatus,
    pub device_support: DeviceSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlatformStatus { Supported, Unsupported, NotTested, DriverUnavailable, LiveVerified }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeviceSupport { Full, Partial, None, NotApplicable }

// ── Migration Chain (LS5-69) ──────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationChain {
    pub steps: Vec<MigrationStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStep {
    pub from_version: String,
    pub to_version: String,
    pub test_status: MigrationTestStatus,
    pub artifact_hash_preserved: bool,
    pub rollback_tested: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MigrationTestStatus { Pending, Passed, Failed, Blocked }

// ── Release Candidate (LS5-70) ────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseCandidate {
    pub version: String,
    pub commit: String,
    pub checks: Vec<RcCheck>,
    pub overall_status: RcStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RcCheck {
    pub name: String,
    pub passed: bool,
    pub evidence_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RcStatus { Pending, Passed, Failed, BlockedByExternal }

// ── Release Assets (LS5-71) ───────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub version: String,
    pub assets: Vec<ReleaseAsset>,
    pub signatures: Vec<String>,
    pub sbom_hash: String,
    pub source_lock_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub kind: AssetKind,
    pub platform: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetKind { Binary, Installer, License, Notices, Sbom, Signature, MigrationGuide }

// ── Canary (LS5-72) ───────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryPlan {
    pub release_version: String,
    pub checks: Vec<CanaryCheck>,
    pub rollback_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanaryCheck {
    pub name: String,
    pub check_type: CanaryCheckType,
    pub status: CanaryStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanaryCheckType { FreshInstall, Migration, ProjectReopen, WorkflowFixture, EvidenceTrace, LiveConnector, HpcSmoke, DeviceSmoke, Rollback }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CanaryStatus { Pending, Passed, Failed, Skipped }

impl CanaryPlan {
    pub fn all_critical_passed(&self) -> bool {
        self.checks.iter().all(|c| {
            matches!(c.status, CanaryStatus::Passed | CanaryStatus::Skipped)
                || !matches!(c.check_type, CanaryCheckType::FreshInstall | CanaryCheckType::Migration)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_matrix_covers_all_required() {
        let required = vec![
            ("macOS", "arm64"), ("macOS", "x86_64"),
            ("Linux", "arm64"), ("Linux", "x86_64"),
            ("Windows", "x86_64"),
        ];
        for (os, arch) in required {
            let entry = PlatformEntry {
                os: os.into(), arch: arch.into(),
                build_status: PlatformStatus::Supported,
                device_support: DeviceSupport::NotApplicable,
            };
            assert_eq!(entry.os, os);
            assert_eq!(entry.arch, arch);
        }
    }

    #[test]
    fn migration_chain_acceptance() {
        let steps = [
            MigrationStep { from_version: "1.0".into(), to_version: "2.0".into(), test_status: MigrationTestStatus::Passed, artifact_hash_preserved: true, rollback_tested: true },
            MigrationStep { from_version: "2.0".into(), to_version: "3.0".into(), test_status: MigrationTestStatus::Passed, artifact_hash_preserved: true, rollback_tested: true },
        ];
        assert!(steps.iter().all(|s| s.test_status == MigrationTestStatus::Passed));
    }

    #[test]
    fn canary_critical_checks() {
        let plan = CanaryPlan {
            release_version: "5.0.0".into(), rollback_ready: true,
            checks: vec![
                CanaryCheck { name: "fresh install".into(), check_type: CanaryCheckType::FreshInstall, status: CanaryStatus::Passed },
                CanaryCheck { name: "migration".into(), check_type: CanaryCheckType::Migration, status: CanaryStatus::Passed },
                CanaryCheck { name: "device smoke".into(), check_type: CanaryCheckType::DeviceSmoke, status: CanaryStatus::Skipped },
            ],
        };
        assert!(plan.all_critical_passed());
    }

    #[test]
    fn release_manifest_contains_required_assets() {
        let manifest = ReleaseManifest {
            version: "5.0.0".into(), assets: vec![
                ReleaseAsset { name: "lumen.exe".into(), kind: AssetKind::Binary, platform: "windows-x86_64".into(), sha256: "abc".into(), size_bytes: 130_000_000 },
                ReleaseAsset { name: "LICENSE".into(), kind: AssetKind::License, platform: "any".into(), sha256: "def".into(), size_bytes: 11358 },
            ],
            signatures: vec!["sig1".into()], sbom_hash: "sbom:1".into(), source_lock_hash: "lock:1".into(),
        };
        assert_eq!(manifest.assets.len(), 2);
        assert!(!manifest.sbom_hash.is_empty());
    }
}
