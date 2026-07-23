//! Redacted `/status` report derived from the shared truth snapshot.

use std::time::{Duration, SystemTime};

use crate::ui_contract::{
    CacheSource, CapabilityState, ModelState, PermissionSummary, ProviderState, TruthSnapshot,
    VerificationSummary,
};

fn age(then: SystemTime, now: SystemTime) -> String {
    let secs = now.duration_since(then).unwrap_or(Duration::ZERO).as_secs();
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3_600 {
        format!("{}m ago", secs / 60)
    } else {
        format!("{}h ago", secs / 3_600)
    }
}

fn opaque_id(value: &str) -> String {
    let digest = blake3::hash(value.as_bytes()).to_hex();
    format!("id:{}", &digest[..12])
}

/// Keep reports copy-safe even when runtime errors or commands contain URLs,
/// credential-looking tokens, control sequences, or `key=value` secrets.
pub(crate) fn redacted_text(value: &str) -> String {
    const SECRET_KEYS: &[&str] = &[
        "api_key",
        "apikey",
        "authorization",
        "bearer",
        "password",
        "secret",
        "token",
    ];
    let clean: String = value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect();
    let mut redact_next = false;
    let mut words = Vec::new();
    for word in clean.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        if redact_next {
            words.push("[redacted]".to_owned());
            redact_next = false;
            continue;
        }
        if lower == "bearer" || lower == "authorization:" {
            words.push(word.to_owned());
            redact_next = true;
            continue;
        }
        if lower.starts_with("sk-") || lower.starts_with("ghp_") || lower.starts_with("github_pat_")
        {
            words.push("[redacted]".to_owned());
            continue;
        }
        // URLs first: otherwise `https://host/v1?api_key=…` is only partially
        // scrubbed by the key=value branch and still leaks hostnames.
        if lower.contains("://") {
            let scheme = word.split_once("://").map_or("url", |(scheme, _)| scheme);
            words.push(format!("{scheme}://[redacted]"));
            continue;
        }
        if let Some((key, _)) = word.split_once('=')
            && SECRET_KEYS
                .iter()
                .any(|candidate| key.to_ascii_lowercase().contains(candidate))
        {
            words.push(format!("{key}=[redacted]"));
            continue;
        }
        // Absolute filesystem paths often appear as evidence/run IDs.
        if word.starts_with('/') || word.starts_with("~/") {
            words.push(opaque_id(word));
            continue;
        }
        words.push(word.to_owned());
    }
    let joined = words.join(" ");
    let mut chars = joined.chars();
    let bounded: String = chars.by_ref().take(160).collect();
    if chars.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
}

