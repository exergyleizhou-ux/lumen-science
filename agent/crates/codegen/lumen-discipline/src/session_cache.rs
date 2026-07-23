//! Session-scoped cache tracker (surpasses Reasonix single-compare).
//!
//! Tracks rolling hit rate, consecutive stable prefixes, and last miss reasons
//! so status / truth bar / headless can show more than a one-shot ratio.

use crate::cache::{CacheUsage, format_cache_line, hit_ratio};
use crate::cache_shape::{
    CacheDiagnostics, PrefixChangeReason, PrefixShape, compare_shape, format_change_reasons,
};
use crate::provider_strategy::{CacheProfile, profile_for_model};

/// Rolling session cache state (host-owned; never inject into system prefix).
#[derive(Debug, Clone, Default)]
pub struct SessionCacheTracker {
    last_shape: Option<PrefixShape>,
    /// Cumulative provider-reported cache hits / prompt inputs (for session ratio).
    cum_hit: u64,
    cum_input: u64,
    /// Consecutive turns with unchanged prefix after cold start.
    stable_streak: u64,
    last_diag: Option<CacheDiagnostics>,
    model_id: String,
    base_url: Option<String>,
}

/// Snapshot for UI / status (no secrets).
#[derive(Debug, Clone, PartialEq)]
pub struct SessionCacheSnapshot {
    pub last_hit_ratio: Option<f64>,
    pub session_hit_ratio: Option<f64>,
    pub stable_streak: u64,
    pub last_change_reasons: String,
    pub prefix_stable: bool,
    pub cache_line: String,
    pub profile_label: &'static str,
    pub adaptation: &'static str,
    /// 0..=100 stability score: rewards long stable streaks, penalizes churn.
    pub stability_score: u8,
}

impl SessionCacheTracker {
    pub fn new(model_id: impl Into<String>, base_url: Option<String>) -> Self {
        Self {
            model_id: model_id.into(),
            base_url,
            ..Default::default()
        }
    }

    pub fn set_model(&mut self, model_id: impl Into<String>, base_url: Option<String>) {
        self.model_id = model_id.into();
        self.base_url = base_url;
        // Model switch invalidates prefix cache continuity.
        self.last_shape = None;
        self.stable_streak = 0;
    }

    pub fn profile(&self) -> CacheProfile {
        profile_for_model(&self.model_id, self.base_url.as_deref())
    }

    /// Observe a turn: new prefix shape + provider usage for that call.
    pub fn observe(
        &mut self,
        shape: PrefixShape,
        prompt_tokens: u64,
        cache_hit_tokens: u64,
    ) -> CacheDiagnostics {
        let miss = prompt_tokens.saturating_sub(cache_hit_tokens);
        let diag = compare_shape(self.last_shape.as_ref(), &shape, cache_hit_tokens, miss);
        if diag.prefix_changed {
            // Cold start does not count as a "break" for streak after first stable.
            if !diag
                .change_reasons
                .iter()
                .all(|r| matches!(r, PrefixChangeReason::ColdStart))
            {
                self.stable_streak = 0;
            }
        } else {
            self.stable_streak = self.stable_streak.saturating_add(1);
        }
        self.cum_hit = self.cum_hit.saturating_add(cache_hit_tokens);
        self.cum_input = self.cum_input.saturating_add(prompt_tokens.max(cache_hit_tokens));
        self.last_shape = Some(shape);
        self.last_diag = Some(diag.clone());
        diag
    }

    pub fn snapshot(&self, last_prompt: u64, last_hit: u64, last_output: u64) -> SessionCacheSnapshot {
        let profile = self.profile();
        let last_ratio = hit_ratio(last_prompt, last_hit);
        let session_ratio = hit_ratio(self.cum_input, self.cum_hit);
        let reasons = self
            .last_diag
            .as_ref()
            .map(|d| format_change_reasons(&d.change_reasons))
            .unwrap_or_else(|| "n/a".into());
        let prefix_stable = self
            .last_diag
            .as_ref()
            .is_some_and(|d| !d.prefix_changed);
        SessionCacheSnapshot {
            last_hit_ratio: last_ratio,
            session_hit_ratio: session_ratio,
            stable_streak: self.stable_streak,
            last_change_reasons: reasons,
            prefix_stable,
            cache_line: format_cache_line(CacheUsage {
                input_tokens: last_prompt,
                cache_read_tokens: last_hit,
                output_tokens: last_output,
            }),
            profile_label: profile.label,
            adaptation: profile.adaptation,
            stability_score: stability_score(self.stable_streak, self.cum_input, self.cum_hit),
        }
    }
}

/// Combine streak + session hit into a 0–100 score (Lumen-only, beyond Reasonix).
fn stability_score(streak: u64, cum_input: u64, cum_hit: u64) -> u8 {
    let streak_part = (streak.min(20) as f64 / 20.0) * 40.0;
    let hit_part = hit_ratio(cum_input, cum_hit).unwrap_or(0.0) * 60.0;
    (streak_part + hit_part).round().clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache_shape::capture_shape;

    #[test]
    fn stable_turns_raise_streak_and_score() {
        let mut t = SessionCacheTracker::new("deepseek-chat", None);
        let shape = capture_shape("sys", "[]", 0);
        t.observe(shape.clone(), 1000, 0); // cold
        t.observe(shape.clone(), 2000, 1500);
        t.observe(shape, 3000, 2500);
        assert_eq!(t.stable_streak, 2);
        let snap = t.snapshot(3000, 2500, 100);
        assert!(snap.stability_score > 40, "score={}", snap.stability_score);
        assert!(snap.session_hit_ratio.unwrap_or(0.0) > 0.5);
    }

    #[test]
    fn system_churn_resets_streak() {
        let mut t = SessionCacheTracker::new("deepseek-chat", None);
        let a = capture_shape("sys-a", "[]", 0);
        let b = capture_shape("sys-b", "[]", 0);
        t.observe(a, 1000, 800);
        t.observe(b, 1000, 0);
        assert_eq!(t.stable_streak, 0);
    }
}
