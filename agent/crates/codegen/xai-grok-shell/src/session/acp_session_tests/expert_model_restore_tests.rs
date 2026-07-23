use super::support::*;
use super::*;
use crate::session::acp_session::expert_impl::ExpertTurnGuard;
use crate::session::expert::{
    ExpertErrorCode, ExpertFeatureState, ExpertMode, ExpertModeState, ExpertOutcome, ExpertPhase,
};
use xai_grok_test_support::MockInferenceServer;

#[derive(Clone, Copy)]
enum TerminalCase {
    Completed,
    ParseFailure,
    Cancelled,
    Off,
    Failed,
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial]
async fn goal_compose_enters_executor_restores_each_round_and_preserves_global_default() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let home = tempfile::tempdir().unwrap();
            let _home = xai_grok_test_support::EnvGuard::set("GROK_HOME", home.path());
            let _lumen_home = xai_grok_test_support::EnvGuard::set("LUMEN_HOME", home.path());
            let config_path = home.path().join("config.toml");
            let config_bytes = b"[models]\ndefault = \"global-sentinel\"\n";
            std::fs::write(&config_path, config_bytes).unwrap();

            let (gateway_tx, _gateway_rx) = tokio::sync::mpsc::unbounded_channel();
            let (persistence_tx, _persistence_rx) = tokio::sync::mpsc::unbounded_channel();
            let actor = create_test_actor(0, 32_000, 85, gateway_tx, persistence_tx).await;
            actor.models_manager.insert_test_entry(
                "deepseek-v4-pro",
                crate::agent::config::ModelEntry {
                    info: crate::agent::config::ModelInfo::fallback("deepseek-v4-pro"),
                    api_key: Some("test-key".into()),
                    env_key: None,
                    api_base_url: Some("http://localhost".into()),
                },
            );
            let original = actor.reconstruct_full_config().await;
            let mut expert = ExpertModeState::configured();
            expert.require_consult_on_medium = false;
            actor.state.lock().await.expert = expert;
            {
                let mut goal = actor.goal_tracker.lock();
                goal.create_goal(
                    "goal-compose".into(),
                    "ship".into(),
                    Some(10_000),
                    0,
                    "2026-01-01T00:00:00Z".into(),
                    None,
                );
                goal.configure_expert_policy(true, 3, 15, true);
            }

            for _round in 0..2 {
                let (guard, envelope) = actor.begin_goal_expert_turn("ship").await.unwrap();
                assert!(guard.goal_composed);
                assert!(envelope.contains("<expert-mode>"));
                assert_eq!(
                    actor.reconstruct_full_config().await.model,
                    "deepseek-v4-pro"
                );
                actor
                    .finish_expert_turn(
                        guard,
                        &Ok(TurnOutcome::Cancelled {
                            category: None,
                            context: None,
                        }),
                    )
                    .await;
                let restored = actor.reconstruct_full_config().await;
                assert_eq!(restored.model, original.model);
                assert_eq!(restored.reasoning_effort, original.reasoning_effort);
            }

            let (guard, _) = actor.begin_goal_expert_turn("ship").await.unwrap();
            actor
                .goal_tracker
                .lock()
                .configure_expert_policy(false, 0, 0, false);
            actor
                .finish_expert_turn(
                    guard,
                    &Ok(TurnOutcome::Cancelled {
                        category: None,
                        context: None,
                    }),
                )
                .await;
            let restored_after_off = actor.reconstruct_full_config().await;
            assert_eq!(restored_after_off.model, original.model);
            assert_eq!(
                restored_after_off.reasoning_effort,
                original.reasoning_effort
            );

            assert_eq!(std::fs::read(&config_path).unwrap(), config_bytes);
            let goal = actor.goal_tracker.lock().snapshot().cloned().unwrap();
            assert!(!goal.expert_policy);
            assert!(goal.expert_consult_attempts_used <= goal.expert_consult_cap_per_goal);
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial]
async fn goal_compose_reserves_rolling_before_consultant_and_executor_resolution() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let server = MockInferenceServer::start().await.unwrap();
            server.set_response(r#"{"plan":["inspect"]}"#);
            let (gateway_tx, _gateway_rx) = tokio::sync::mpsc::unbounded_channel();
            let (persistence_tx, mut persistence_rx) = tokio::sync::mpsc::unbounded_channel();
            let (observed_tx, mut observed_rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::task::spawn_local(async move {
                while let Some(msg) = persistence_rx.recv().await {
                    match msg {
                        PersistenceMsg::GoalModeState(_) => {
                            let _ = observed_tx.send("goal");
                        }
                        PersistenceMsg::ExpertModeStateAndAck { respond_to, .. } => {
                            let _ = observed_tx.send("ack");
                            let _ = respond_to.send(Ok(()));
                        }
                        _ => {}
                    }
                }
            });
            let actor = create_test_actor(0, 32_000, 85, gateway_tx, persistence_tx).await;
            // Deliberately leave both consultant and executor absent from the
            // catalog. Ambient bearer env can synthesize aux configs; clear it
            // for this serial test so resolution fails closed without HTTP.
            let previous_xai = std::env::var("XAI_API_KEY").ok();
            let previous_openai = std::env::var("OPENAI_API_KEY").ok();
            // SAFETY: #[serial_test::serial]; restored before returning.
            unsafe {
                std::env::remove_var("XAI_API_KEY");
                std::env::remove_var("OPENAI_API_KEY");
            }
            actor.state.lock().await.expert = ExpertModeState::configured();
            {
                let mut goal = actor.goal_tracker.lock();
                goal.create_goal(
                    "goal-reserve".into(),
                    "production auth migration".into(),
                    None,
                    0,
                    "2026-01-01T00:00:00Z".into(),
                    None,
                );
                goal.configure_expert_policy(true, 1, 1, true);
            }

            let first = actor
                .begin_goal_expert_turn("production auth migration")
                .await;
            assert!(
                matches!(first, Err(ExpertErrorCode::ModelMissing)),
                "executor resolution must fail closed without ambient bearer"
            );
            assert!(
                server.requests().is_empty(),
                "rolling charge + reservation must not poll providers when models are missing"
            );
            assert_eq!(observed_rx.recv().await, Some("goal"));
            assert_eq!(observed_rx.recv().await, Some("ack"));
            assert_eq!(
                actor
                    .goal_tracker
                    .lock()
                    .snapshot()
                    .unwrap()
                    .expert_consult_attempts_used,
                1
            );

            assert!(matches!(
                actor
                    .begin_goal_expert_turn("production auth migration")
                    .await,
                Err(ExpertErrorCode::ModelMissing)
            ));
            assert!(server.requests().is_empty());
            let expert = actor.state.lock().await.expert.clone();
            assert_eq!(expert.budget.attempts, 0);
            assert!(
                expert
                    .notes
                    .iter()
                    .any(|note| note == "executor-only: budget_exhausted"),
                "rolling cap must block a second consult reservation after pre-executor failure"
            );
            unsafe {
                match previous_xai {
                    Some(v) => std::env::set_var("XAI_API_KEY", v),
                    None => std::env::remove_var("XAI_API_KEY"),
                }
                match previous_openai {
                    Some(v) => std::env::set_var("OPENAI_API_KEY", v),
                    None => std::env::remove_var("OPENAI_API_KEY"),
                }
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
#[serial_test::serial]
async fn expert_session_model_and_effort_restore_on_every_terminal_without_global_write() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let home = tempfile::tempdir().unwrap();
            let _home = xai_grok_test_support::EnvGuard::set("GROK_HOME", home.path());
            let config_path = home.path().join("config.toml");
            let config_bytes = b"[models]\ndefault = \"global-sentinel\"\n";
            std::fs::write(&config_path, config_bytes).unwrap();
            let process_default = crate::models::default_model().to_owned();

            for case in [
                TerminalCase::Completed,
                TerminalCase::ParseFailure,
                TerminalCase::Cancelled,
                TerminalCase::Off,
                TerminalCase::Failed,
            ] {
                let (gateway_tx, _gateway_rx) = tokio::sync::mpsc::unbounded_channel();
                let (persistence_tx, _persistence_rx) = tokio::sync::mpsc::unbounded_channel();
                let actor = create_test_actor(0, 32_000, 85, gateway_tx, persistence_tx).await;
                let original = xai_grok_sampler::SamplerConfig {
                    api_key: Some("session-key".to_owned()),
                    base_url: "http://session.invalid".to_owned(),
                    model: "session-model-before-expert".to_owned(),
                    api_backend: crate::sampling::ApiBackend::ChatCompletions,
                    context_window: 32_000,
                    reasoning_effort: Some("high".parse().unwrap()),
                    ..Default::default()
                };
                actor
                    .handle_set_session_model(original.clone(), false, false, true, 85)
                    .await
                    .unwrap();

                let mut expert = ExpertModeState::configured();
                expert
                    .start(
                        "production auth migration",
                        ExpertMode::Fast,
                        "executor-model",
                    )
                    .unwrap();
                expert.phase = ExpertPhase::Executing;
                expert.model_before_expert = Some(original.model.clone());
                expert.reasoning_effort_before_expert = Some("high".to_owned());
                if matches!(case, TerminalCase::ParseFailure) {
                    expert.last_error_code = Some(ExpertErrorCode::ParseError.as_str().to_owned());
                    expert.notes.push("executor-only: parse_error".to_owned());
                }
                let task_id = expert.task_id.clone().unwrap();
                let generation = expert.task_generation;
                if matches!(case, TerminalCase::Off) {
                    expert.disable();
                    // The executor is still the in-flight session model until
                    // its terminal callback runs and owns restoration.
                    expert.phase = ExpertPhase::Executing;
                }
                actor.state.lock().await.expert = expert;

                let executor = xai_grok_sampler::SamplerConfig {
                    model: "executor-model".to_owned(),
                    reasoning_effort: Some("xhigh".parse().unwrap()),
                    ..original.clone()
                };
                actor
                    .handle_set_session_model(executor, false, false, true, 85)
                    .await
                    .unwrap();
                let live = actor.reconstruct_full_config().await;
                assert_eq!(live.model, "executor-model");
                assert_eq!(
                    live.reasoning_effort.map(|e| e.to_string()).as_deref(),
                    Some("xhigh")
                );

                let guard = ExpertTurnGuard {
                    original_config: original.clone(),
                    task_id,
                    generation,
                    goal_composed: false,
                };
                match case {
                    TerminalCase::Completed => {
                        actor
                            .finish_expert_turn(
                                guard,
                                &Ok(TurnOutcome::Completed {
                                    snapshot: Box::new(None),
                                    tools_called: vec![],
                                    structured_output: None,
                                    refusal: false,
                                }),
                            )
                            .await;
                    }
                    TerminalCase::Cancelled | TerminalCase::Off => {
                        actor
                            .finish_expert_turn(
                                guard,
                                &Ok(TurnOutcome::Cancelled {
                                    category: None,
                                    context: None,
                                }),
                            )
                            .await;
                    }
                    TerminalCase::ParseFailure | TerminalCase::Failed => {
                        actor.abort_expert_turn(guard, "executor failed").await;
                    }
                }

                let restored = actor.reconstruct_full_config().await;
                assert_eq!(restored.model, original.model);
                assert_eq!(
                    restored.reasoning_effort.map(|e| e.to_string()).as_deref(),
                    Some("high")
                );
                let state = actor.state.lock().await.expert.clone();
                assert!(matches!(
                    state.feature_state,
                    ExpertFeatureState::IdleConfigured | ExpertFeatureState::Off
                ));
                assert!(state.model_before_expert.is_none());
                assert!(state.reasoning_effort_before_expert.is_none());
                if matches!(case, TerminalCase::Off) {
                    assert_eq!(state.feature_state, ExpertFeatureState::Off);
                    assert_eq!(state.last_outcome, Some(ExpertOutcome::Aborted));
                }
            }

            assert_eq!(std::fs::read(&config_path).unwrap(), config_bytes);
            assert_eq!(crate::models::default_model(), process_default);
        })
        .await;
}

