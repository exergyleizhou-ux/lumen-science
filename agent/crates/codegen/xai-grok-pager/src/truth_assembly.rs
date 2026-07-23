//! FINAL-5UX Gate C (data side): assemble display truth from runtime facts.
//!
//! No renderer, no network. UI (Gate D) and readiness wizards consume
//! [`TruthSnapshot`] values produced here after validation via [`ui_contract`].

use std::time::SystemTime;

use crate::ui_contract::{
    CacheSource, CacheSummary, CapabilityFingerprintInput, CapabilityState, ModelState,
    PermissionSummary, ProductIdentity, ProviderState, TruthSnapshot, VerificationSummary,
    WorkPhase, cache_allows_hit_display, capability_fingerprint,
    invalidate_capability_if_fingerprint_changed, validate_truth_snapshot, verification_is_fresh,
};

/// Outcome of a capability probe (CLI, startup, or in-process tool exercise).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Endpoint unreachable or probe aborted before a model answer.
    Unreachable { reason: String },
    /// Model answered without a structured tool_call.
    ChatOnly,
    /// Model emitted a real tool_call (agent-capable).
    ToolCallObserved,
    /// Probe ran but failed for a classified reason.
    Failed { reason: String },
}

/// Evidence recorded when a probe finishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeEvidence {
    pub outcome: ProbeOutcome,
    /// Stable id for this probe run (log path, uuid, etc.).
    pub evidence_id: String,
    pub checked_at: SystemTime,
    pub fingerprint_input: CapabilityFingerprintInput,
}

fn fingerprint_value_is_known(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && !value.eq_ignore_ascii_case("unknown")
}

/// Build a capability fingerprint only when every identity input is real.
///
/// `capability_fingerprint` intentionally accepts any non-empty string as a
/// general hashing primitive. Truth claims are stricter: placeholder values
/// such as `"unknown"` must never become durable Tool-ready evidence.
pub fn claimable_capability_fingerprint(
    input: &CapabilityFingerprintInput,
) -> Result<String, String> {
    let fields = [
        ("provider_id", input.provider_id.as_str()),
        ("model_id", input.model_id.as_str()),
        ("base_url", input.base_url.as_str()),
        ("tool_schema_hash", input.tool_schema_hash.as_str()),
        ("binary_id", input.binary_id.as_str()),
    ];
    if let Some((field, _)) = fields
        .into_iter()
        .find(|(_, value)| !fingerprint_value_is_known(value))
    {
        return Err(format!(
            "capability fingerprint {field} is unknown; Tool-ready requires a complete binding"
        ));
    }
    capability_fingerprint(input).map_err(|e| format!("fingerprint: {e}"))
}

/// Mutable session facts used to rebuild a [`TruthSnapshot`].
#[derive(Debug, Clone, PartialEq)]
pub struct TruthSessionState {
    pub product: ProductIdentity,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub permission: PermissionSummary,
    pub cache: CacheSummary,
    pub verification: VerificationSummary,
    pub phase: WorkPhase,
    pub capability: CapabilityState,
    /// Monotonic counter of user/agent file mutations in this session view.
    pub last_change_seq: u64,
    /// Fingerprint inputs for the *current* provider/model/runtime binding.
    pub fingerprint_input: CapabilityFingerprintInput,
}

impl TruthSessionState {
    pub fn new(product: ProductIdentity, fingerprint_input: CapabilityFingerprintInput) -> Self {
        Self {
            product,
            provider_id: None,
            model_id: None,
            permission: PermissionSummary::Unknown,
            cache: CacheSummary {
                hit_ratio: None,
                source: None,
            },
            verification: VerificationSummary::NotRun,
            phase: WorkPhase::Idle,
            capability: CapabilityState::Unknown {
                reason: "capability not probed yet".to_owned(),
            },
            last_change_seq: 0,
            fingerprint_input,
        }
    }
}

