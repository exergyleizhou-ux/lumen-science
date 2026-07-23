//! `/status` -- show redacted provider/capability/permission/cache/verification truth.

use crate::app::actions::Action;
use crate::slash::command::{CommandExecCtx, CommandResult, SlashCommand};

pub struct StatusCommand;

impl SlashCommand for StatusCommand {
    fn name(&self) -> &str {
        "status"
    }

    fn description(&self) -> &str {
        "View runtime truth and evidence"
    }

    fn session_scoped(&self) -> bool {
        false
    }

    fn usage(&self) -> &str {
        "/status"
    }

    fn run(&self, ctx: &mut CommandExecCtx, _args: &str) -> CommandResult {
        if ctx.session_id.is_some() {
            CommandResult::Action(Action::ShowTruthStatus)
        } else {
            // Dashboard has no session context, but its selected row owns the
            // same truth report as the clickable dashboard truth bar.
            CommandResult::Action(Action::DashboardShowTruthStatus)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::model_state::ModelState;
    use crate::slash::commands::tests::make_ctx;

    #[test]
    fn status_routes_dashboard_to_selected_truth_action() {
        let models = ModelState::default();
        let mut ctx = make_ctx(&models);
        assert!(matches!(
            StatusCommand.run(&mut ctx, ""),
            CommandResult::Action(Action::DashboardShowTruthStatus)
        ));
    }

    #[test]
    fn status_with_session_routes_to_show_truth_status_click_equivalent() {
        use agent_client_protocol as acp;

        let models = ModelState::default();
        let session = acp::SessionId::new("sess-1");
        let mut ctx = make_ctx(&models);
        ctx.session_id = Some(&session);
        assert!(matches!(
            StatusCommand.run(&mut ctx, ""),
            CommandResult::Action(Action::ShowTruthStatus)
        ));
    }
}
