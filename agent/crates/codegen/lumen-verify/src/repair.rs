//! Repair cycle state machine.

use super::config::Config;
use std::collections::HashMap;
use std::path::PathBuf;

/// Identity for one repair budget. Failures never cross session or file
/// boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepairKey {
    pub session_id: String,
    pub file: PathBuf,
}

impl RepairKey {
    pub fn new(session_id: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self {
            session_id: session_id.into(),
            file: file.into(),
        }
    }
}

/// Decision made before an automatic verification command is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairPermit {
    Verify { attempt: u32, max_repair: u32 },
    Blocked { failures: u32, max_repair: u32 },
}

/// Tracks consecutive verify failures by `(session, file)`.
///
/// A PASS removes the key, so the next failure starts at attempt 1. The
/// tracker is intentionally in-memory and session-scoped; restarting a
/// session starts a fresh repair budget.
#[derive(Debug, Default)]
pub struct RepairTracker {
    failures: HashMap<RepairKey, u32>,
}

impl RepairTracker {
    pub fn permit(&self, key: &RepairKey, cfg: &Config) -> RepairPermit {
        let failures = self.failures(key);
        if failures >= cfg.max_repair {
            RepairPermit::Blocked {
                failures,
                max_repair: cfg.max_repair,
            }
        } else {
            RepairPermit::Verify {
                attempt: failures + 1,
                max_repair: cfg.max_repair,
            }
        }
    }

    pub fn record_failure(&mut self, key: &RepairKey) -> u32 {
        let failures = self.failures.entry(key.clone()).or_default();
        *failures = failures.saturating_add(1);
        *failures
    }

    pub fn record_pass(&mut self, key: &RepairKey) {
        self.failures.remove(key);
    }

    pub fn failures(&self, key: &RepairKey) -> u32 {
        self.failures.get(key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_three_failures_are_allowed_and_fourth_is_blocked() {
        let cfg = Config::default();
        let mut tracker = RepairTracker::default();
        let key = RepairKey::new("session-a", "/workspace/main.go");

        for expected_attempt in 1..=3 {
            assert_eq!(
                tracker.permit(&key, &cfg),
                RepairPermit::Verify {
                    attempt: expected_attempt,
                    max_repair: 3,
                }
            );
            assert_eq!(tracker.record_failure(&key), expected_attempt);
        }
        assert_eq!(
            tracker.permit(&key, &cfg),
            RepairPermit::Blocked {
                failures: 3,
                max_repair: 3,
            }
        );
    }

    #[test]
    fn pass_resets_consecutive_failure_budget() {
        let cfg = Config::default();
        let mut tracker = RepairTracker::default();
        let key = RepairKey::new("session-a", "/workspace/main.go");

        tracker.record_failure(&key);
        tracker.record_failure(&key);
        assert_eq!(tracker.failures(&key), 2);
        tracker.record_pass(&key);

        assert_eq!(tracker.failures(&key), 0);
        assert_eq!(
            tracker.permit(&key, &cfg),
            RepairPermit::Verify {
                attempt: 1,
                max_repair: 3,
            }
        );
    }

    #[test]
    fn failure_budgets_are_isolated_by_session_and_file() {
        let cfg = Config::default();
        let mut tracker = RepairTracker::default();
        let a_main = RepairKey::new("session-a", "/workspace/main.go");
        let a_other = RepairKey::new("session-a", "/workspace/other.go");
        let b_main = RepairKey::new("session-b", "/workspace/main.go");

        for _ in 0..3 {
            tracker.record_failure(&a_main);
        }

        assert!(matches!(
            tracker.permit(&a_main, &cfg),
            RepairPermit::Blocked { .. }
        ));
        assert!(matches!(
            tracker.permit(&a_other, &cfg),
            RepairPermit::Verify { attempt: 1, .. }
        ));
        assert!(matches!(
            tracker.permit(&b_main, &cfg),
            RepairPermit::Verify { attempt: 1, .. }
        ));
    }
}