/// Map a finished probe into a contract-valid [`CapabilityState`].
pub fn capability_from_probe(evidence: &ProbeEvidence) -> Result<CapabilityState, String> {
    if evidence.evidence_id.trim().is_empty() {
        return Err("probe evidence_id must not be empty".to_owned());
    }
    let state = match &evidence.outcome {
        ProbeOutcome::Unreachable { reason } => CapabilityState::Failed {
            reason: reason.clone(),
            evidence_id: Some(evidence.evidence_id.clone()),
        },
        ProbeOutcome::Failed { reason } => CapabilityState::Failed {
            reason: reason.clone(),
            evidence_id: Some(evidence.evidence_id.clone()),
        },
        ProbeOutcome::ChatOnly => CapabilityState::ChatOnly {
            evidence_id: evidence.evidence_id.clone(),
        },
        ProbeOutcome::ToolCallObserved => CapabilityState::ToolReady {
            fingerprint: claimable_capability_fingerprint(&evidence.fingerprint_input)?,
            checked_at: Some(evidence.checked_at),
            evidence_id: evidence.evidence_id.clone(),
        },
    };
    crate::ui_contract::validate_capability(&state).map_err(|e| e.to_string())?;
    Ok(state)
}

/// Begin a probe: surface shows Checking until evidence arrives.
pub fn begin_capability_probe(state: &mut TruthSessionState) {
    state.capability = CapabilityState::Checking;
    state.phase = WorkPhase::Understanding;
}

/// Apply probe evidence and refresh fingerprint binding from the evidence input.
pub fn apply_probe_evidence(
    state: &mut TruthSessionState,
    evidence: ProbeEvidence,
) -> Result<(), String> {
    state.fingerprint_input = evidence.fingerprint_input.clone();
    state.provider_id = fingerprint_value_is_known(&evidence.fingerprint_input.provider_id)
        .then(|| evidence.fingerprint_input.provider_id.clone());
    state.model_id = fingerprint_value_is_known(&evidence.fingerprint_input.model_id)
        .then(|| evidence.fingerprint_input.model_id.clone());
    state.capability = capability_from_probe(&evidence)?;
    if matches!(state.capability, CapabilityState::ToolReady { .. }) {
        if matches!(
            state.phase,
            WorkPhase::Understanding | WorkPhase::Idle | WorkPhase::Recovering
        ) {
            state.phase = WorkPhase::Idle;
        }
    } else if matches!(state.capability, CapabilityState::Failed { .. }) {
        state.phase = WorkPhase::Blocked;
    }
    Ok(())
}

/// Provider/model/runtime identity changed: drop ToolReady until re-probed.
pub fn on_binding_changed(
    state: &mut TruthSessionState,
    new_input: CapabilityFingerprintInput,
) -> Result<(), String> {
    state.capability = match claimable_capability_fingerprint(&new_input) {
        Ok(new_fp) => {
            invalidate_capability_if_fingerprint_changed(state.capability.clone(), &new_fp)
        }
        Err(_) if matches!(state.capability, CapabilityState::ToolReady { .. }) => {
            CapabilityState::Unknown {
                reason: "provider/model binding is incomplete; re-run capability probe after identity is known"
                    .to_owned(),
            }
        }
        Err(_) => state.capability.clone(),
    };
    state.fingerprint_input = new_input;
    state.provider_id = fingerprint_value_is_known(&state.fingerprint_input.provider_id)
        .then(|| state.fingerprint_input.provider_id.clone());
    state.model_id = fingerprint_value_is_known(&state.fingerprint_input.model_id)
        .then(|| state.fingerprint_input.model_id.clone());
    if matches!(state.capability, CapabilityState::Unknown { .. }) {
        // Re-bind always requires a new probe for tool claims.
        state.capability = CapabilityState::Unknown {
            reason: "provider/model binding changed; re-run capability probe".to_owned(),
        };
    }
    Ok(())
}

/// Record a working-tree mutation; fresh Passed verification becomes Stale.
pub fn note_workspace_change(state: &mut TruthSessionState) {
    state.last_change_seq = state.last_change_seq.saturating_add(1);
    if let VerificationSummary::Passed {
        run_id, source_seq, ..
    } = &state.verification
    {
        if *source_seq < state.last_change_seq {
            state.verification = VerificationSummary::Stale {
                prior_run_id: run_id.clone(),
                changed_at_seq: state.last_change_seq,
            };
        }
    }
    if matches!(state.phase, WorkPhase::Complete | WorkPhase::Idle) {
        state.phase = WorkPhase::Editing;
    }
}

