//! Shared FINAL-5UX truth-bar semantics and renderer.
//!
//! Fullscreen, minimal, and dashboard surfaces call this module with the same
//! validated [`TruthSnapshot`].  Copy is derived here once; individual views
//! must not infer readiness from model names, spinners, or success colours.

use std::time::{Duration, SystemTime};

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::theme::Theme;
use crate::ui_contract::{
    CacheSource, CapabilityState, ModelState, PermissionSummary, ProviderState, TruthSnapshot,
    VerificationSummary, cache_allows_hit_display,
};

/// Semantic severity used for both colour and non-colour rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TruthTone {
    Passive,
    Success,
    Caution,
    Blocker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    full: String,
    compact: String,
    tone: TruthTone,
}

/// Display-ready truth bar.  `text` always contains non-colour state markers,
/// so 16-colour and `NO_COLOR` modes retain the same meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TruthBarLine {
    pub text: String,
    pub tone: TruthTone,
    pub compact: bool,
}

fn provider_name(id: &str) -> String {
    match id.trim().to_ascii_lowercase().as_str() {
        "deepseek" => "DeepSeek".to_owned(),
        "openai" => "OpenAI".to_owned(),
        "anthropic" => "Anthropic".to_owned(),
        "xai" => "xAI".to_owned(),
        "local" => "Local".to_owned(),
        _ => id.trim().to_owned(),
    }
}

fn provider_model(snapshot: &TruthSnapshot) -> Segment {
    let (provider, mut tone) = match &snapshot.provider {
        ProviderState::Unknown => ("Unknown provider".to_owned(), TruthTone::Caution),
        ProviderState::Ready { provider_id } => (provider_name(provider_id), TruthTone::Passive),
        ProviderState::Degraded { provider_id, .. } => (
            format!("! {}", provider_name(provider_id)),
            TruthTone::Blocker,
        ),
    };
    let model = match &snapshot.model {
        ModelState::Unknown => {
            tone = tone.max(TruthTone::Caution);
            "unknown model".to_owned()
        }
        ModelState::Selected { model_id } => model_id.clone(),
        ModelState::Unavailable { model_id, .. } => {
            tone = TruthTone::Blocker;
            format!("! {model_id}")
        }
    };
    Segment {
        full: format!("{provider} · {model}"),
        compact: format!("{provider}/{model}"),
        tone,
    }
}

fn capability(snapshot: &TruthSnapshot) -> Segment {
    let (full, compact, tone) = match &snapshot.capability {
        CapabilityState::Unknown { .. } => (
            "? Capability unknown",
            "? capability unknown",
            TruthTone::Caution,
        ),
        CapabilityState::Checking => (
            "… Checking capability",
            "… checking tools",
            TruthTone::Caution,
        ),
        CapabilityState::ChatOnly { .. } => ("✗ Chat-only", "✗ chat-only", TruthTone::Blocker),
        CapabilityState::ToolReady { .. } => ("✓ Tool-ready", "✓ tools", TruthTone::Success),
        CapabilityState::Failed { .. } => {
            ("✗ Capability failed", "✗ tools failed", TruthTone::Blocker)
        }
    };
    Segment {
        full: full.to_owned(),
        compact: compact.to_owned(),
        tone,
    }
}

fn permission(snapshot: &TruthSnapshot) -> Segment {
    let (text, compact, tone) = match &snapshot.permission {
        PermissionSummary::Unknown => ("Permission unknown", "? permission", TruthTone::Caution),
        PermissionSummary::ReadOnly => ("Read-only", "Read-only", TruthTone::Caution),
        PermissionSummary::AskBeforeChanges => ("Ask before changes", "Ask", TruthTone::Passive),
        PermissionSummary::AutoApproved => ("Accept edits", "Accept edits", TruthTone::Caution),
        PermissionSummary::Denied { .. } => ("✗ Permission denied", "✗ denied", TruthTone::Blocker),
    };
    Segment {
        full: text.to_owned(),
        compact: compact.to_owned(),
        tone,
    }
}

fn cache(snapshot: &TruthSnapshot) -> Segment {
    let (full, compact, tone) = if cache_allows_hit_display(&snapshot.cache) {
        let pct = snapshot.cache.hit_ratio.unwrap_or_default() * 100.0;
        (
            format!("Cache {pct:.0}% hit"),
            format!("cache {pct:.0}%"),
            TruthTone::Passive,
        )
    } else if let (Some(ratio), Some(CacheSource::Estimated)) =
        (snapshot.cache.hit_ratio, snapshot.cache.source)
    {
        let pct = ratio * 100.0;
        (
            format!("Cache {pct:.0}% estimated"),
            format!("cache ~{pct:.0}%"),
            TruthTone::Caution,
        )
    } else {
        (
            "Cache unavailable".to_owned(),
            "cache unavailable".to_owned(),
            TruthTone::Passive,
        )
    };
    Segment {
        full,
        compact,
        tone,
    }
}

