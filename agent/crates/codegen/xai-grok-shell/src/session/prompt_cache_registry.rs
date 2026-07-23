//! Process-wide session prompt-cache trackers (no SessionActor field churn).
//!
//! Tracks prefix shape + rolling hit rate per session id. Never injects into
//! the system prompt — diagnostics only.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use lumen_discipline::{
    PrefixShape, SessionCacheSnapshot, SessionCacheTracker, shape_from_parts,
};

static REGISTRY: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();

struct Entry {
    model_id: String,
    tracker: SessionCacheTracker,
    log_rewrite_version: u64,
    last_snap: Option<SessionCacheSnapshot>,
}

fn map() -> &'static Mutex<HashMap<String, Entry>> {
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn entry_mut<'a>(
    guard: &'a mut HashMap<String, Entry>,
    session_id: &str,
    model_id: &str,
    base_url: Option<&str>,
) -> &'a mut Entry {
    guard
        .entry(session_id.to_string())
        .and_modify(|e| {
            if e.model_id != model_id {
                e.model_id = model_id.to_string();
                e.tracker
                    .set_model(model_id.to_string(), base_url.map(str::to_string));
            }
        })
        .or_insert_with(|| Entry {
            model_id: model_id.to_string(),
            tracker: SessionCacheTracker::new(model_id.to_string(), base_url.map(str::to_string)),
            log_rewrite_version: 0,
            last_snap: None,
        })
}

/// Compaction / history rewrite invalidates automatic prefix cache.
pub fn bump_log_rewrite(session_id: &str) {
    let Ok(mut guard) = map().lock() else {
        return;
    };
    if let Some(e) = guard.get_mut(session_id) {
        e.log_rewrite_version = e.log_rewrite_version.saturating_add(1);
    } else {
        // Seed so the next observe sees a non-zero rewrite if session was new.
        guard.insert(
            session_id.to_string(),
            Entry {
                model_id: "unknown".into(),
                tracker: SessionCacheTracker::new("unknown", None),
                log_rewrite_version: 1,
                last_snap: None,
            },
        );
    }
}

/// Observe one model call: prefix shape from system+tools, then usage.
pub fn observe_call(
    session_id: &str,
    model_id: &str,
    base_url: Option<&str>,
    system_text: &str,
    tools: &[(String, String)],
    prompt_tokens: u64,
    cache_hit_tokens: u64,
    output_tokens: u64,
) -> SessionCacheSnapshot {
    let Ok(mut guard) = map().lock() else {
        return empty_snap(model_id, base_url, prompt_tokens, cache_hit_tokens, output_tokens);
    };
    let e = entry_mut(&mut guard, session_id, model_id, base_url);
    let shape: PrefixShape =
        shape_from_parts(system_text, tools, e.log_rewrite_version);
    let diag = e.tracker.observe(shape, prompt_tokens, cache_hit_tokens);
    if diag.prefix_changed && !diag.change_reasons.is_empty() {
        tracing::debug!(
            session_id,
            model_id,
            reasons = %lumen_discipline::format_change_reasons(&diag.change_reasons),
            hit = cache_hit_tokens,
            prompt = prompt_tokens,
            "prompt cache prefix diagnostics"
        );
    }
    let snap = e
        .tracker
        .snapshot(prompt_tokens, cache_hit_tokens, output_tokens);
    e.last_snap = Some(snap.clone());
    snap
}

/// Last snapshot for session/info and mid-turn meta (if any).
pub fn last_snapshot(session_id: &str) -> Option<SessionCacheSnapshot> {
    let Ok(guard) = map().lock() else {
        return None;
    };
    guard.get(session_id).and_then(|e| e.last_snap.clone())
}

/// Drop tracker when session ends (best-effort).
pub fn drop_session(session_id: &str) {
    if let Ok(mut guard) = map().lock() {
        guard.remove(session_id);
    }
}

fn empty_snap(
    model_id: &str,
    base_url: Option<&str>,
    prompt: u64,
    hit: u64,
    out: u64,
) -> SessionCacheSnapshot {
    let mut t = SessionCacheTracker::new(model_id, base_url.map(str::to_string));
    t.snapshot(prompt, hit, out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_stable_prefix_raises_streak() {
        let sid = "test-cache-session-stable";
        drop_session(sid);
        let tools = vec![("read".into(), "{}".into())];
        observe_call(sid, "deepseek-chat", None, "sys", &tools, 1000, 0, 10);
        let s2 = observe_call(sid, "deepseek-chat", None, "sys", &tools, 2000, 1500, 10);
        assert!(s2.stable_streak >= 1);
        drop_session(sid);
    }
}