/// Mark verification as Passed for the current `last_change_seq`.
pub fn note_verification_passed(
    state: &mut TruthSessionState,
    command: impl Into<String>,
    run_id: impl Into<String>,
    finished_at: SystemTime,
) {
    let command = command.into();
    let run_id = run_id.into();
    state.verification = VerificationSummary::Passed {
        command,
        run_id,
        finished_at,
        source_seq: state.last_change_seq,
    };
    state.phase = WorkPhase::Complete;
}

/// Build a validated snapshot for UI consumption.
pub fn assemble_truth_snapshot(
    state: &TruthSessionState,
    captured_at: SystemTime,
) -> Result<TruthSnapshot, String> {
    let provider = match &state.provider_id {
        Some(id) if fingerprint_value_is_known(id) => ProviderState::Ready {
            provider_id: id.clone(),
        },
        _ => ProviderState::Unknown,
    };
    let model = match &state.model_id {
        Some(id) if fingerprint_value_is_known(id) => ModelState::Selected {
            model_id: id.clone(),
        },
        _ => ModelState::Unknown,
    };

    // Drop stale tool claims if fingerprint no longer matches current binding.
    let capability = match &state.capability {
        CapabilityState::ToolReady { .. } => {
            match claimable_capability_fingerprint(&state.fingerprint_input) {
                Ok(current_fp) => invalidate_capability_if_fingerprint_changed(
                    state.capability.clone(),
                    &current_fp,
                ),
                Err(reason) => CapabilityState::Unknown { reason },
            }
        }
        other => other.clone(),
    };

    let mut verification = state.verification.clone();
    if matches!(verification, VerificationSummary::Passed { .. })
        && !verification_is_fresh(&verification, state.last_change_seq)
    {
        if let VerificationSummary::Passed { run_id, .. } = &state.verification {
            verification = VerificationSummary::Stale {
                prior_run_id: run_id.clone(),
                changed_at_seq: state.last_change_seq,
            };
        }
    }

    let snapshot = TruthSnapshot {
        product: state.product.clone(),
        provider,
        model,
        capability,
        permission: state.permission.clone(),
        cache: state.cache.clone(),
        verification,
        phase: state.phase,
        captured_at,
    };
    validate_truth_snapshot(&snapshot, state.last_change_seq).map_err(|e| e.to_string())?;
    Ok(snapshot)
}

/// Whether the snapshot may claim a definitive cache hit in the truth bar.
pub fn snapshot_allows_cache_hit_display(snapshot: &TruthSnapshot) -> bool {
    cache_allows_hit_display(&snapshot.cache)
}

