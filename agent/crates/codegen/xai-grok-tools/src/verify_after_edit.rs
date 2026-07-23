//! Automatic verification feedback for successful writer tool results.
//!
//! This layer intentionally changes only the model-facing `prompt_text`; the
//! structured tool output remains the exact edit result used by ACP, hunk
//! tracking, and persistence.

use crate::types::output::{SearchReplaceOutput, ToolOutput};
use lumen_verify::repair::{RepairKey, RepairPermit, RepairTracker};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_FEEDBACK_CHARS: usize = 16_000;

/// Ephemeral verify state owned by one finalized tool session.
///
/// The tracker still keys by session ID and file so shared/child session
/// contexts cannot consume each other's repair budgets. It is deliberately
/// not persisted: a new session starts with a fresh finite repair budget.
#[derive(Clone, Default)]
pub(crate) struct VerifyAfterEditState {
    tracker: Arc<Mutex<RepairTracker>>,
    config_override: Option<lumen_verify::config::Config>,
}

#[cfg(test)]
impl VerifyAfterEditState {
    pub(crate) fn with_config(config: lumen_verify::config::Config) -> Self {
        Self {
            tracker: Arc::default(),
            config_override: Some(config),
        }
    }
}

pub(crate) async fn feedback_for_output(
    workspace_root: PathBuf,
    session_id: String,
    state: VerifyAfterEditState,
    output: &ToolOutput,
) -> Option<String> {
    let changed_file = match output {
        ToolOutput::SearchReplace(SearchReplaceOutput::EditsApplied(applied)) => {
            &applied.absolute_path
        }
        _ => return None,
    };
    if changed_file.extension().and_then(|ext| ext.to_str()) != Some("go") {
        return None;
    }

    let cfg = match state
        .config_override
        .clone()
        .map(Ok)
        .unwrap_or_else(load_user_verify_config)
    {
        Ok(cfg) => cfg,
        Err(error) => {
            return Some(truncate_feedback(format!(
                "[verify-after-edit] ERROR: user [verify] configuration could not be loaded safely: {error:#}. \
                 No verification command was run; treat this edit as unverified."
            )));
        }
    };
    feedback_for_output_with_config(workspace_root, session_id, state, output, cfg).await
}

async fn feedback_for_output_with_config(
    workspace_root: PathBuf,
    session_id: String,
    state: VerifyAfterEditState,
    output: &ToolOutput,
    cfg: lumen_verify::config::Config,
) -> Option<String> {
    let changed_file = match output {
        ToolOutput::SearchReplace(SearchReplaceOutput::EditsApplied(applied)) => {
            applied.absolute_path.clone()
        }
        _ => return None,
    };

    if !cfg.enabled || changed_file.extension().and_then(|ext| ext.to_str()) != Some("go") {
        return None;
    }

    let changed_file = if changed_file.is_absolute() {
        changed_file
    } else {
        workspace_root.join(changed_file)
    };
    let repair_file = dunce::canonicalize(&changed_file).unwrap_or_else(|_| changed_file.clone());
    let repair_key = RepairKey::new(session_id, repair_file);
    // Serialize verify decisions within a session so concurrent writer results
    // cannot both consume the same numbered attempt. The Resources lock is not
    // held while commands run.
    let mut tracker = state.tracker.lock().await;
    let (attempt, max_repair) = match tracker.permit(&repair_key, &cfg) {
        RepairPermit::Verify {
            attempt,
            max_repair,
        } => (attempt, max_repair),
        RepairPermit::Blocked {
            failures,
            max_repair,
        } => {
            return Some(format!(
                "[verify-after-edit] BLOCKED: automatic verification stopped after {failures}/{max_repair} \
                 consecutive failed attempts for this file in this session. No verification command was run. \
                 Report the blocker and use an explicit manual check or a new session after changing strategy."
            ));
        }
    };

    let outcome = tokio::task::spawn_blocking(move || {
        lumen_verify::run_after_edit(&workspace_root, &changed_file, &cfg)
    })
    .await;

    let feedback = match outcome {
        Ok(Ok(None)) => return None,
        Ok(Ok(Some(result))) if result.step_results.iter().all(|step| step.skipped) => {
            "[verify-after-edit] SKIPPED: no allowed Go verifier executable was available on PATH. The edit remains unverified.".to_string()
        }
        Ok(Ok(Some(result))) if result.ok => {
            tracker.record_pass(&repair_key);
            let ran = result
                .step_results
                .iter()
                .filter(|step| !step.skipped)
                .count();
            let skipped = result.step_results.len().saturating_sub(ran);
            if skipped == 0 {
                format!(
                    "[verify-after-edit] PASS: {ran} fixed Go build/vet/test step(s) passed after this edit."
                )
            } else {
                format!(
                    "[verify-after-edit] PASS: {ran} fixed Go verification step(s) passed; {skipped} unavailable step(s) were skipped."
                )
            }
        }
        Ok(Ok(Some(result))) => {
            let failures = tracker.record_failure(&repair_key);
            let diagnostics = lumen_verify::format_diagnostics(&result.step_results);
            let mut feedback = format!(
                "[verify-after-edit] FAILED (attempt {attempt}/{max_repair}): automatic Go build/vet/test found errors after this edit.\n\
                 {diagnostics}"
            );
            if failures >= max_repair {
                feedback.push_str(
                    "\n[verify-after-edit] BLOCKED: repair limit reached. Stop the automatic repair loop; subsequent edits to this file in this session will not run automatic verification. Report the unresolved diagnostics.",
                );
            } else {
                feedback.push_str(
                    "\nFix these diagnostics and edit again; the next successful write will verify again.",
                );
            }
            feedback
        }
        Ok(Err(error)) => format!(
            "[verify-after-edit] ERROR: automatic verification could not run safely: {error:#}. \
             Treat this edit as unverified and report the blocker if it cannot be corrected."
        ),
        Err(error) => format!(
            "[verify-after-edit] ERROR: verifier worker failed: {error}. \
             Treat this edit as unverified and report the blocker."
        ),
    };

    Some(truncate_feedback(feedback))
}