fn short_age(then: SystemTime, now: SystemTime) -> String {
    let age = now.duration_since(then).unwrap_or(Duration::ZERO);
    if age.as_secs() < 5 {
        "now".to_owned()
    } else if age.as_secs() < 60 {
        format!("{}s ago", age.as_secs())
    } else if age.as_secs() < 3_600 {
        format!("{}m ago", age.as_secs() / 60)
    } else if age.as_secs() < 86_400 {
        format!("{}h ago", age.as_secs() / 3_600)
    } else {
        format!("{}d ago", age.as_secs() / 86_400)
    }
}

fn verification(snapshot: &TruthSnapshot, now: SystemTime) -> Segment {
    let (full, compact, tone) = match &snapshot.verification {
        VerificationSummary::NotRun => (
            "- Not verified".to_owned(),
            "- not verified".to_owned(),
            TruthTone::Caution,
        ),
        VerificationSummary::Running { .. } => (
            "… Verifying".to_owned(),
            "… verifying".to_owned(),
            TruthTone::Caution,
        ),
        VerificationSummary::Passed { finished_at, .. } => (
            format!("✓ Verified {}", short_age(*finished_at, now)),
            "✓ verify".to_owned(),
            TruthTone::Success,
        ),
        VerificationSummary::Failed { .. } => (
            "✗ Verification failed".to_owned(),
            "✗ verify failed".to_owned(),
            TruthTone::Blocker,
        ),
        VerificationSummary::Stale { .. } => (
            "! Verification stale".to_owned(),
            "! stale".to_owned(),
            TruthTone::Caution,
        ),
        VerificationSummary::Unavailable { .. } => (
            "? Verification unavailable".to_owned(),
            "? verify unavailable".to_owned(),
            TruthTone::Caution,
        ),
    };
    Segment {
        full,
        compact,
        tone,
    }
}

fn truncate_middle(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_owned();
    }
    if max_width <= 1 {
        return "…".to_owned();
    }
    let target = max_width - 1;
    let left_target = target.div_ceil(2);
    let right_target = target / 2;
    let mut left = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let width = ch.width().unwrap_or(0);
        if used + width > left_target {
            break;
        }
        left.push(ch);
        used += width;
    }
    let mut right_rev = String::new();
    used = 0;
    for ch in text.chars().rev() {
        let width = ch.width().unwrap_or(0);
        if used + width > right_target {
            break;
        }
        right_rev.push(ch);
        used += width;
    }
    let right: String = right_rev.chars().rev().collect();
    format!("{left}…{right}")
}

/// Build a width-safe line. At compact widths cache is dropped before any
/// higher-priority fact, matching the 80-column FINAL-5UX format.
pub fn line(snapshot: &TruthSnapshot, width: u16, now: SystemTime) -> TruthBarLine {
    let mut segments = vec![
        provider_model(snapshot),
        capability(snapshot),
        permission(snapshot),
        cache(snapshot),
        verification(snapshot, now),
    ];
    let compact = width < 100;
    if compact {
        segments.remove(3);
    }
    let delimiter = if compact { " · " } else { "  |  " };
    let mut labels: Vec<String> = segments
        .iter()
        .map(|segment| {
            if compact {
                segment.compact.clone()
            } else {
                segment.full.clone()
            }
        })
        .collect();
    let fixed_width = labels.iter().skip(1).map(|s| s.width()).sum::<usize>()
        + delimiter.width() * labels.len().saturating_sub(1);
    let provider_width = usize::from(width).saturating_sub(fixed_width).max(1);
    labels[0] = truncate_middle(&labels[0], provider_width);
    let mut text = labels.join(delimiter);
    if text.width() > usize::from(width) {
        text = truncate_middle(&text, usize::from(width));
    }
    TruthBarLine {
        text,
        tone: segments
            .iter()
            .map(|segment| segment.tone)
            .max()
            .unwrap_or(TruthTone::Passive),
        compact,
    }
}