/// Helper: provider-reported cache metrics.
pub fn cache_from_provider_ratio(hit_ratio: f64) -> CacheSummary {
    CacheSummary {
        hit_ratio: Some(hit_ratio),
        source: Some(CacheSource::ProviderReported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn product() -> ProductIdentity {
        ProductIdentity {
            display_name: "Lumen".to_owned(),
            version: "0.1.220-alpha.4".to_owned(),
            release_channel: "alpha".to_owned(),
        }
    }

    fn fp(model: &str) -> CapabilityFingerprintInput {
        CapabilityFingerprintInput {
            provider_id: "deepseek".to_owned(),
            model_id: model.to_owned(),
            base_url: "https://api.deepseek.com/v1".to_owned(),
            tool_schema_hash: "schema-v1".to_owned(),
            binary_id: "lumen-test".to_owned(),
        }
    }

    fn session() -> TruthSessionState {
        TruthSessionState::new(product(), fp("deepseek-chat"))
    }

    #[test]
    fn probe_tool_call_becomes_tool_ready_with_fingerprint() {
        let mut state = session();
        begin_capability_probe(&mut state);
        assert!(matches!(state.capability, CapabilityState::Checking));

        let evidence = ProbeEvidence {
            outcome: ProbeOutcome::ToolCallObserved,
            evidence_id: "probe-1".to_owned(),
            checked_at: SystemTime::UNIX_EPOCH,
            fingerprint_input: fp("deepseek-chat"),
        };
        apply_probe_evidence(&mut state, evidence).unwrap();
        let snap = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap();
        match &snap.capability {
            CapabilityState::ToolReady {
                fingerprint,
                evidence_id,
                checked_at,
            } => {
                assert!(!fingerprint.is_empty());
                assert_eq!(evidence_id, "probe-1");
                assert!(checked_at.is_some());
            }
            other => panic!("expected ToolReady, got {other:?}"),
        }
        assert!(matches!(
            snap.provider,
            ProviderState::Ready { ref provider_id } if provider_id == "deepseek"
        ));
    }

    #[test]
    fn probe_chat_only_is_not_tool_ready() {
        let mut state = session();
        apply_probe_evidence(
            &mut state,
            ProbeEvidence {
                outcome: ProbeOutcome::ChatOnly,
                evidence_id: "probe-chat".to_owned(),
                checked_at: SystemTime::UNIX_EPOCH,
                fingerprint_input: fp("local-model"),
            },
        )
        .unwrap();
        let snap = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap();
        assert!(matches!(snap.capability, CapabilityState::ChatOnly { .. }));
    }

    #[test]
    fn tool_call_with_unknown_binding_cannot_become_tool_ready() {
        let mut input = fp("deepseek-chat");
        input.provider_id = "unknown".to_owned();
        let err = capability_from_probe(&ProbeEvidence {
            outcome: ProbeOutcome::ToolCallObserved,
            evidence_id: "probe-unknown".to_owned(),
            checked_at: SystemTime::UNIX_EPOCH,
            fingerprint_input: input,
        })
        .unwrap_err();
        assert!(
            err.contains("provider_id") && err.contains("unknown"),
            "{err}"
        );
    }

    #[test]
    fn unknown_provider_is_not_assembled_as_ready() {
        let mut state = session();
        state.provider_id = Some("unknown".to_owned());
        let snapshot = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap();
        assert!(matches!(snapshot.provider, ProviderState::Unknown));
    }

    #[test]
    fn model_change_invalidates_tool_ready() {
        let mut state = session();
        apply_probe_evidence(
            &mut state,
            ProbeEvidence {
                outcome: ProbeOutcome::ToolCallObserved,
                evidence_id: "probe-1".to_owned(),
                checked_at: SystemTime::UNIX_EPOCH,
                fingerprint_input: fp("deepseek-chat"),
            },
        )
        .unwrap();
        assert!(matches!(
            state.capability,
            CapabilityState::ToolReady { .. }
        ));

        on_binding_changed(&mut state, fp("deepseek-reasoner")).unwrap();
        let snap = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap();
        assert!(
            matches!(snap.capability, CapabilityState::Unknown { .. }),
            "expected Unknown after model change, got {:?}",
            snap.capability
        );
    }

    #[test]
    fn workspace_edit_stales_passed_verification() {
        let mut state = session();
        note_verification_passed(&mut state, "go test ./...", "run-1", SystemTime::UNIX_EPOCH);
        assert!(verification_is_fresh(
            &state.verification,
            state.last_change_seq
        ));

        note_workspace_change(&mut state);
        let snap = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap();
        assert!(
            matches!(snap.verification, VerificationSummary::Stale { .. }),
            "expected Stale verification, got {:?}",
            snap.verification
        );
        assert!(!verification_is_fresh(
            &snap.verification,
            state.last_change_seq
        ));
    }

    #[test]
    fn estimated_cache_cannot_display_definitive_hit() {
        let mut state = session();
        state.cache = CacheSummary {
            hit_ratio: Some(0.9),
            source: Some(CacheSource::Estimated),
        };
        let snap = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap();
        assert!(!snapshot_allows_cache_hit_display(&snap));
    }

    #[test]
    fn provider_reported_cache_can_display_hit() {
        let mut state = session();
        state.cache = cache_from_provider_ratio(0.82);
        let snap = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap();
        assert!(snapshot_allows_cache_hit_display(&snap));
    }

    #[test]
    fn assemble_rejects_invalid_product_identity() {
        let mut state = session();
        state.product.display_name = "Grok".to_owned();
        let err = assemble_truth_snapshot(&state, SystemTime::UNIX_EPOCH).unwrap_err();
        assert!(err.contains("Lumen") || err.contains("product"), "{err}");
    }
}
