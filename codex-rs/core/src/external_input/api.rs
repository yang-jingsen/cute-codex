use crate::CodexThread;
use anyhow::Result;
use codex_extension_api::ThreadIdleCause;
use codex_protocol::external_input::Envelope;
use codex_protocol::external_input_record::MessageKey;
use codex_protocol::external_input_status::Status;
use uuid::Uuid;

impl CodexThread {
    pub async fn subscribe_external_input_status(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<String>> {
        let state = self.session.external_input.lock().await;
        let runtime = state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("external input disabled"))?;
        Ok(runtime.changed.subscribe())
    }

    pub async fn submit_external_input(&self, envelope: Envelope) -> Result<Status> {
        let after_turn =
            envelope.message.delivery == codex_protocol::external_input::Delivery::AfterTurn;
        let (status, inserted) = self.session.admit_external_input(envelope).await?;
        if inserted && after_turn {
            self.session
                .emit_thread_idle_lifecycle_if_idle(ThreadIdleCause::Completed)
                .await;
            self.session.dispatch_external_input().await;
        }
        Ok(status)
    }

    pub async fn external_input_status(
        &self,
        owner: &str,
        message_id: &str,
        digest: &str,
    ) -> Result<Status> {
        self.session
            .external_input_status(owner, message_id, digest)
            .await
    }

    pub async fn retry_external_input(
        &self,
        owner: &str,
        key: MessageKey,
        expected: Option<Uuid>,
        retry_id: &str,
    ) -> Result<()> {
        if self
            .session
            .retry_external_input(owner, key, expected, retry_id)
            .await?
        {
            self.session
                .emit_thread_idle_lifecycle_if_idle(ThreadIdleCause::Completed)
                .await;
            self.session.dispatch_external_input().await;
        }
        Ok(())
    }

    pub async fn pause_external_input(&self) -> Result<()> {
        self.session.external_input_gate(true).await
    }
}