/// Render the shared line and return its clickable `/status` hit rectangle.
pub fn render(
    buf: &mut Buffer,
    area: Rect,
    snapshot: &TruthSnapshot,
    now: SystemTime,
    theme: &Theme,
    hovered: bool,
) -> Option<Rect> {
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let rendered = line(snapshot, area.width, now);
    let fg = match rendered.tone {
        TruthTone::Passive => theme.gray,
        TruthTone::Success => theme.accent_success,
        TruthTone::Caution => theme.warning,
        TruthTone::Blocker => theme.accent_error,
    };
    let mut style = Style::default().fg(fg).bg(theme.bg_base);
    if hovered {
        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
    }
    buf.set_style(area, Style::default().bg(theme.bg_base));
    let used = rendered.text.width().min(usize::from(area.width)) as u16;
    buf.set_span(
        area.x,
        area.y,
        &Span::styled(rendered.text, style),
        area.width,
    );
    (used > 0).then_some(Rect::new(area.x, area.y, used, 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_contract::{
        CacheSummary, ProductIdentity, ProviderState, VerificationSummary, WorkPhase,
    };

    fn snapshot() -> TruthSnapshot {
        TruthSnapshot {
            product: ProductIdentity {
                display_name: "Lumen".to_owned(),
                version: "0.1.220-alpha.4".to_owned(),
                release_channel: "alpha".to_owned(),
            },
            provider: ProviderState::Ready {
                provider_id: "deepseek".to_owned(),
            },
            model: ModelState::Selected {
                model_id: "deepseek-chat".to_owned(),
            },
            capability: CapabilityState::ToolReady {
                fingerprint: "fp".to_owned(),
                checked_at: Some(SystemTime::UNIX_EPOCH),
                evidence_id: "probe-1".to_owned(),
            },
            permission: PermissionSummary::AskBeforeChanges,
            cache: CacheSummary {
                hit_ratio: Some(0.82),
                source: Some(CacheSource::ProviderReported),
            },
            verification: VerificationSummary::Passed {
                command: "cargo test".to_owned(),
                run_id: "run-1".to_owned(),
                finished_at: SystemTime::UNIX_EPOCH + Duration::from_secs(10),
                source_seq: 1,
            },
            phase: WorkPhase::Complete,
            captured_at: SystemTime::UNIX_EPOCH,
        }
    }

    #[test]
    fn standard_truth_bar_matches_spec_vocabulary() {
        let rendered = line(
            &snapshot(),
            120,
            SystemTime::UNIX_EPOCH + Duration::from_secs(22),
        );
        assert_eq!(
            rendered.text,
            "DeepSeek · deepseek-chat  |  ✓ Tool-ready  |  Ask before changes  |  Cache 82% hit  |  ✓ Verified 12s ago"
        );
        assert_eq!(rendered.tone, TruthTone::Success);
        assert!(!rendered.compact);
    }

    #[test]
    fn compact_truth_bar_keeps_four_load_bearing_facts() {
        let rendered = line(&snapshot(), 80, SystemTime::UNIX_EPOCH);
        assert_eq!(
            rendered.text,
            "DeepSeek/deepseek-chat · ✓ tools · Ask · ✓ verify"
        );
        assert!(rendered.compact);
        assert!(!rendered.text.contains("cache"));
        assert!(rendered.text.width() <= 80);
    }

    #[test]
    fn blocker_tone_wins_over_success_and_copy_names_failure() {
        let mut snap = snapshot();
        snap.capability = CapabilityState::ChatOnly {
            evidence_id: "probe-chat".to_owned(),
        };
        let rendered = line(&snap, 120, SystemTime::UNIX_EPOCH);
        assert_eq!(rendered.tone, TruthTone::Blocker);
        assert!(rendered.text.contains("✗ Chat-only"));
        assert!(rendered.text.contains("✓ Verified"));
    }

    #[test]
    fn stale_and_unknown_are_explicit_without_colour() {
        let mut snap = snapshot();
        snap.capability = CapabilityState::Unknown {
            reason: "not probed".to_owned(),
        };
        snap.verification = VerificationSummary::Stale {
            prior_run_id: "run-1".to_owned(),
            changed_at_seq: 2,
        };
        let rendered = line(&snap, 80, SystemTime::UNIX_EPOCH);
        assert!(rendered.text.contains("? capability unknown"));
        assert!(rendered.text.contains("! stale"));
        assert_eq!(rendered.tone, TruthTone::Caution);
    }

    #[test]
    fn estimated_cache_is_never_called_a_definitive_hit() {
        let mut snap = snapshot();
        snap.cache.source = Some(CacheSource::Estimated);
        let rendered = line(&snap, 120, SystemTime::UNIX_EPOCH);
        assert!(rendered.text.contains("Cache 82% estimated"));
        assert!(!rendered.text.contains("82% hit"));
    }

    #[test]
    fn very_narrow_width_is_never_exceeded() {
        let mut snap = snapshot();
        snap.provider = ProviderState::Ready {
            provider_id: "provider-with-a-very-long-name".to_owned(),
        };
        snap.model = ModelState::Selected {
            model_id: "model-with-a-very-long-name".to_owned(),
        };
        for width in [1, 20, 40, 80, 120, 180] {
            let rendered = line(&snap, width, SystemTime::UNIX_EPOCH);
            assert!(rendered.text.width() <= usize::from(width), "{rendered:?}");
        }
    }

    /// Fullscreen, minimal, and dashboard all call [`line`] on the same
    /// `TruthSnapshot` Arc — prove semantic identity across surface widths.
    #[test]
    fn cross_surface_same_snapshot_same_semantics() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(30);
        let snap = snapshot();
        // Typical surface widths: fullscreen status row ~120, compact/dashboard
        // often 80, minimal often narrow.
        let full = line(&snap, 120, now);
        let dashboard = line(&snap, 120, now);
        let minimal = line(&snap, 80, now);

        assert_eq!(full.text, dashboard.text);
        assert_eq!(full.tone, dashboard.tone);
        assert_eq!(full.tone, minimal.tone);
        assert!(full.text.contains("Tool-ready") || full.text.contains("✓ tools"));
        assert!(minimal.text.contains("✓ tools") || minimal.text.contains("Tool-ready"));
        // Chat-only must never look like tool success on any surface.
        let mut chat = snap;
        chat.capability = CapabilityState::ChatOnly {
            evidence_id: "e".into(),
        };
        for width in [40_u16, 80, 120, 180] {
            let rendered = line(&chat, width, now);
            assert_eq!(rendered.tone, TruthTone::Blocker);
            assert!(
                rendered.text.to_ascii_lowercase().contains("chat-only"),
                "width {width}: {}",
                rendered.text
            );
            assert!(!rendered.text.contains("Tool-ready"));
        }
    }

    #[test]
    fn pty_matrix_widths_keep_load_bearing_vocabulary() {
        let now = SystemTime::UNIX_EPOCH;
        let snap = snapshot();
        for (w, h) in [(80_u16, 24_u16), (120, 40), (180, 50)] {
            let _ = h; // height is a full-frame concern; bar is one row
            let rendered = line(&snap, w, now);
            assert!(
                rendered.text.width() <= usize::from(w),
                "{w}x{h}: {}",
                rendered.text
            );
            let lower = rendered.text.to_ascii_lowercase();
            assert!(
                lower.contains("tool") || lower.contains("verify") || lower.contains("ask"),
                "width {w}: missing load-bearing facts: {}",
                rendered.text
            );
        }
    }

    /// Gate F / E matrix: at FINAL-5UX sizes, semantics are carried in text
    /// (works under NO_COLOR / 16-colour) and match status recovery keywords.
    #[test]
    fn final5ux_size_matrix_semantics_are_colour_independent() {
        use crate::views::status_detail::redacted_report;
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(60);
        let cases: [(u16, u16); 3] = [(80, 24), (120, 40), (180, 50)];
        for capability in [
            CapabilityState::ToolReady {
                fingerprint: "fp".into(),
                checked_at: Some(SystemTime::UNIX_EPOCH),
                evidence_id: "e".into(),
            },
            CapabilityState::ChatOnly {
                evidence_id: "e".into(),
            },
            CapabilityState::Unknown {
                reason: "not probed".into(),
            },
        ] {
            let mut snap = snapshot();
            snap.capability = capability;
            let report = redacted_report(&snap, now);
            for (w, _h) in cases {
                let bar = line(&snap, w, now);
                assert!(bar.text.width() <= usize::from(w));
                // Text markers (not colour) encode state for low-colour terminals.
                match &snap.capability {
                    CapabilityState::ToolReady { .. } => {
                        assert!(
                            bar.text.contains("Tool-ready")
                                || bar.text.contains("✓ tools")
                                || bar.text.contains("tools"),
                            "{}",
                            bar.text
                        );
                    }
                    CapabilityState::ChatOnly { .. } => {
                        assert!(bar.text.to_ascii_lowercase().contains("chat-only"));
                        assert_eq!(bar.tone, TruthTone::Blocker);
                        assert!(report.to_ascii_lowercase().contains("chat-only"));
                        assert!(report.contains("Recovery"));
                    }
                    CapabilityState::Unknown { .. } => {
                        assert!(
                            bar.text.contains('?') || bar.text.to_ascii_lowercase().contains("unknown"),
                            "{}",
                            bar.text
                        );
                        assert!(report.contains("Recovery"));
                    }
                    _ => {}
                }
            }
        }
    }
}
