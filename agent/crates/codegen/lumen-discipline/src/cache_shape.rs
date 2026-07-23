//! Prefix shape capture & miss diagnostics (Reasonix cache_shape, upgraded).
//!
//! DeepSeek (and similar automatic-prefix providers) reuse the **byte-stable**
//! system + tools prefix across turns. Comparing shapes explains *why* a cache
//! miss happened instead of only reporting hit/miss tokens.

use sha2::{Digest, Sha256};

/// Snapshot of the cache-stable request prefix.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrefixShape {
    pub system_hash: String,
    pub tools_hash: String,
    pub prefix_hash: String,
    /// Host-side log rewrite / compaction generation; bumps invalidate prefix.
    pub log_rewrite_version: u64,
    /// Rough tools-schema token estimate (bytes/4).
    pub tool_schema_tokens: u64,
}

/// Why the prefix changed between two turns (Reasonix + Lumen extras).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefixChangeReason {
    System,
    Tools,
    LogRewrite,
    /// First turn of a session (no previous shape).
    ColdStart,
}

impl PrefixChangeReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Tools => "tools",
            Self::LogRewrite => "log_rewrite",
            Self::ColdStart => "cold_start",
        }
    }
}

/// Diagnostics for one turn's prefix + provider cache tokens.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CacheDiagnostics {
    pub prefix_hash: String,
    pub prefix_changed: bool,
    pub change_reasons: Vec<PrefixChangeReason>,
    pub system_hash: String,
    pub tools_hash: String,
    pub log_rewrite_version: u64,
    pub tool_schema_tokens: u64,
    pub cache_hit_tokens: u64,
    pub cache_miss_tokens: u64,
}

fn short_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    // 8 hex bytes — enough for diagnostics, cheap to log.
    digest
        .iter()
        .take(8)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Capture prefix shape from system prompt text and a tools JSON blob
/// (already serialized tool schemas). Tool order is caller-normalized.
pub fn capture_shape(
    system_prompt: &str,
    tools_json: &str,
    log_rewrite_version: u64,
) -> PrefixShape {
    let system_hash = short_hash(system_prompt.as_bytes());
    let tools_hash = short_hash(tools_json.as_bytes());
    let mut prefix = Vec::with_capacity(system_prompt.len() + tools_json.len() + 1);
    prefix.extend_from_slice(system_prompt.as_bytes());
    prefix.push(0);
    prefix.extend_from_slice(tools_json.as_bytes());
    let prefix_hash = short_hash(&prefix);
    let tool_schema_tokens = estimate_tokens(tools_json);
    PrefixShape {
        system_hash,
        tools_hash,
        prefix_hash,
        log_rewrite_version,
        tool_schema_tokens,
    }
}

/// Rough token estimate from UTF-8 length (~4 bytes/token for schema JSON).
pub fn estimate_tokens(s: &str) -> u64 {
    if s.is_empty() {
        0
    } else {
        (s.len() as u64).div_ceil(4)
    }
}

/// Compare previous vs current shape; fold optional provider usage.
///
/// `cache_hit_tokens` / `cache_miss_tokens` come from the provider when known.
/// If only hit + total input are known, pass `cache_miss = input.saturating_sub(hit)`.
pub fn compare_shape(
    prev: Option<&PrefixShape>,
    cur: &PrefixShape,
    cache_hit_tokens: u64,
    cache_miss_tokens: u64,
) -> CacheDiagnostics {
    let mut reasons = Vec::new();
    let prefix_changed = match prev {
        None => {
            reasons.push(PrefixChangeReason::ColdStart);
            true
        }
        Some(p) => {
            if p.system_hash != cur.system_hash {
                reasons.push(PrefixChangeReason::System);
            }
            if p.tools_hash != cur.tools_hash {
                reasons.push(PrefixChangeReason::Tools);
            }
            if p.log_rewrite_version != cur.log_rewrite_version {
                reasons.push(PrefixChangeReason::LogRewrite);
            }
            !reasons.is_empty()
        }
    };
    CacheDiagnostics {
        prefix_hash: cur.prefix_hash.clone(),
        prefix_changed,
        change_reasons: reasons,
        system_hash: cur.system_hash.clone(),
        tools_hash: cur.tools_hash.clone(),
        log_rewrite_version: cur.log_rewrite_version,
        tool_schema_tokens: cur.tool_schema_tokens,
        cache_hit_tokens,
        cache_miss_tokens,
    }
}

/// Human-readable miss reasons for status / logs.
pub fn format_change_reasons(reasons: &[PrefixChangeReason]) -> String {
    if reasons.is_empty() {
        return "stable".to_string();
    }
    reasons
        .iter()
        .map(|r| r.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_prefix_is_stable() {
        let a = capture_shape("sys", r#"[{"name":"t"}]"#, 0);
        let b = capture_shape("sys", r#"[{"name":"t"}]"#, 0);
        assert_eq!(a.prefix_hash, b.prefix_hash);
        let d = compare_shape(Some(&a), &b, 1000, 100);
        assert!(!d.prefix_changed);
        assert!(d.change_reasons.is_empty());
    }

    #[test]
    fn system_change_detected() {
        let a = capture_shape("sys-a", "[]", 0);
        let b = capture_shape("sys-b", "[]", 0);
        let d = compare_shape(Some(&a), &b, 0, 500);
        assert!(d.prefix_changed);
        assert!(d.change_reasons.contains(&PrefixChangeReason::System));
    }

    #[test]
    fn tools_change_detected() {
        let a = capture_shape("sys", r#"[{"name":"a"}]"#, 0);
        let b = capture_shape("sys", r#"[{"name":"b"}]"#, 0);
        let d = compare_shape(Some(&a), &b, 0, 500);
        assert!(d.change_reasons.contains(&PrefixChangeReason::Tools));
    }

    #[test]
    fn log_rewrite_detected() {
        let a = capture_shape("sys", "[]", 1);
        let b = capture_shape("sys", "[]", 2);
        let d = compare_shape(Some(&a), &b, 10, 10);
        assert!(d.change_reasons.contains(&PrefixChangeReason::LogRewrite));
    }

    #[test]
    fn cold_start_marked() {
        let b = capture_shape("sys", "[]", 0);
        let d = compare_shape(None, &b, 0, 100);
        assert!(d.change_reasons.contains(&PrefixChangeReason::ColdStart));
    }
}
