//! Display-facing truth contract for the Lumen TUI.
//!
//! This module deliberately contains no renderer or I/O. It gives every UI
//! surface the same validated facts instead of letting individual views infer
//! product identity, capability, cache, or verification state from copy and
//! spinner state.

use std::time::SystemTime;

/// Product identity shown by primary Lumen chrome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductIdentity {
    pub display_name: String,
    pub version: String,
    pub release_channel: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderState {
    Unknown,
    Ready { provider_id: String },
    Degraded { provider_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelState {
    Unknown,
    Selected { model_id: String },
    Unavailable { model_id: String, reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityState {
    Unknown {
        reason: String,
    },
    Checking,
    ChatOnly {
        evidence_id: String,
    },
    ToolReady {
        fingerprint: String,
        /// Optional so deserialized or partially assembled state cannot make a
        /// missing probe timestamp look valid.
        checked_at: Option<SystemTime>,
        evidence_id: String,
    },
    Failed {
        reason: String,
        evidence_id: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionSummary {
    Unknown,
    ReadOnly,
    AskBeforeChanges,
    AutoApproved,
    Denied { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheSource {
    ProviderReported,
    Estimated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CacheSummary {
    /// Ratio in the inclusive range 0.0..=1.0.
    pub hit_ratio: Option<f64>,
    pub source: Option<CacheSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationSummary {
    NotRun,
    Running {
        command: String,
        run_id: String,
    },
    Passed {
        command: String,
        run_id: String,
        finished_at: SystemTime,
        source_seq: u64,
    },
    Failed {
        command: String,
        run_id: String,
        exit_code: i32,
    },
    Stale {
        prior_run_id: String,
        changed_at_seq: u64,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkPhase {
    Idle,
    Understanding,
    Reading,
    Editing,
    Verifying,
    Complete,
    WaitingForUser,
    Blocked,
    Recovering,
}

/// One source of display truth shared by all Lumen UI surfaces.
#[derive(Debug, Clone, PartialEq)]
pub struct TruthSnapshot {
    pub product: ProductIdentity,
    pub provider: ProviderState,
    pub model: ModelState,
    pub capability: CapabilityState,
    pub permission: PermissionSummary,
    pub cache: CacheSummary,
    pub verification: VerificationSummary,
    pub phase: WorkPhase,
    pub captured_at: SystemTime,
}

/// Inputs whose identity determines whether a successful tool probe is still
/// evidence for the current runtime configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityFingerprintInput {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub tool_schema_hash: String,
    pub binary_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractError {
    pub field: &'static str,
    pub message: &'static str,
}

impl ContractError {
    const fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }
}

impl std::fmt::Display for ContractError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl std::error::Error for ContractError {}

fn require_non_empty(value: &str, field: &'static str) -> Result<(), ContractError> {
    if value.trim().is_empty() {
        Err(ContractError::new(field, "must not be empty"))
    } else {
        Ok(())
    }
}

/// Enforces the primary product identity. Provider and model names belong in
/// their own fields and must not replace `Lumen` in product chrome.
pub fn validate_product_identity(identity: &ProductIdentity) -> Result<(), ContractError> {
    if identity.display_name != "Lumen" {
        return Err(ContractError::new(
            "product.display_name",
            "primary product identity must be Lumen",
        ));
    }
    require_non_empty(&identity.version, "product.version")?;
    require_non_empty(&identity.release_channel, "product.release_channel")?;
    if identity.release_channel.trim().eq_ignore_ascii_case("beta") {
        return Err(ContractError::new(
            "product.release_channel",
            "Beta is legacy product copy, not a release channel",
        ));
    }
    Ok(())
}

/// Ensures capability claims have the evidence required by their state.
pub fn validate_capability(capability: &CapabilityState) -> Result<(), ContractError> {
    match capability {
        CapabilityState::Unknown { reason } => require_non_empty(reason, "capability.reason"),
        CapabilityState::Checking => Ok(()),
        CapabilityState::ChatOnly { evidence_id } => {
            require_non_empty(evidence_id, "capability.evidence_id")
        }
        CapabilityState::ToolReady {
            fingerprint,
            checked_at,
            evidence_id,
        } => {
            require_non_empty(fingerprint, "capability.fingerprint")?;
            if checked_at.is_none() {
                return Err(ContractError::new(
                    "capability.checked_at",
                    "ToolReady requires a probe timestamp",
                ));
            }
            require_non_empty(evidence_id, "capability.evidence_id")
        }
        CapabilityState::Failed {
            reason,
            evidence_id,
        } => {
            require_non_empty(reason, "capability.reason")?;
            if let Some(evidence_id) = evidence_id {
                require_non_empty(evidence_id, "capability.evidence_id")?;
            }
            Ok(())
        }
    }
}

/// Builds a stable, length-delimited fingerprint. Length prefixes prevent
/// ambiguous concatenations while preserving deterministic output.
pub fn capability_fingerprint(input: &CapabilityFingerprintInput) -> Result<String, ContractError> {
    let fields = [
        ("fingerprint.provider_id", input.provider_id.as_str()),
        ("fingerprint.model_id", input.model_id.as_str()),
        ("fingerprint.base_url", input.base_url.as_str()),
        (
            "fingerprint.tool_schema_hash",
            input.tool_schema_hash.as_str(),
        ),
        ("fingerprint.binary_id", input.binary_id.as_str()),
    ];
    let mut hasher = blake3::Hasher::new();
    for (field, value) in fields {
        require_non_empty(value, field)?;
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Invalidates old tool evidence when any fingerprint input has changed.
pub fn invalidate_capability_if_fingerprint_changed(
    capability: CapabilityState,
    current_fingerprint: &str,
) -> CapabilityState {
    match capability {
        CapabilityState::ToolReady { fingerprint, .. } if fingerprint != current_fingerprint => {
            CapabilityState::Unknown {
                reason: "capability fingerprint changed; a new probe is required".to_owned(),
            }
        }
        state => state,
    }
}

/// A passed verification is display-fresh only when it covers the latest
/// change sequence. Other states are never a fresh pass.
pub fn verification_is_fresh(verification: &VerificationSummary, last_change_seq: u64) -> bool {
    matches!(
        verification,
        VerificationSummary::Passed { source_seq, .. } if *source_seq >= last_change_seq
    )
}

/// Only provider-reported metrics may be presented as a definitive cache hit.
pub fn cache_allows_hit_display(cache: &CacheSummary) -> bool {
    cache.hit_ratio.is_some() && cache.source == Some(CacheSource::ProviderReported)
}

pub fn validate_cache(cache: &CacheSummary) -> Result<(), ContractError> {
    if let Some(hit_ratio) = cache.hit_ratio {
        if !hit_ratio.is_finite() || !(0.0..=1.0).contains(&hit_ratio) {
            return Err(ContractError::new(
                "cache.hit_ratio",
                "must be a finite ratio between zero and one",
            ));
        }
        if cache.source.is_none() {
            return Err(ContractError::new(
                "cache.source",
                "cache ratio requires a source",
            ));
        }
    }
    Ok(())
}

/// Validates the invariants required before a snapshot is consumed by a view.
pub fn validate_truth_snapshot(
    snapshot: &TruthSnapshot,
    last_change_seq: u64,
) -> Result<(), ContractError> {
    validate_product_identity(&snapshot.product)?;
    validate_capability(&snapshot.capability)?;
    validate_cache(&snapshot.cache)?;
    if matches!(snapshot.verification, VerificationSummary::Passed { .. })
        && !verification_is_fresh(&snapshot.verification, last_change_seq)
    {
        return Err(ContractError::new(
            "verification.source_seq",
            "passed verification is stale for the current files",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lumen_identity() -> ProductIdentity {
        ProductIdentity {
            display_name: "Lumen".to_owned(),
            version: "0.1.220-alpha.4".to_owned(),
            release_channel: "alpha".to_owned(),
        }
    }

    fn tool_ready(fingerprint: &str) -> CapabilityState {
        CapabilityState::ToolReady {
            fingerprint: fingerprint.to_owned(),
            checked_at: Some(SystemTime::UNIX_EPOCH),
            evidence_id: "probe-42".to_owned(),
        }
    }

    fn passed(source_seq: u64) -> VerificationSummary {
        VerificationSummary::Passed {
            command: "cargo test".to_owned(),
            run_id: "verify-42".to_owned(),
            finished_at: SystemTime::UNIX_EPOCH,
            source_seq,
        }
    }

    #[test]
    fn identity_lumen_with_real_channel_is_valid() {
        assert_eq!(validate_product_identity(&lumen_identity()), Ok(()));
    }

    #[test]
    fn identity_rejects_grok_product_names() {
        for display_name in ["Grok", "Grok Build"] {
            let identity = ProductIdentity {
                display_name: display_name.to_owned(),
                ..lumen_identity()
            };
            assert!(validate_product_identity(&identity).is_err());
        }
    }

    #[test]
    fn identity_rejects_beta_as_product_channel() {
        let identity = ProductIdentity {
            release_channel: "Beta".to_owned(),
            ..lumen_identity()
        };
        assert!(validate_product_identity(&identity).is_err());
    }

    #[test]
    fn tool_ready_requires_fingerprint_checked_at_and_evidence() {
        let cases = [
            CapabilityState::ToolReady {
                fingerprint: "".to_owned(),
                checked_at: Some(SystemTime::UNIX_EPOCH),
                evidence_id: "probe-42".to_owned(),
            },
            CapabilityState::ToolReady {
                fingerprint: "fp-42".to_owned(),
                checked_at: None,
                evidence_id: "probe-42".to_owned(),
            },
            CapabilityState::ToolReady {
                fingerprint: "fp-42".to_owned(),
                checked_at: Some(SystemTime::UNIX_EPOCH),
                evidence_id: "  ".to_owned(),
            },
        ];
        for capability in cases {
            assert!(validate_capability(&capability).is_err());
        }
    }

    #[test]
    fn tool_ready_with_complete_probe_evidence_is_valid() {
        assert_eq!(validate_capability(&tool_ready("fp-42")), Ok(()));
    }

    #[test]
    fn verification_pass_is_stale_before_last_change() {
        assert!(!verification_is_fresh(&passed(5), 6));
    }

    #[test]
    fn verification_pass_is_fresh_at_last_change() {
        assert!(verification_is_fresh(&passed(6), 6));
    }

    #[test]
    fn fingerprint_input_change_invalidates_tool_ready() {
        let original = CapabilityFingerprintInput {
            provider_id: "deepseek".to_owned(),
            model_id: "deepseek-chat".to_owned(),
            base_url: "https://api.deepseek.com".to_owned(),
            tool_schema_hash: "schema-v1".to_owned(),
            binary_id: "lumen-1".to_owned(),
        };
        let old_fingerprint = capability_fingerprint(&original).unwrap();

        for changed in [
            CapabilityFingerprintInput {
                provider_id: "xai".to_owned(),
                ..original.clone()
            },
            CapabilityFingerprintInput {
                model_id: "deepseek-reasoner".to_owned(),
                ..original.clone()
            },
            CapabilityFingerprintInput {
                base_url: "http://localhost:11434".to_owned(),
                ..original.clone()
            },
            CapabilityFingerprintInput {
                tool_schema_hash: "schema-v2".to_owned(),
                ..original.clone()
            },
            CapabilityFingerprintInput {
                binary_id: "lumen-2".to_owned(),
                ..original.clone()
            },
        ] {
            let current_fingerprint = capability_fingerprint(&changed).unwrap();
            let invalidated = invalidate_capability_if_fingerprint_changed(
                tool_ready(&old_fingerprint),
                &current_fingerprint,
            );
            assert!(matches!(invalidated, CapabilityState::Unknown { .. }));
        }
    }

    #[test]
    fn cache_without_provider_source_cannot_display_definitive_hit() {
        for source in [None, Some(CacheSource::Estimated)] {
            let cache = CacheSummary {
                hit_ratio: Some(0.82),
                source,
            };
            assert!(!cache_allows_hit_display(&cache));
        }
    }

    #[test]
    fn provider_reported_cache_ratio_can_display_definitive_hit() {
        let cache = CacheSummary {
            hit_ratio: Some(0.82),
            source: Some(CacheSource::ProviderReported),
        };
        assert!(cache_allows_hit_display(&cache));
    }

    #[test]
    fn truth_snapshot_rejects_stale_passed_verification() {
        let snapshot = TruthSnapshot {
            product: lumen_identity(),
            provider: ProviderState::Ready {
                provider_id: "deepseek".to_owned(),
            },
            model: ModelState::Selected {
                model_id: "deepseek-chat".to_owned(),
            },
            capability: tool_ready("fp-42"),
            permission: PermissionSummary::AskBeforeChanges,
            cache: CacheSummary {
                hit_ratio: Some(0.82),
                source: Some(CacheSource::ProviderReported),
            },
            verification: passed(5),
            phase: WorkPhase::Complete,
            captured_at: SystemTime::UNIX_EPOCH,
        };

        assert!(validate_truth_snapshot(&snapshot, 6).is_err());
    }
}
