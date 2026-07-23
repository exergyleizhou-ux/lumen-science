//! `/goal` -- set or manage an autonomous goal.
//!
//! Goal state is owned by the shell's session actor. The pager deliberately
//! passes the command through unchanged so the shell remains the single parser
//! for objective text, lifecycle subcommands, and `--budget`.

use xai_grok_tools::implementations::grok_build::UPDATE_GOAL_TOOL_NAME;

use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

const REQUIRED_TOOLS: &[&str] = &[UPDATE_GOAL_TOOL_NAME];

pub struct GoalCommand;

impl SlashCommand for GoalCommand {
    fn name(&self) -> &str {
        "goal"
    }

    fn description(&self) -> &str {
        "Set, manage, or check an autonomous goal"
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn usage(&self) -> &str {
        "/goal <objective> [--budget <tokens>] | status | pause | resume | clear"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("<objective> [--budget <tokens>] | status | pause | resume | clear")
    }

    fn required_tools(&self) -> &[&str] {
        REQUIRED_TOOLS
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let args = args.trim();
        if args.is_empty() {
            CommandResult::PassThrough("/goal".to_string())
        } else {
            CommandResult::PassThrough(format!("/goal {args}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::model_state::ModelState;

    fn run(args: &str) -> CommandResult {
        let models = ModelState::default();
        let mut ctx = super::super::tests::make_ctx(&models);
        GoalCommand.run(&mut ctx, args)
    }

    #[test]
    fn bare_goal_passes_through_as_status_request() {
        assert!(matches!(run(""), CommandResult::PassThrough(text) if text == "/goal"));
        assert!(matches!(run("   "), CommandResult::PassThrough(text) if text == "/goal"));
    }

    #[test]
    fn objective_passes_through_without_plan_mode_aliasing() {
        assert!(matches!(
            run("  Build a REST API  "),
            CommandResult::PassThrough(text) if text == "/goal Build a REST API"
        ));
    }

    #[test]
    fn lifecycle_subcommands_pass_through_to_shell() {
        for subcommand in ["status", "pause", "resume", "clear"] {
            assert!(matches!(
                run(subcommand),
                CommandResult::PassThrough(text) if text == format!("/goal {subcommand}")
            ));
        }
    }

    #[test]
    fn budget_syntax_is_preserved_for_authoritative_shell_parser() {
        assert!(matches!(
            run("ship release --budget 500000"),
            CommandResult::PassThrough(text)
                if text == "/goal ship release --budget 500000"
        ));
    }

    #[test]
    fn metadata_matches_shell_contract() {
        assert_eq!(GoalCommand.name(), "goal");
        assert!(GoalCommand.session_scoped());
        assert!(!GoalCommand.offered_when_session_less());
        assert!(GoalCommand.takes_args());
        assert!(!GoalCommand.args_required());
        assert!(GoalCommand.usage().contains("--budget"));
        assert!(GoalCommand.arg_placeholder().unwrap().contains("resume"));
    }

    #[test]
    fn goal_is_hidden_without_update_goal_capability() {
        assert_eq!(GoalCommand.required_tools(), &["update_goal"]);
    }
}
