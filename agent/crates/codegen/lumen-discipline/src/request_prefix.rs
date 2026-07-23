//! Extract cache-stable prefix material from a conversation-shaped request.
//!
//! Hosts pass system text + tools JSON; this keeps hashing rules in one place.

use crate::cache_shape::{PrefixShape, capture_shape};

/// Concatenate system-role message bodies in order (stable prefix core).
pub fn join_system_texts<'a, I>(systems: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut out = String::new();
    for s in systems {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(s);
    }
    out
}

/// Canonical tools JSON for hashing: array of `{name, parameters}` sorted by name.
pub fn tools_fingerprint_json(tools: &[(String, String)]) -> String {
    // tools: (name, parameters_json)
    let mut sorted = tools.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let arr: Vec<serde_json::Value> = sorted
        .into_iter()
        .map(|(name, params)| {
            let params_val: serde_json::Value =
                serde_json::from_str(&params).unwrap_or(serde_json::Value::Null);
            serde_json::json!({ "name": name, "parameters": params_val })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
}

/// Capture shape from system text + tool name/params pairs.
pub fn shape_from_parts(
    system_text: &str,
    tools: &[(String, String)],
    log_rewrite_version: u64,
) -> PrefixShape {
    let tools_json = tools_fingerprint_json(tools);
    capture_shape(system_text, &tools_json, log_rewrite_version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_order_does_not_change_hash() {
        let a = tools_fingerprint_json(&[
            ("b".into(), "{}".into()),
            ("a".into(), "{}".into()),
        ]);
        let b = tools_fingerprint_json(&[
            ("a".into(), "{}".into()),
            ("b".into(), "{}".into()),
        ]);
        assert_eq!(a, b);
    }

    #[test]
    fn join_systems() {
        assert_eq!(join_system_texts(["one", "two"]), "one\ntwo");
    }
}