fn bash_tool_result(command: &str, exit_code: i32, output: &str) -> ToolRunResult {
    ToolRunResult {
        output: ToolsToolOutput::Bash(xai_grok_tools::types::output::BashOutput {
            output: output.as_bytes().to_vec(),
            output_for_prompt: output.to_owned(),
            exit_code,
            command: command.to_owned(),
            truncated: false,
            signal: None,
            timed_out: false,
            description: Some("run tests".to_owned()),
            current_dir: "/fixture".to_owned(),
            output_file: String::new(),
            total_bytes: output.len(),
            output_delta: None,
            was_bare_echo: false,
        }),
        prompt_text: output.to_owned(),
        effective_tool_name: None,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn successful_production_tool_result_is_required_for_expert_completed() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            for (command, exit_code, output, expected) in [
                (
                    "cargo test -p fixture",
                    0,
                    "tests completed",
                    ExpertOutcome::Completed,
                ),
                (
                    "cargo build -p fixture",
                    0,
                    "build completed",
                    ExpertOutcome::Completed,
                ),
                (
                    "go test ./...",
                    0,
                    "package fixture",
                    ExpertOutcome::Completed,
                ),
                (
                    "cargo test -p fixture",
                    1,
                    "ok pass 0 failed",
                    ExpertOutcome::Partial,
                ),
                (
                    "echo ok",
                    0,
                    "ok pass 0 failed test result: ok",
                    ExpertOutcome::Partial,
                ),
                (
                    "cargo test -p fixture || true",
                    0,
                    "ok pass 0 failed test result: ok",
                    ExpertOutcome::Partial,
                ),
            ] {
                let (gateway_tx, _gateway_rx) = tokio::sync::mpsc::unbounded_channel();
                let (persistence_tx, _persistence_rx) = tokio::sync::mpsc::unbounded_channel();
                let actor = create_test_actor(0, 32_000, 85, gateway_tx, persistence_tx).await;
                let original = actor.reconstruct_full_config().await;
                let mut expert = ExpertModeState::configured();
                expert
                    .start("fix production", ExpertMode::Fast, "executor-model")
                    .unwrap();
                expert.phase = ExpertPhase::Executing;
                expert.model_before_expert = Some(original.model.clone());
                let guard = ExpertTurnGuard {
                    original_config: original,
                    task_id: expert.task_id.clone().unwrap(),
                    generation: expert.task_generation,
                    goal_composed: false,
                };
                actor.state.lock().await.expert = expert;

                actor.record_delivery_evidence_from_tool_result(
                    "bash",
                    "bash",
                    &bash_tool_result(command, exit_code, output),
                );
                actor
                    .finish_expert_turn(
                        guard,
                        &Ok(TurnOutcome::Completed {
                            snapshot: Box::new(None),
                            tools_called: vec!["bash".to_owned()],
                            structured_output: None,
                            refusal: false,
                        }),
                    )
                    .await;

                let state = actor.state.lock().await.expert.clone();
                assert_eq!(state.last_outcome, Some(expected));
                assert_eq!(
                    state.verification_summary.outcome,
                    if expected == ExpertOutcome::Completed {
                        crate::session::expert::HostVerificationOutcome::Met
                    } else {
                        crate::session::expert::HostVerificationOutcome::Unknown
                    }
                );
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn failed_off_restore_keeps_anchor_and_guard_until_retry_succeeds() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let (gateway_tx, _gateway_rx) = tokio::sync::mpsc::unbounded_channel();
            let (persistence_tx, mut persistence_rx) = tokio::sync::mpsc::unbounded_channel();
            tokio::task::spawn_local(async move {
                while let Some(msg) = persistence_rx.recv().await {
                    if let PersistenceMsg::ExpertModeStateAndAck { respond_to, .. } = msg {
                        let _ = respond_to.send(Ok(()));
                    }
                }
            });
            let actor = create_test_actor(0, 32_000, 85, gateway_tx, persistence_tx).await;
            let live_before = actor.reconstruct_full_config().await.model;
            let mut expert = ExpertModeState::configured();
            expert
                .start("restore model", ExpertMode::Fast, "executor-model")
                .unwrap();
            expert.model_before_expert = Some("model-not-present-in-catalog".to_owned());
            expert.reasoning_effort_before_expert = Some("high".to_owned());
            expert.disable();
            actor.state.lock().await.expert = expert;

            // Ambient XAI_API_KEY synthesizes an aux sampler for unknown
            // catalog IDs. Clear bearer env so missing-model restore fails
            // closed and keeps the Disabling anchors for retry.
            let previous_xai = std::env::var("XAI_API_KEY").ok();
            let previous_openai = std::env::var("OPENAI_API_KEY").ok();
            // SAFETY: single-threaded LocalSet test; restored before return.
            unsafe {
                std::env::remove_var("XAI_API_KEY");
                std::env::remove_var("OPENAI_API_KEY");
            }

            assert_eq!(
                actor.restore_disabled_expert().await,
                Err(ExpertErrorCode::RestoreFailed)
            );
            let failed = actor.state.lock().await.expert.clone();
            assert_eq!(failed.feature_state, ExpertFeatureState::Disabling);
            assert_eq!(failed.last_error_code.as_deref(), Some("restore_failed"));
            assert_eq!(
                failed.model_before_expert.as_deref(),
                Some("model-not-present-in-catalog")
            );
            assert_eq!(
                failed.reasoning_effort_before_expert.as_deref(),
                Some("high")
            );
            assert_eq!(actor.reconstruct_full_config().await.model, live_before);

            // Simulate the missing catalog route becoming available, then use
            // the same `/expert off` restore path as the retry.
            {
                let mut state = actor.state.lock().await;
                state.expert.model_before_expert = Some("grok-4.5".to_owned());
                state.expert.reasoning_effort_before_expert = None;
            }
            let mut restored_entry = crate::agent::config::ModelEntry::fallback(
                "grok-4.5",
                &crate::agent::config::EndpointsConfig::default(),
            );
            restored_entry.api_key = Some("fixture-api-key".to_owned());
            actor
                .models_manager
                .insert_test_entry("grok-4.5", restored_entry);
            assert_eq!(actor.restore_disabled_expert().await, Ok(()));
            let restored = actor.state.lock().await.expert.clone();
            assert_eq!(restored.feature_state, ExpertFeatureState::Off);
            assert_eq!(restored.last_error_code, None);
            assert_eq!(restored.model_before_expert, None);
            assert_eq!(restored.reasoning_effort_before_expert, None);
            assert_eq!(actor.reconstruct_full_config().await.model, "grok-4.5");
            unsafe {
                match previous_xai {
                    Some(v) => std::env::set_var("XAI_API_KEY", v),
                    None => std::env::remove_var("XAI_API_KEY"),
                }
                match previous_openai {
                    Some(v) => std::env::set_var("OPENAI_API_KEY", v),
                    None => std::env::remove_var("OPENAI_API_KEY"),
                }
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn stale_old_completion_cannot_restore_over_new_expert_task() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let (gateway_tx, _gateway_rx) = tokio::sync::mpsc::unbounded_channel();
            let (persistence_tx, _persistence_rx) = tokio::sync::mpsc::unbounded_channel();
            let actor = create_test_actor(0, 32_000, 85, gateway_tx, persistence_tx).await;
            let old_original = actor.reconstruct_full_config().await;
            let mut old = ExpertModeState::configured();
            old.start("old task", ExpertMode::Fast, "old-executor")
                .unwrap();
            old.phase = ExpertPhase::Executing;
            let old_guard = ExpertTurnGuard {
                original_config: old_original,
                task_id: old.task_id.clone().unwrap(),
                generation: old.task_generation,
                goal_composed: false,
            };
            old.restored(Ok(()), ExpertOutcome::Aborted);
            old.start("new task", ExpertMode::Fast, "new-executor")
                .unwrap();
            old.phase = ExpertPhase::Executing;
            old.model_before_expert = Some("new-anchor".to_owned());
            let new_task_id = old.task_id.clone();
            let new_generation = old.task_generation;
            actor.state.lock().await.expert = old;
            let mut live = actor.reconstruct_full_config().await;
            live.model = "new-executor".to_owned();
            actor
                .handle_set_session_model(live, false, false, true, 85)
                .await
                .unwrap();

            actor
                .abort_expert_turn(old_guard, "late old completion")
                .await;

            let state = actor.state.lock().await.expert.clone();
            assert_eq!(state.task_id, new_task_id);
            assert_eq!(state.task_generation, new_generation);
            assert_eq!(state.phase, ExpertPhase::Executing);
            assert_eq!(actor.reconstruct_full_config().await.model, "new-executor");
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn run_loop_rejects_external_model_switch_while_expert_owns_guard() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let (gateway_tx, _gateway_rx) = tokio::sync::mpsc::unbounded_channel();
            let (persistence_tx, _persistence_rx) = tokio::sync::mpsc::unbounded_channel();
            let (actor, event_rx) =
                create_test_actor_ex(0, 32_000, 85, gateway_tx, persistence_tx).await;
            let actor = Arc::new(actor);
            let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
            let (_chat_tx, chat_rx) =
                tokio::sync::mpsc::unbounded_channel::<xai_chat_state::ChatStateEvent>();
            tokio::task::spawn_local(super::run_session(
                actor.clone(),
                cmd_rx,
                chat_rx,
                event_rx,
                None,
                Arc::new(parking_lot::Mutex::new(
                    xai_grok_workspace::file_system::CodebaseIndexManager::new(),
                )),
                std::path::PathBuf::from("/tmp"),
                crate::session::fs_watch::FsWatchCapabilities::none(),
            ));

            let config = |model: &str, effort: &str| xai_grok_sampler::SamplerConfig {
                model: model.to_owned(),
                reasoning_effort: Some(effort.parse().unwrap()),
                context_window: 32_000,
                ..Default::default()
            };
            let mut expert = ExpertModeState::configured();
            expert
                .start("guard model", ExpertMode::Fast, "executor-model")
                .unwrap();
            actor.state.lock().await.expert = expert;

            // Expert's internal switch path bypasses the external run-loop
            // gate and must remain usable while Active.
            actor
                .handle_set_session_model(
                    config("expert-executor", "xhigh"),
                    false,
                    false,
                    true,
                    85,
                )
                .await
                .unwrap();

            let external_switch = |model: &str| {
                let (responds_to, response) = tokio::sync::oneshot::channel();
                let command = SessionCommand::SetSessionModel {
                    sampling_config: config(model, "low"),
                    use_concise: false,
                    apply_prompt_override: false,
                    skip_prompt_rewrite: true,
                    auto_compact_threshold_percent: 85,
                    responds_to,
                };
                (command, response)
            };

            let (command, response) = external_switch("external-active");
            cmd_tx.send(command).unwrap();
            let error = response.await.unwrap().expect_err("Active must reject");
            assert!(error.data.as_ref().is_some_and(|data| {
                data.as_str()
                    .is_some_and(|message| message.starts_with("expert_active:"))
            }));
            assert_eq!(
                actor.reconstruct_full_config().await.model,
                "expert-executor"
            );

            actor.state.lock().await.expert.disable();
            let (rebuild_tx, rebuild_rx) = tokio::sync::oneshot::channel();
            cmd_tx
                .send(SessionCommand::RebuildAgentForDefinition {
                    definition: actor.agent.borrow().definition().clone(),
                    responds_to: rebuild_tx,
                })
                .unwrap();
            rebuild_rx
                .await
                .unwrap()
                .expect_err("Disabling must reject external harness rebuild");
            let (command, response) = external_switch("external-disabling");
            cmd_tx.send(command).unwrap();
            response.await.unwrap().expect_err("Disabling must reject");
            assert_eq!(
                actor.reconstruct_full_config().await.model,
                "expert-executor"
            );

            actor
                .state
                .lock()
                .await
                .expert
                .restored(Ok(()), ExpertOutcome::Aborted);
            let (command, response) = external_switch("external-after-restore");
            cmd_tx.send(command).unwrap();
            let switched = response
                .await
                .unwrap()
                .expect("terminal restored state must allow normal switch");
            assert_eq!(switched.0.as_ref(), "external-after-restore");
            let live = actor.reconstruct_full_config().await;
            assert_eq!(live.model, "external-after-restore");
            assert_eq!(
                live.reasoning_effort
                    .map(|effort| effort.to_string())
                    .as_deref(),
                Some("low")
            );
        })
        .await;
}