fn load_user_verify_config() -> anyhow::Result<lumen_verify::config::Config> {
    let root = xai_grok_config::load_from_disk()
        .map_err(|error| anyhow::anyhow!("load user config.toml: {error}"))?;
    lumen_verify::config::Config::from_toml_value(&root)
}

fn truncate_feedback(mut feedback: String) -> String {
    if feedback.chars().count() <= MAX_FEEDBACK_CHARS {
        return feedback;
    }
    feedback = feedback.chars().take(MAX_FEEDBACK_CHARS).collect();
    feedback.push_str("\n[verify-after-edit diagnostics truncated]");
    feedback
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::output::{SearchReplaceEditContextInformation, SearchReplaceEditsApplied};

    fn applied_edit(path: PathBuf) -> ToolOutput {
        ToolOutput::SearchReplace(SearchReplaceOutput::EditsApplied(
            SearchReplaceEditsApplied {
                old_string: String::new(),
                new_string: String::new(),
                tool_output_for_prompt: "edited".to_string(),
                tool_output_for_prompt_concise: None,
                absolute_path: path,
                edits: SearchReplaceEditContextInformation::default(),
                patch: None,
                unicode_normalized: false,
            },
        ))
    }

    #[test]
    fn feedback_truncation_preserves_utf8_boundary() {
        let input = "错".repeat(MAX_FEEDBACK_CHARS + 10);
        let output = truncate_feedback(input);
        assert!(output.ends_with("[verify-after-edit diagnostics truncated]"));
        assert!(output.is_char_boundary(output.len()));
    }

    #[tokio::test]
    async fn disabled_policy_skips_writer_verification() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cfg = lumen_verify::config::Config {
            enabled: false,
            ..lumen_verify::config::Config::default()
        };

        let feedback = feedback_for_output_with_config(
            tmp.path().to_path_buf(),
            "session-a".to_string(),
            VerifyAfterEditState::default(),
            &applied_edit(tmp.path().join("main.go")),
            cfg,
        )
        .await;

        assert!(feedback.is_none());
    }

    #[tokio::test]
    async fn third_failure_closes_loop_without_requesting_another_edit() {
        if which::which("go").is_err() {
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("go.mod"), "module example.com/limit\n").unwrap();
        let file = tmp.path().join("main.go");
        std::fs::write(&file, "package main\nfunc main() { missingAtLimit() }\n").unwrap();
        let state = VerifyAfterEditState::default();
        let session_id = "session-a".to_string();
        let key = RepairKey::new(&session_id, dunce::canonicalize(&file).unwrap());
        {
            let mut tracker = state.tracker.lock().await;
            tracker.record_failure(&key);
            tracker.record_failure(&key);
        }

        let feedback = feedback_for_output_with_config(
            tmp.path().to_path_buf(),
            session_id,
            state,
            &applied_edit(file),
            lumen_verify::config::Config::default(),
        )
        .await
        .expect("third failure feedback");

        assert!(feedback.contains("FAILED (attempt 3/3)"));
        assert!(feedback.contains("[verify-after-edit] BLOCKED"));
        assert!(!feedback.contains("edit again"));
    }

    #[tokio::test]
    async fn exhausted_fourth_attempt_returns_explicit_blocked_without_running() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("go.mod"), "module example.com/blocked\n").unwrap();
        let file = tmp.path().join("main.go");
        std::fs::write(&file, "package main\nfunc main() {}\n").unwrap();
        let state = VerifyAfterEditState::default();
        let session_id = "session-a".to_string();
        let key = RepairKey::new(&session_id, dunce::canonicalize(&file).unwrap());
        {
            let mut tracker = state.tracker.lock().await;
            for _ in 0..3 {
                tracker.record_failure(&key);
            }
        }

        let feedback = feedback_for_output_with_config(
            tmp.path().to_path_buf(),
            session_id,
            state,
            &applied_edit(file),
            lumen_verify::config::Config::default(),
        )
        .await
        .expect("blocked feedback");

        assert!(feedback.contains("[verify-after-edit] BLOCKED"));
        assert!(feedback.contains("3/3"));
        assert!(feedback.contains("No verification command was run"));
    }
}
