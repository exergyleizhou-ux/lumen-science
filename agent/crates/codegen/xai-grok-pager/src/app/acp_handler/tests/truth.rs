use super::*;

fn completed_execute(command: &str, exit_code: i32) -> acp::ToolCallUpdate {
    acp::ToolCallUpdate::new(
        acp::ToolCallId::new("truth-run"),
        acp::ToolCallUpdateFields::new()
            .kind(Some(acp::ToolKind::Execute))
            .title(Some(command.to_owned()))
            .status(Some(acp::ToolCallStatus::Completed))
            .raw_output(Some(serde_json::json!({ "exit_code": exit_code }))),
    )
}

#[test]
fn zero_exit_non_verification_commands_do_not_verify() {
    for command in [
        "pwd",
        "ls -la",
        "echo hello",
        "echo cargo test",
        "echo changed > src/lib.rs",
        "git status --short",
    ] {
        assert!(
            truth_exec_from_update(&completed_execute(command, 0)).is_none(),
            "{command:?} must not be treated as verification"
        );
    }
}

#[test]
fn explicit_verification_commands_are_classified() {
    for command in [
        "cargo test -p xai-grok-pager",
        "cd agent && cargo clippy --workspace",
        "go test ./...",
        "python -m pytest tests",
        "npm run lint",
        "make verify",
    ] {
        let classified = truth_exec_from_update(&completed_execute(command, 0));
        assert!(classified.is_some(), "{command:?} should verify");
    }
}

#[test]
fn failed_verification_is_preserved_for_failed_state() {
    let (_, _, exit) = truth_exec_from_update(&completed_execute("cargo test", 101))
        .expect("cargo test is a verification run even when it fails");
    assert_eq!(exit, 101);
}

#[test]
fn fs_notification_stales_verification_from_external_or_bash_changes() {
    let mut app = make_app_with_agent("truth-session");
    let agent = app.agents.get_mut(&AgentId(0)).unwrap();
    agent
        .note_truth_verification_passed("cargo test", "verify-1")
        .unwrap();

    let raw = serde_json::value::to_raw_value(&serde_json::json!({
        "sessionId": "truth-session",
        "event": { "kind": "Modify", "paths": ["/tmp/src/lib.rs"] }
    }))
    .unwrap();
    let notification = acp::ExtNotification::new("x.ai/fs_notify", std::sync::Arc::from(raw));
    assert!(handle_fs_notify(&notification, &mut app));
    assert!(matches!(
        app.agents[&AgentId(0)]
            .display_truth_snapshot()
            .verification,
        crate::ui_contract::VerificationSummary::Stale { .. }
    ));
}
