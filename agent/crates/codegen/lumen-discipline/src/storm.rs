//! Storm breaker + repeat-success guard.

use sha2::{Digest, Sha256};

/// Normalize error text to a short signature (first N chars, collapsed space).
pub fn error_signature(err: &str, max_chars: usize) -> String {
    let collapsed: String = err.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(max_chars).collect()
}

/// Stable hash of tool arguments (JSON string or raw).
pub fn hash_tool_args(args: &str) -> String {
    let mut h = Sha256::new();
    h.update(args.as_bytes());
    format!("{:x}", h.finalize())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StormAction {
    /// Inject model-visible reminder to change strategy.
    Nudge(String),
    /// Hard-stop further same-tool retries this batch.
    StopBatch(String),
}

/// Tracks consecutive failures of the same tool + error signature.
#[derive(Debug, Clone)]
pub struct StormBreaker {
    last_sig: Option<(String, String)>, // (tool, err_sig)
    count: u32,
    pub threshold: u32,
    pub stop_after_nudge: bool,
}

impl Default for StormBreaker {
    fn default() -> Self {
        Self {
            last_sig: None,
            count: 0,
            threshold: 3,
            stop_after_nudge: false,
        }
    }
}

impl StormBreaker {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            ..Default::default()
        }
    }

    /// Call after a tool **success** (resets storm for that tool family).
    pub fn on_tool_success(&mut self, tool: &str) {
        if self
            .last_sig
            .as_ref()
            .is_some_and(|(t, _)| t == tool)
        {
            self.last_sig = None;
            self.count = 0;
        }
    }

    /// Call after a tool **error**. Returns action when threshold hit.
    pub fn on_tool_error(&mut self, tool: &str, err: &str) -> Option<StormAction> {
        let sig = error_signature(err, 120);
        let key = (tool.to_owned(), sig);
        if self.last_sig.as_ref() == Some(&key) {
            self.count = self.count.saturating_add(1);
        } else {
            self.last_sig = Some(key);
            self.count = 1;
        }
        if self.count >= self.threshold {
            let msg = format!(
                "<storm-breaker>\nTool `{tool}` failed {n} times with the same error signature. \
                 Stop retrying the same call. Change strategy (different tool, smaller scope, \
                 or ask the user).\n</storm-breaker>",
                n = self.count
            );
            if self.stop_after_nudge {
                return Some(StormAction::StopBatch(msg));
            }
            return Some(StormAction::Nudge(msg));
        }
        None
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepeatSuccessAction {
    Nudge(String),
}

/// Same tool + args hash succeeding repeatedly → "don't thrash".
#[derive(Debug, Clone)]
pub struct RepeatSuccessGuard {
    last: Option<(String, String)>, // (tool, args_hash)
    count: u32,
    pub threshold: u32,
}

impl Default for RepeatSuccessGuard {
    fn default() -> Self {
        Self {
            last: None,
            count: 0,
            threshold: 3,
        }
    }
}

impl RepeatSuccessGuard {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold: threshold.max(1),
            ..Default::default()
        }
    }

    pub fn on_tool_success(&mut self, tool: &str, args: &str) -> Option<RepeatSuccessAction> {
        let h = hash_tool_args(args);
        let key = (tool.to_owned(), h);
        if self.last.as_ref() == Some(&key) {
            self.count = self.count.saturating_add(1);
        } else {
            self.last = Some(key);
            self.count = 1;
        }
        if self.count >= self.threshold {
            return Some(RepeatSuccessAction::Nudge(format!(
                "<repeat-success>\nTool `{tool}` succeeded {n} times with identical arguments. \
                 Avoid meaningless repetition; move on or change inputs.\n</repeat-success>",
                n = self.count
            )));
        }
        None
    }

    pub fn on_tool_error(&mut self, _tool: &str) {
        self.last = None;
        self.count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storm_nudge_on_third_same_error() {
        let mut s = StormBreaker::new(3);
        assert!(s.on_tool_error("bash", "exit 1: foo").is_none());
        assert!(s.on_tool_error("bash", "exit 1: foo").is_none());
        let a = s.on_tool_error("bash", "exit 1: foo").expect("nudge");
        match a {
            StormAction::Nudge(m) => assert!(m.contains("storm-breaker")),
            _ => panic!("expected Nudge"),
        }
    }

    #[test]
    fn storm_resets_on_different_error() {
        let mut s = StormBreaker::new(3);
        assert!(s.on_tool_error("bash", "err-a").is_none());
        assert!(s.on_tool_error("bash", "err-a").is_none());
        // different error → reset
        assert!(s.on_tool_error("bash", "err-b").is_none());
        assert_eq!(s.count(), 1);
    }

    #[test]
    fn storm_success_clears() {
        let mut s = StormBreaker::new(3);
        s.on_tool_error("bash", "e");
        s.on_tool_error("bash", "e");
        s.on_tool_success("bash");
        assert_eq!(s.count(), 0);
    }

    #[test]
    fn repeat_success_nudge() {
        let mut g = RepeatSuccessGuard::new(3);
        let args = r#"{"path":"a.rs"}"#;
        assert!(g.on_tool_success("read_file", args).is_none());
        assert!(g.on_tool_success("read_file", args).is_none());
        let a = g.on_tool_success("read_file", args).expect("nudge");
        match a {
            RepeatSuccessAction::Nudge(m) => assert!(m.contains("repeat-success")),
        }
    }

    #[test]
    fn repeat_success_resets_on_path_change() {
        let mut g = RepeatSuccessGuard::new(3);
        assert!(g.on_tool_success("read_file", r#"{"path":"a"}"#).is_none());
        assert!(g.on_tool_success("read_file", r#"{"path":"a"}"#).is_none());
        assert!(g.on_tool_success("read_file", r#"{"path":"b"}"#).is_none());
        assert_eq!(g.count, 1);
    }
}