/// Format a copy-safe report. The contract contains no credential values or
/// auth URLs; evidence identifiers and fingerprints are deliberately shortened.
pub fn redacted_report(snapshot: &TruthSnapshot, now: SystemTime) -> String {
    let product = format!(
        "{} {} ({})",
        snapshot.product.display_name, snapshot.product.version, snapshot.product.release_channel
    );
    let provider = match &snapshot.provider {
        ProviderState::Unknown => "unknown · source runtime · reason not selected".to_owned(),
        ProviderState::Ready { provider_id } => {
            format!("{} · source runtime · current", redacted_text(provider_id))
        }
        ProviderState::Degraded {
            provider_id,
            reason,
        } => format!(
            "{} · degraded: {} · source runtime",
            redacted_text(provider_id),
            redacted_text(reason)
        ),
    };
    let model = match &snapshot.model {
        ModelState::Unknown => "unknown · source runtime".to_owned(),
        ModelState::Selected { model_id } => {
            format!("{} · source session", redacted_text(model_id))
        }
        ModelState::Unavailable { model_id, reason } => {
            format!(
                "{} · unavailable: {} · source session",
                redacted_text(model_id),
                redacted_text(reason)
            )
        }
    };
    let capability = match &snapshot.capability {
        CapabilityState::Unknown { reason } => {
            format!(
                "Capability unknown · reason {} · source probe",
                redacted_text(reason)
            )
        }
        CapabilityState::Checking => "Checking · source live probe".to_owned(),
        CapabilityState::ChatOnly { evidence_id } => format!(
            "Chat-only · evidence {} · source probe",
            opaque_id(evidence_id)
        ),
        CapabilityState::ToolReady {
            fingerprint,
            checked_at,
            evidence_id,
        } => format!(
            "Tool-ready · checked {} · fingerprint {} · evidence {}",
            checked_at.map_or_else(|| "unknown".to_owned(), |t| age(t, now)),
            opaque_id(fingerprint),
            opaque_id(evidence_id)
        ),
        CapabilityState::Failed {
            reason,
            evidence_id,
        } => format!(
            "Failed: {} · evidence {} · source probe",
            redacted_text(reason),
            evidence_id
                .as_deref()
                .map(opaque_id)
                .unwrap_or_else(|| "unknown".to_owned())
        ),
    };
    let permission = match &snapshot.permission {
        PermissionSummary::Unknown => "unknown · source policy".to_owned(),
        PermissionSummary::ReadOnly => "Read-only · source policy".to_owned(),
        PermissionSummary::AskBeforeChanges => "Ask before changes · source policy".to_owned(),
        PermissionSummary::AutoApproved => "Accept edits · source policy".to_owned(),
        PermissionSummary::Denied { reason } => {
            format!("Denied: {} · source policy", redacted_text(reason))
        }
    };
    let cache = match (snapshot.cache.hit_ratio, snapshot.cache.source) {
        (Some(ratio), Some(CacheSource::ProviderReported)) => {
            format!(
                "{:.0}% hit · provider reported · current window",
                ratio * 100.0
            )
        }
        (Some(ratio), Some(CacheSource::Estimated)) => {
            format!("{:.0}% estimated · source local estimate", ratio * 100.0)
        }
        _ => "unavailable · reason provider supplied no metrics".to_owned(),
    };
    let verification = match &snapshot.verification {
        VerificationSummary::NotRun => "Not run · source verification events".to_owned(),
        VerificationSummary::Running { command, run_id } => {
            format!(
                "Running · command {} · run {}",
                redacted_text(command),
                opaque_id(run_id)
            )
        }
        VerificationSummary::Passed {
            command,
            run_id,
            finished_at,
            source_seq,
        } => format!(
            "Passed · fresh · {} · run {} · {} · source seq {source_seq}",
            redacted_text(command),
            opaque_id(run_id),
            age(*finished_at, now)
        ),
        VerificationSummary::Failed {
            command,
            run_id,
            exit_code,
        } => format!(
            "Failed ({exit_code}) · {} · run {}",
            redacted_text(command),
            opaque_id(run_id)
        ),
        VerificationSummary::Stale {
            prior_run_id,
            changed_at_seq,
        } => format!(
            "Stale · prior run {} · changed at seq {changed_at_seq}",
            opaque_id(prior_run_id)
        ),
        VerificationSummary::Unavailable { reason } => {
            format!(
                "Unavailable: {} · source verification events",
                redacted_text(reason)
            )
        }
    };
    let recovery = crate::views::readiness::recovery_report(snapshot);
    format!(
        "Lumen status\n\nProduct      {product}\nProvider     {provider}\nModel        {model}\nCapability   {capability}\nPermission   {permission}\nCache        {cache}\nVerification {verification}\nCaptured     {}\n\n{recovery}",
        age(snapshot.captured_at, now)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_contract::{
        CacheSummary, ProductIdentity, ProviderState, VerificationSummary, WorkPhase,
    };

    #[test]
    fn report_is_explicit_and_shortens_evidence() {
        let snapshot = TruthSnapshot {
            product: ProductIdentity {
                display_name: "Lumen".into(),
                version: "1.2.3".into(),
                release_channel: "alpha".into(),
            },
            provider: ProviderState::Ready {
                provider_id: "deepseek".into(),
            },
            model: ModelState::Selected {
                model_id: "deepseek-chat".into(),
            },
            capability: CapabilityState::ToolReady {
                fingerprint: "1234567890abcdef".into(),
                checked_at: Some(SystemTime::UNIX_EPOCH),
                evidence_id: "evidence-secret-looking-long-id".into(),
            },
            permission: PermissionSummary::AskBeforeChanges,
            cache: CacheSummary {
                hit_ratio: None,
                source: None,
            },
            verification: VerificationSummary::NotRun,
            phase: WorkPhase::Idle,
            captured_at: SystemTime::UNIX_EPOCH,
        };
        let text = redacted_report(&snapshot, SystemTime::UNIX_EPOCH + Duration::from_secs(120));
        assert!(text.contains("Tool-ready"));
        assert!(text.contains("fingerprint id:"));
        assert!(!text.contains("1234567890abcdef"));
        assert!(!text.contains("evidence-secret-looking-long-id"));
        assert!(text.contains("Cache        unavailable"));
        assert!(text.contains("Verification Not run"));
    }

    #[test]
    fn report_scrubs_urls_tokens_commands_and_control_sequences() {
        let snapshot = TruthSnapshot {
            product: ProductIdentity {
                display_name: "Lumen".into(),
                version: "1.2.3".into(),
                release_channel: "alpha".into(),
            },
            provider: ProviderState::Degraded {
                provider_id: "deepseek".into(),
                reason: "401 from https://api.example/v1?api_key=sk-provider-secret\n".into(),
            },
            model: ModelState::Selected {
                model_id: "deepseek-chat".into(),
            },
            capability: CapabilityState::Failed {
                reason: "Bearer sk-capability-secret".into(),
                evidence_id: Some("/private/evidence/path".into()),
            },
            permission: PermissionSummary::Denied {
                reason: "token=permission-secret".into(),
            },
            cache: CacheSummary {
                hit_ratio: None,
                source: None,
            },
            verification: VerificationSummary::Running {
                command: "API_KEY=sk-command-secret cargo test".into(),
                run_id: "/private/run/path".into(),
            },
            phase: WorkPhase::Blocked,
            captured_at: SystemTime::UNIX_EPOCH,
        };
        let text = redacted_report(&snapshot, SystemTime::UNIX_EPOCH);
        for secret in [
            "api.example",
            "sk-provider-secret",
            "sk-capability-secret",
            "permission-secret",
            "sk-command-secret",
            "/private/evidence/path",
            "/private/run/path",
        ] {
            assert!(!text.contains(secret), "leaked {secret}: {text}");
        }
        assert!(text.contains("[redacted]"));

    }
}
