//! Lumen loop discipline (pure logic, host-owned state).
//!
//! - [`StormBreaker`] — same tool + error signature fails ≥ N → nudge/stop
//! - [`RepeatSuccessGuard`] — same tool + args hash succeeds ≥ N → nudge
//! - [`DeliverySessionState`] + [`gate_goal_complete`] — anti fake-complete
//! - Prompt-cache stack (Reasonix-class, DeepSeek-first, multi-provider):
//!   - [`format_cache_line`] / [`hit_ratio`]
//!   - [`capture_shape`] / [`compare_shape`] — prefix miss diagnostics
//!   - [`profile_for_model`] — adaptation matrix
//!   - [`SessionCacheTracker`] — rolling hit + stability score
//!
//! **Never** put this state into the system-prompt *prefix* (breaks DeepSeek
//! automatic prefix cache and every AutomaticPrefix provider). Inject only as
//! turn-tail reminders or tool results.

mod cache;
mod cache_shape;
mod delivery;
mod provider_strategy;
mod request_prefix;
mod session_cache;
mod storm;

pub use cache::{CacheUsage, format_cache_line, format_cache_line_rich, hit_ratio};
pub use cache_shape::{
    CacheDiagnostics, PrefixChangeReason, PrefixShape, capture_shape, compare_shape,
    estimate_tokens, format_change_reasons,
};
pub use delivery::{
    DELIVERY_REMINDER, DeliveryAction, DeliverySessionState, DeliveryStrictness, GoalGate,
    GoalIncompletePolicy, TodoSnapshot, gate_goal_complete, on_turn_end,
};
pub use provider_strategy::{
    CacheMechanism, CacheProfile, CacheValue, allows_definitive_display, profile_for_model,
};
pub use request_prefix::{join_system_texts, shape_from_parts, tools_fingerprint_json};
pub use session_cache::{SessionCacheSnapshot, SessionCacheTracker};
pub use storm::{
    RepeatSuccessAction, RepeatSuccessGuard, StormAction, StormBreaker, error_signature,
    hash_tool_args,
};
