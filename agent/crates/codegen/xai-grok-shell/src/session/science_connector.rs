//! P4 SSH/SCP Science admission through the existing Lumen permission manager.
//!
//! This is intentionally separate from `handle.rs`: that file has concurrent
//! owner work, while this extension only adds a new SessionHandle capability.

use super::{SessionCommand, SessionHandle};
use agent_client_protocol as acp;
use tokio::sync::oneshot;
use xai_grok_science::{
    ApprovalDecision, RunContext, ScienceError, ScienceStore,
    connector::{
        AdmissionTicket, ConnectorPolicy, ConnectorRequest, OfflineTransportOutcome,
        OfflineTransportReceipt,
    },
};

impl SessionHandle {
    /// P5 host-verification entry. The handle carries no Goal/Expert mutation
    /// authority; it can only ask the owning SessionActor to run the bound,
    /// durable completion gate.
    pub async fn verify_science_goal(
        &self,
        store: ScienceStore,
        run_id: xai_grok_science::RunId,
    ) -> Result<
        xai_grok_science::review::HostVerificationReport,
        crate::session::science_goal::ScienceGoalReviewError,
    > {
        let (respond_to, response) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::VerifyScienceGoal(Box::new(
                crate::session::commands::VerifyScienceGoal {
                    store,
                    run_id,
                    respond_to,
                },
            )))
            .map_err(|_| crate::session::science_goal::ScienceGoalReviewError::NoActiveGoal)?;
        response
            .await
            .map_err(|_| crate::session::science_goal::ScienceGoalReviewError::NoActiveGoal)?
    }

    /// S3 real SCP product path. Admission and process execution both cross
    /// the sole SessionActor; the operation digest is bound before permission.
    pub async fn run_science_ssh_scp_transport(
        &self,
        store: ScienceStore,
        context: RunContext,
        policy: ConnectorPolicy,
        request: ConnectorRequest,
        operation: xai_grok_science::transport::ScpOperation,
        config: xai_grok_science::transport::ScpExecutionConfig,
        approval_timeout: std::time::Duration,
    ) -> xai_grok_science::Result<Option<xai_grok_science::transport::TransportReceipt>> {
        let Some(ticket) = self
            .admit_science_ssh_scp_with_approval_timeout(
                store.clone(),
                context,
                policy,
                request,
                approval_timeout,
            )
            .await?
        else {
            return Ok(None);
        };
        let (respond_to, response) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::ExecuteScienceSshScpTransport(Box::new(
                crate::session::commands::ExecuteScienceSshScpTransport {
                    store,
                    ticket,
                    operation,
                    config,
                    respond_to,
                },
            )))
            .map_err(|_| ScienceError::Invalid("session actor unavailable".into()))?;
        Ok(Some(response.await.map_err(|_| {
            ScienceError::Invalid("session actor stopped".into())
        })??))
    }

    /// Full P4 offline product path. Admission and the deterministic transport
    /// both execute through SessionActor; only the pre-existing Lumen
    /// permission manager can supply the Allow that permits the model run.
    pub async fn run_science_ssh_scp_offline_transport(
        &self,
        store: ScienceStore,
        context: RunContext,
        policy: ConnectorPolicy,
        request: ConnectorRequest,
        approval_timeout: std::time::Duration,
        outcome: OfflineTransportOutcome,
    ) -> xai_grok_science::Result<Option<OfflineTransportReceipt>> {
        let Some(ticket) = self
            .admit_science_ssh_scp_with_approval_timeout(
                store.clone(),
                context,
                policy,
                request,
                approval_timeout,
            )
            .await?
        else {
            return Ok(None);
        };
        let (respond_to, response) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::ExecuteScienceSshScpOfflineTransport(
                Box::new(
                    crate::session::commands::ExecuteScienceSshScpOfflineTransport {
                        store,
                        ticket,
                        outcome,
                        respond_to,
                    },
                ),
            ))
            .map_err(|_| ScienceError::Invalid("session actor unavailable".into()))?;
        Ok(Some(response.await.map_err(|_| {
            ScienceError::Invalid("session actor stopped".into())
        })??))
    }

    /// Executes the P4 ordering invariant:
    ///
    /// `SessionActor durable admission -> Lumen permission manager ->
    /// SessionActor durable terminal decision`.
    ///
    /// A denied admission returns `Ok(None)` without asking permission. An
    /// allowed permission returns an opaque ticket only; this method has no
    /// transport implementation and cannot open a socket or start a remote job.
    pub async fn admit_science_ssh_scp_with_approval_timeout(
        &self,
        store: ScienceStore,
        context: RunContext,
        policy: ConnectorPolicy,
        request: ConnectorRequest,
        approval_timeout: std::time::Duration,
    ) -> xai_grok_science::Result<Option<AdmissionTicket>> {
        use xai_grok_workspace::permission::{AccessKind, Decision};

        let (begin_tx, begin_rx) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::BeginScienceSshScpAdmission(Box::new(
                crate::session::commands::BeginScienceSshScpAdmission {
                    store,
                    context,
                    policy,
                    request,
                    respond_to: begin_tx,
                },
            )))
            .map_err(|_| ScienceError::Invalid("session actor unavailable".into()))?;
        let Some(prepared) = begin_rx
            .await
            .map_err(|_| ScienceError::Invalid("session actor stopped".into()))??
        else {
            return Ok(None);
        };

        // This is deliberately generic: the target identity remains in the
        // project policy and redacted Science audit, not in a shell command or
        // permission-manager free-form reason.
        let call_id = acp::ToolCallId::new(std::sync::Arc::from(prepared.ticket.call_id.0.clone()));
        let update = acp::ToolCallUpdate::new(
            call_id,
            acp::ToolCallUpdateFields::new()
                .kind(Some(acp::ToolKind::Other))
                .title(Some("Lumen Science SSH/SCP connector admission".into())),
        );
        let permission = tokio::time::timeout(
            approval_timeout,
            self.permission_handle.request(
                // Keep the ACP permission subject an actual tool identity so
                // the production bridge can present an Allow option, while
                // leaving host, path, and credential material out of it.
                AccessKind::Bash("scp".into()),
                update,
                Some(self.info.id.0.to_string()),
                None,
                None,
            ),
        )
        .await;
        let decision = match permission {
            Err(_) => ApprovalDecision::Timeout,
            Ok(Decision::Allow) => ApprovalDecision::Allow,
            Ok(Decision::Ask)
            | Ok(Decision::Reject(_))
            | Ok(Decision::PolicyDeny(_))
            | Ok(Decision::FollowupMessage(_)) => ApprovalDecision::Deny,
            Ok(Decision::Cancelled) => ApprovalDecision::Cancel,
        };
        let (finish_tx, finish_rx) = oneshot::channel();
        self.cmd_tx
            .send(SessionCommand::FinishScienceSshScpAdmission(Box::new(
                crate::session::commands::FinishScienceSshScpAdmission {
                    prepared,
                    decision,
                    respond_to: finish_tx,
                },
            )))
            .map_err(|_| ScienceError::Invalid("session actor unavailable".into()))?;
        finish_rx
            .await
            .map_err(|_| ScienceError::Invalid("session actor stopped".into()))?
    }
}
