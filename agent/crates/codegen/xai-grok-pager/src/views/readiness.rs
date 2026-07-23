//! FINAL-5UX Gate C recovery copy derived from a validated [`TruthSnapshot`].
//!
//! Pure display helpers — no network, no fake Tool-ready. Callers show this
//! after `/status`, `/probe`, or when chat-only blocks an edit.

use crate::ui_contract::{CapabilityState, TruthSnapshot};

/// Recovery steps the user can take. Order is intentional (probe first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryPlan {
    pub headline: String,
    pub steps: Vec<String>,
    /// Whether agent edit/tool flows must stay blocked.
    pub blocks_edit: bool,
}

/// Build recovery guidance from the current truth. Never invents Tool-ready.
///
/// Reasons are run through [`crate::views::status_detail::redacted_text`] so
/// recovery copy stays safe to paste into chats/tickets.
pub fn recovery_plan(snapshot: &TruthSnapshot) -> RecoveryPlan {
    use crate::views::status_detail::redacted_text;
    match &snapshot.capability {
        CapabilityState::Unknown { reason } => RecoveryPlan {
            headline: format!("Capability unknown — {}", redacted_text(reason)),
            steps: vec![
                "Run /probe to mark capability as Checking (does not invent Tool-ready).".into(),
                "Send a real coding task so the model issues a tool_call, or use scripts/probe-local.sh for local endpoints.".into(),
                "Open /status for the redacted evidence report.".into(),
                "Switch provider/model with /model if the current binding cannot call tools.".into(),
            ],
            blocks_edit: false,
        },
        CapabilityState::Checking => RecoveryPlan {
            headline: "Capability check in progress".into(),
            steps: vec![
                "Wait for a real tool_call on this binding, or finish scripts/probe-local.sh.".into(),
                "Do not treat green colours or model names as Tool-ready.".into(),
                "Open /status for the current snapshot.".into(),
            ],
            blocks_edit: false,
        },
        CapabilityState::ChatOnly { .. } => RecoveryPlan {
            headline: "Chat-only — agent edit is blocked".into(),
            steps: vec![
                "This binding answered without a tool_call. Edits are blocked until Tool-ready.".into(),
                "Switch to a tool-capable model/provider (/model), then re-run /probe.".into(),
                "Or continue in chat-only for explanations (no workspace edits).".into(),
                "Open /status for redacted evidence.".into(),
            ],
            blocks_edit: true,
        },
        CapabilityState::Failed { reason, .. } => RecoveryPlan {
            headline: format!("Capability failed — {}", redacted_text(reason)),
            steps: vec![
                "Re-check credentials and base URL for the current provider.".into(),
                "Run /probe again after fixing config.".into(),
                "Use scripts/probe-local.sh for local OpenAI-compatible endpoints.".into(),
                "Open /status and copy the redacted report if you need support.".into(),
            ],
            blocks_edit: true,
        },
        CapabilityState::ToolReady { .. } => RecoveryPlan {
            headline: "Tool-ready for the current binding".into(),
            steps: vec![
                "Agent tools are proven for this fingerprint.".into(),
                "After /model switch, capability returns to Unknown until a new tool_call.".into(),
                "Open /status for evidence digests.".into(),
            ],
            blocks_edit: false,
        },
    }
}

/// Multi-line recovery block for scrollback / status report.
pub fn recovery_report(snapshot: &TruthSnapshot) -> String {
    let plan = recovery_plan(snapshot);
    let mut lines = vec![
        "Recovery".to_owned(),
        format!("  {}", plan.headline),
    ];
    for (i, step) in plan.steps.iter().enumerate() {
        lines.push(format!("  {}. {step}", i + 1));
    }
    if plan.blocks_edit {
        lines.push("  Edit flow: blocked until Tool-ready evidence exists.".to_owned());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_contract::{
        CacheSummary, ModelState, PermissionSummary, ProductIdentity, ProviderState,
        VerificationSummary, WorkPhase,
    };
    use std::time::SystemTime;

    fn snap(cap: CapabilityState) -> TruthSnapshot {
        TruthSnapshot {
            product: ProductIdentity {
                display_name: "Lumen".into(),
                version: "0.1.0".into(),
                release_channel: "alpha".into(),
            },
            provider: ProviderState::Ready {
                provider_id: "deepseek".into(),
            },
            model: ModelState::Selected {
                model_id: "deepseek-chat".into(),
            },
            capability: cap,
            permission: PermissionSummary::AskBeforeChanges,
            cache: CacheSummary {
                hit_ratio: None,
                source: None,
            },
            verification: VerificationSummary::NotRun,
            phase: WorkPhase::Idle,
            captured_at: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn chat_only_recovery_blocks_edit_and_names_probe() {
        let plan = recovery_plan(&snap(CapabilityState::ChatOnly {
            evidence_id: "e1".into(),
        }));
        assert!(plan.blocks_edit);
        assert!(plan.headline.to_ascii_lowercase().contains("chat-only"));
        let report = recovery_report(&snap(CapabilityState::ChatOnly {
            evidence_id: "e1".into(),
        }));
        assert!(report.contains("/probe") || report.contains("/status"));
        assert!(report.contains("blocked") || report.contains("Edit flow"));
    }

    #[test]
    fn tool_ready_does_not_block_edit() {
        let plan = recovery_plan(&snap(CapabilityState::ToolReady {
            fingerprint: "fp".into(),
            checked_at: Some(SystemTime::UNIX_EPOCH),
            evidence_id: "e".into(),
        }));
        assert!(!plan.blocks_edit);
    }

    #[test]
    fn unknown_never_claims_tool_ready() {
        let report = recovery_report(&snap(CapabilityState::Unknown {
            reason: "not probed".into(),
        }));
        assert!(!report.to_ascii_lowercase().contains("you are tool-ready"));
        assert!(report.contains("/probe") || report.contains("tool_call"));
    }
}
