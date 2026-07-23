//! Cache usage formatting for status / headless (DeepSeek-friendly, multi-provider safe).

/// Token usage slice relevant to prompt cache visibility.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheUsage {
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub output_tokens: u64,
}

/// Hit ratio in 0.0..=1.0, or None when input is zero / unknown.
pub fn hit_ratio(input_tokens: u64, cache_read_tokens: u64) -> Option<f64> {
    let input = input_tokens.max(cache_read_tokens);
    if input == 0 {
        return None;
    }
    Some((cache_read_tokens as f64 / input as f64).clamp(0.0, 1.0))
}

/// One-line summary for status bar / session-info.
///
/// Example: `cache 12.4k/50.0k (24%) · out 800`
pub fn format_cache_line(u: CacheUsage) -> String {
    let cached = u.cache_read_tokens;
    let input = u.input_tokens.max(cached);
    let pct = hit_ratio(u.input_tokens, u.cache_read_tokens)
        .map(|r| (r * 100.0).round() as u64)
        .unwrap_or(0);
    format!(
        "cache {}/{} ({}%) · out {}",
        fmt_k(cached),
        fmt_k(input),
        pct,
        fmt_k(u.output_tokens)
    )
}

/// Extended line with miss reasons + profile (beyond Reasonix status one-liner).
pub fn format_cache_line_rich(
    u: CacheUsage,
    change_reasons: &str,
    profile_label: &str,
    stability_score: u8,
) -> String {
    format!(
        "{} · {} · prefix={} · stability={}",
        format_cache_line(u),
        profile_label,
        change_reasons,
        stability_score
    )
}

fn fmt_k(n: u64) -> String {
    if n >= 1000 {
        format!("{:.1}k", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_shows_percent() {
        let line = format_cache_line(CacheUsage {
            input_tokens: 50_000,
            cache_read_tokens: 12_400,
            output_tokens: 800,
        });
        assert!(line.contains("cache"), "{line}");
        assert!(line.contains("%"), "{line}");
        assert!(line.contains("out"), "{line}");
    }

    #[test]
    fn zero_input_no_div0() {
        let line = format_cache_line(CacheUsage::default());
        assert!(line.contains("(0%)"), "{line}");
        assert_eq!(hit_ratio(0, 0), None);
    }

    #[test]
    fn thousands_use_one_decimal_without_duplicate_ranges() {
        assert_eq!(fmt_k(999), "999");
        assert_eq!(fmt_k(1_000), "1.0k");
        assert_eq!(fmt_k(10_000), "10.0k");
    }

    #[test]
    fn pure_cache_hit_ratio_is_one() {
        // Anthropic-style: input_tokens=0, cache_read>0
        assert_eq!(hit_ratio(0, 2500), Some(1.0));
    }

    #[test]
    fn rich_line_includes_profile() {
        let line = format_cache_line_rich(
            CacheUsage {
                input_tokens: 10_000,
                cache_read_tokens: 8_000,
                output_tokens: 50,
            },
            "stable",
            "DeepSeek auto-prefix",
            88,
        );
        assert!(line.contains("DeepSeek"), "{line}");
        assert!(line.contains("stability=88"), "{line}");
    }
}
