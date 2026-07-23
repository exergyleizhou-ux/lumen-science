//! `/probe` — mark capability Checking and show recovery (never invents Tool-ready).

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct ProbeCommand;

impl SlashCommand for ProbeCommand {
    fn name(&self) -> &str {
        "probe"
    }

    fn description(&self) -> &str {
        "Start capability check (real tool_call required for Tool-ready)"
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn usage(&self) -> &str {
        "/probe"
    }

    fn run(&self, ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        if ctx.session_id.is_none() {
            return CommandResult::Error(
                "No active session. Start a session first, then run /probe.".into(),
            );
        }
        CommandResult::Action(Action::BeginTruthProbe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::model_state::ModelState;
    use crate::slash::commands::tests::make_ctx;
    use agent_client_protocol as acp;

    #[test]
    fn probe_requires_session() {
        let models = ModelState::default();
        let mut ctx = make_ctx(&models);
        assert!(matches!(
            ProbeCommand.run(&mut ctx, ""),
            CommandResult::Error(_)
        ));
    }

    #[test]
    fn probe_with_session_starts_begin_truth_probe() {
        let models = ModelState::default();
        let session = acp::SessionId::new("s1");
        let mut ctx = make_ctx(&models);
        ctx.session_id = Some(&session);
        assert!(matches!(
            ProbeCommand.run(&mut ctx, ""),
            CommandResult::Action(Action::BeginTruthProbe)
        ));
    }
}
