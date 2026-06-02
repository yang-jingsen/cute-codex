//! Structured HTTP notification hooks for `ChatWidget`.

use super::*;
use crate::app::app_server_requests::ResolvedAppServerRequest;
use codex_config::types::NotifyServiceEvent;
use codex_config::types::NotifyServiceUserMessageContent;

impl ChatWidget {
    fn notify_service_enabled(&self) -> bool {
        self.config
            .notify_service
            .notify_service_url
            .as_ref()
            .is_some_and(|url| !url.is_empty())
    }

    fn notify_service_event_enabled(&self, status: NotifyServiceEvent) -> bool {
        self.notify_service_enabled()
            && crate::notify_service::event_enabled(&self.config.notify_service, status)
    }

    fn notify_service_should_track_idle(&self, status: NotifyServiceEvent) -> bool {
        if !self.notify_service_enabled() {
            return false;
        }
        match status {
            NotifyServiceEvent::TaskCompleted => {
                self.notify_service_event_enabled(NotifyServiceEvent::TaskCompleted)
                    || self.notify_service_event_enabled(NotifyServiceEvent::ThinkingTooLong)
            }
            other => self.notify_service_event_enabled(other),
        }
    }

    pub(super) fn current_turn_duration_seconds(&self) -> Option<u64> {
        self.bottom_pane
            .status_widget()
            .map(crate::status_indicator_widget::StatusIndicatorWidget::elapsed_seconds)
    }

    pub(super) fn post_notify_service_event(
        &self,
        status: NotifyServiceEvent,
        turn_duration_seconds: Option<u64>,
        idle_seconds: u64,
        event_details: Option<serde_json::Value>,
    ) -> bool {
        let ns = &self.config.notify_service;
        let Some(url) = ns.notify_service_url.as_ref().filter(|url| !url.is_empty()) else {
            return false;
        };
        if !crate::notify_service::event_enabled(ns, status) {
            return false;
        }

        let cwd = self
            .current_cwd
            .clone()
            .unwrap_or_else(|| self.config.cwd.as_path().to_path_buf());
        let usage = self.token_usage();
        let payload = crate::notify_service::build_payload_with_details(
            status,
            &cwd,
            &ns.notify_service_agent_name,
            self.thread_name.as_deref(),
            self.thread_id.map(|id| id.to_string()),
            self.session_started_at,
            turn_duration_seconds,
            &usage,
            idle_seconds,
            event_details,
        );
        crate::notify_service::send_idle_notification(
            url.clone(),
            ns.notify_service_token.clone(),
            payload,
        );
        true
    }

    pub(super) fn user_message_notify_details(
        &self,
        user_message: &UserMessage,
    ) -> serde_json::Value {
        let text_chars = user_message.text.chars().count();
        let mut details = serde_json::json!({
            "user_message": {
                "has_text": !user_message.text.is_empty(),
                "text_chars": text_chars,
                "local_image_count": user_message.local_images.len(),
                "remote_image_count": user_message.remote_image_urls.len(),
                "mention_count": user_message.mention_bindings.len()
            }
        });

        let Some(user_message_details) = details
            .as_object_mut()
            .and_then(|root| root.get_mut("user_message"))
            .and_then(serde_json::Value::as_object_mut)
        else {
            return details;
        };

        match self
            .config
            .notify_service
            .notify_service_user_message_content
        {
            NotifyServiceUserMessageContent::None => {}
            NotifyServiceUserMessageContent::Preview => {
                let preview_chars = self
                    .config
                    .notify_service
                    .notify_service_user_message_preview_chars;
                let preview = user_message
                    .text
                    .chars()
                    .take(preview_chars)
                    .collect::<String>();
                user_message_details.insert(
                    "text_preview".to_string(),
                    serde_json::Value::String(preview),
                );
                user_message_details.insert(
                    "text_truncated".to_string(),
                    serde_json::Value::Bool(text_chars > preview_chars),
                );
            }
            NotifyServiceUserMessageContent::Full => {
                user_message_details.insert(
                    "text".to_string(),
                    serde_json::Value::String(user_message.text.clone()),
                );
            }
        }

        details
    }

    pub(super) fn emit_user_message_sent_notification(
        &mut self,
        user_message: &UserMessage,
    ) -> bool {
        self.clear_idle_state();
        self.clear_session_startup_idle_state();
        let details = self.user_message_notify_details(user_message);
        self.post_notify_service_event(NotifyServiceEvent::UserMessageSent, None, 0, Some(details))
    }

    pub(super) fn emit_user_message_dispatched_notification(
        &self,
        user_message: &UserMessage,
        dispatched_from_queue: bool,
    ) -> bool {
        let mut details = self.user_message_notify_details(user_message);
        if let Some(user_message_details) = details
            .as_object_mut()
            .and_then(|root| root.get_mut("user_message"))
            .and_then(serde_json::Value::as_object_mut)
        {
            user_message_details.insert(
                "dispatched_from_queue".to_string(),
                serde_json::Value::Bool(dispatched_from_queue),
            );
        }
        self.post_notify_service_event(
            NotifyServiceEvent::UserMessageDispatched,
            None,
            0,
            Some(details),
        )
    }

    pub(super) fn maybe_emit_session_started_notification(
        &mut self,
        session: &ThreadSessionState,
        display: SessionConfiguredDisplay,
        previous_thread_id: Option<ThreadId>,
    ) {
        if display != SessionConfiguredDisplay::Normal {
            return;
        }
        if previous_thread_id == Some(session.thread_id)
            || self.notify_service_session_started_sent_for == Some(session.thread_id)
        {
            return;
        }
        let details = serde_json::json!({
            "session": {
                "model": session.model.clone(),
                "model_provider_id": session.model_provider_id.clone(),
                "resumed": session.message_history.is_some(),
                "forked_from_id": session.forked_from_id.as_ref().map(ToString::to_string)
            }
        });
        if self.post_notify_service_event(
            NotifyServiceEvent::SessionStarted,
            None,
            0,
            Some(details),
        ) {
            self.notify_service_session_started_sent_for = Some(session.thread_id);
        }
    }

    pub(super) fn maybe_track_session_startup_idle(
        &mut self,
        session: &ThreadSessionState,
        display: SessionConfiguredDisplay,
        previous_thread_id: Option<ThreadId>,
    ) {
        if display != SessionConfiguredDisplay::Normal {
            self.clear_session_startup_idle_state();
            return;
        }
        if previous_thread_id == Some(session.thread_id) {
            return;
        }
        self.enter_session_startup_idle_state();
    }

    fn enter_session_startup_idle_state(&mut self) {
        self.clear_session_startup_idle_state();
        if !self.notify_service_event_enabled(NotifyServiceEvent::SessionStartupIdle) {
            return;
        }
        self.session_startup_idle_entered_at = Some(Instant::now());
        self.session_startup_idle_notification_sent = false;
        self.schedule_next_session_startup_idle_timer();
    }

    pub(super) fn clear_session_startup_idle_state(&mut self) {
        self.session_startup_idle_entered_at = None;
        self.session_startup_idle_notification_sent = false;
        self.session_startup_idle_generation = self.session_startup_idle_generation.wrapping_add(1);
        self.session_startup_idle_timer_due_at = None;
    }

    fn fire_session_startup_idle_notification(&mut self, idle_seconds: u64) {
        self.session_startup_idle_notification_sent = self.post_notify_service_event(
            NotifyServiceEvent::SessionStartupIdle,
            None,
            idle_seconds,
            Some(serde_json::json!({
                "session": {
                    "model": self.current_model(),
                    "startup_idle_timeout_seconds": self
                        .config
                        .notify_service
                        .notify_service_startup_idle_timeout_secs
                }
            })),
        );
        self.session_startup_idle_timer_due_at = None;
    }

    pub(crate) fn emit_approval_resolved_notification(
        &self,
        request: &ResolvedAppServerRequest,
    ) -> bool {
        let details = match request {
            ResolvedAppServerRequest::ExecApproval { id } => serde_json::json!({
                "approval": { "kind": "exec", "approval_id": id, "item_id": id }
            }),
            ResolvedAppServerRequest::FileChangeApproval { id } => serde_json::json!({
                "approval": { "kind": "patch", "item_id": id }
            }),
            ResolvedAppServerRequest::PermissionsApproval { id } => serde_json::json!({
                "approval": { "kind": "permissions", "item_id": id }
            }),
            ResolvedAppServerRequest::UserInput { call_id } => serde_json::json!({
                "approval": { "kind": "user_input", "item_id": call_id }
            }),
            ResolvedAppServerRequest::McpElicitation {
                server_name,
                request_id,
            } => serde_json::json!({
                "approval": {
                    "kind": "elicitation",
                    "server_name": server_name,
                    "request_id": request_id
                }
            }),
        };
        self.post_notify_service_event(
            NotifyServiceEvent::ApprovalResolved,
            self.current_turn_duration_seconds(),
            0,
            Some(details),
        )
    }

    #[allow(dead_code)]
    pub(crate) fn emit_connection_error_notification(&self, message: &str) -> bool {
        self.post_notify_service_event(
            NotifyServiceEvent::ConnectionError,
            self.current_turn_duration_seconds(),
            0,
            Some(serde_json::json!({
                "connection": {
                    "scope": "app_server",
                    "message": message
                }
            })),
        )
    }

    pub(super) fn emit_started_notification_for_thread_item(&self, item: &ThreadItem) {
        match item {
            ThreadItem::CommandExecution {
                id,
                cwd,
                process_id,
                source,
                command_actions,
                ..
            } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::CommandExecutionStarted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "command_execution": {
                            "item_id": id,
                            "cwd": cwd.display().to_string(),
                            "process_id": process_id,
                            "source": source,
                            "action_count": command_actions.len()
                        }
                    })),
                );
            }
            ThreadItem::FileChange { id, changes, .. } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::PatchApplyStarted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "patch_apply": { "item_id": id, "change_count": changes.len() }
                    })),
                );
            }
            ThreadItem::McpToolCall {
                id,
                server,
                tool,
                mcp_app_resource_uri,
                ..
            } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::McpToolCallStarted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "mcp_tool_call": {
                            "item_id": id,
                            "server": server,
                            "tool": tool,
                            "mcp_app_resource_uri": mcp_app_resource_uri
                        }
                    })),
                );
            }
            ThreadItem::WebSearch { id, .. } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::WebSearchStarted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({ "web_search": { "item_id": id } })),
                );
            }
            ThreadItem::ImageGeneration { id, .. } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::ImageGenerationStarted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({ "image_generation": { "item_id": id } })),
                );
            }
            ThreadItem::EnteredReviewMode { id, review } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::ReviewStarted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "review": { "item_id": id, "review_chars": review.chars().count() }
                    })),
                );
            }
            _ => {}
        }
    }

    pub(super) fn emit_completed_notification_for_thread_item(&self, item: &ThreadItem) {
        match item {
            ThreadItem::CommandExecution {
                id,
                cwd,
                process_id,
                source,
                command_actions,
                exit_code,
                duration_ms,
                ..
            } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::CommandExecutionCompleted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "command_execution": {
                            "item_id": id,
                            "cwd": cwd.display().to_string(),
                            "process_id": process_id,
                            "source": source,
                            "action_count": command_actions.len(),
                            "exit_code": exit_code,
                            "duration_ms": duration_ms
                        }
                    })),
                );
            }
            ThreadItem::FileChange {
                id,
                changes,
                status,
            } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::PatchApplyCompleted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "patch_apply": {
                            "item_id": id,
                            "change_count": changes.len(),
                            "status": status
                        }
                    })),
                );
            }
            ThreadItem::McpToolCall {
                id,
                server,
                tool,
                status,
                mcp_app_resource_uri,
                duration_ms,
                error,
                ..
            } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::McpToolCallCompleted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "mcp_tool_call": {
                            "item_id": id,
                            "server": server,
                            "tool": tool,
                            "status": status,
                            "mcp_app_resource_uri": mcp_app_resource_uri,
                            "duration_ms": duration_ms,
                            "success": error.is_none()
                        }
                    })),
                );
            }
            ThreadItem::WebSearch { id, action, .. } => {
                let action_type = match action {
                    Some(codex_app_server_protocol::WebSearchAction::Search { .. }) => "search",
                    Some(codex_app_server_protocol::WebSearchAction::OpenPage { .. }) => {
                        "open_page"
                    }
                    Some(codex_app_server_protocol::WebSearchAction::FindInPage { .. }) => {
                        "find_in_page"
                    }
                    Some(codex_app_server_protocol::WebSearchAction::Other) | None => "other",
                };
                self.post_notify_service_event(
                    NotifyServiceEvent::WebSearchCompleted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "web_search": { "item_id": id, "action_type": action_type }
                    })),
                );
            }
            ThreadItem::ImageGeneration {
                id,
                status,
                saved_path,
                ..
            } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::ImageGenerationCompleted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "image_generation": {
                            "item_id": id,
                            "status": status,
                            "saved_path": saved_path
                        }
                    })),
                );
            }
            ThreadItem::ExitedReviewMode { id, review } => {
                self.post_notify_service_event(
                    NotifyServiceEvent::ReviewCompleted,
                    self.current_turn_duration_seconds(),
                    0,
                    Some(serde_json::json!({
                        "review": { "item_id": id, "review_chars": review.chars().count() }
                    })),
                );
            }
            _ => {}
        }
    }

    pub(super) fn enter_idle_state(
        &mut self,
        status: crate::notify_service::IdleNotifyStatus,
        turn_duration_seconds: Option<u64>,
    ) {
        if !self.notify_service_should_track_idle(status) {
            return;
        }
        self.idle_entered_at = Some(Instant::now());
        self.idle_notification_status = Some(status);
        self.idle_composer_activity_generation =
            Some(self.bottom_pane.composer_activity_generation());
        self.idle_turn_duration_seconds = turn_duration_seconds;
        self.idle_notification_sent = false;
        self.idle_notification_generation = self.idle_notification_generation.wrapping_add(1);
        self.idle_timer_due_at = None;
        self.schedule_next_idle_notification_timer();
    }

    pub(super) fn clear_idle_state(&mut self) {
        self.idle_entered_at = None;
        self.idle_notification_status = None;
        self.idle_composer_activity_generation = None;
        self.idle_turn_duration_seconds = None;
        self.idle_notification_sent = false;
        self.idle_notification_generation = self.idle_notification_generation.wrapping_add(1);
        self.idle_timer_due_at = None;
    }

    pub(super) fn clear_waiting_approval_idle_state(&mut self) {
        if matches!(
            self.idle_notification_status,
            Some(crate::notify_service::IdleNotifyStatus::WaitingApproval)
        ) {
            self.clear_idle_state();
        }
    }

    pub(crate) fn clear_waiting_approval_idle_state_for_thread(&mut self, thread_id: ThreadId) {
        if self.thread_id == Some(thread_id)
            || (self.thread_id.is_none() && thread_id == ThreadId::default())
        {
            self.clear_waiting_approval_idle_state();
        }
    }

    pub(crate) fn clear_idle_state_for_exit(&mut self) {
        self.clear_idle_state();
        self.clear_session_startup_idle_state();
    }

    fn fire_idle_notification(
        &mut self,
        status: crate::notify_service::IdleNotifyStatus,
        idle_seconds: u64,
    ) {
        self.idle_notification_sent = self.post_notify_service_event(
            status,
            self.idle_turn_duration_seconds,
            idle_seconds,
            None,
        );
        self.idle_timer_due_at = None;
    }

    fn idle_timeout_for_status(&self, status: crate::notify_service::IdleNotifyStatus) -> Duration {
        match status {
            crate::notify_service::IdleNotifyStatus::WaitingApproval => Duration::from_secs(
                self.config
                    .notify_service
                    .notify_service_approval_timeout_secs,
            ),
            _ => Duration::from_secs(self.config.notify_service.notify_service_idle_timeout_secs),
        }
    }

    fn idle_task_completed_composer_changed(&self) -> bool {
        self.idle_composer_activity_generation
            .is_some_and(|generation| generation != self.bottom_pane.composer_activity_generation())
            || self.external_editor_state != ExternalEditorState::Closed
    }

    fn idle_task_completed_composer_idle_started_at(&self, entered_at: Instant) -> Instant {
        self.bottom_pane
            .last_composer_activity_at()
            .filter(|last_activity_at| *last_activity_at > entered_at)
            .unwrap_or(entered_at)
    }

    pub(super) fn idle_notification_due_at(
        &self,
        now: Instant,
    ) -> Option<(crate::notify_service::IdleNotifyStatus, u64)> {
        let entered_at = self.idle_entered_at?;
        let status = self.idle_notification_status?;
        let elapsed = now.checked_duration_since(entered_at)?;
        if !matches!(
            status,
            crate::notify_service::IdleNotifyStatus::TaskCompleted
        ) {
            let timeout = self.idle_timeout_for_status(status);
            if elapsed >= timeout {
                return Some((status, elapsed.as_secs()));
            }
            return None;
        }

        if !self.idle_task_completed_composer_changed() {
            let short_timeout =
                Duration::from_secs(self.config.notify_service.notify_service_idle_timeout_secs);
            if elapsed >= short_timeout
                && self.notify_service_event_enabled(NotifyServiceEvent::TaskCompleted)
            {
                return Some((status, elapsed.as_secs()));
            }
            return None;
        }

        let long_timeout = Duration::from_secs(
            self.config
                .notify_service
                .notify_service_composer_idle_timeout_secs,
        );
        let started_at = self.idle_task_completed_composer_idle_started_at(entered_at);
        let composer_elapsed = now.checked_duration_since(started_at)?;
        if composer_elapsed >= long_timeout
            && self.notify_service_event_enabled(NotifyServiceEvent::ThinkingTooLong)
        {
            return Some((
                crate::notify_service::IdleNotifyStatus::ThinkingTooLong,
                composer_elapsed.as_secs(),
            ));
        }
        None
    }

    fn idle_notification_next_due_at(&self, now: Instant) -> Option<Instant> {
        if self.idle_notification_sent {
            return None;
        }
        let entered_at = self.idle_entered_at?;
        let status = self.idle_notification_status?;
        if matches!(
            status,
            crate::notify_service::IdleNotifyStatus::TaskCompleted
        ) && self.idle_task_completed_composer_changed()
        {
            if !self.notify_service_event_enabled(NotifyServiceEvent::ThinkingTooLong) {
                return None;
            }
            let long_timeout = Duration::from_secs(
                self.config
                    .notify_service
                    .notify_service_composer_idle_timeout_secs,
            );
            let started_at = self.idle_task_completed_composer_idle_started_at(entered_at);
            return started_at.checked_add(long_timeout).or(Some(now));
        }

        if !self.notify_service_event_enabled(status) {
            return None;
        }
        entered_at
            .checked_add(self.idle_timeout_for_status(status))
            .or(Some(now))
    }

    fn schedule_next_idle_notification_timer(&mut self) {
        let now = Instant::now();
        let Some(due_at) = self.idle_notification_next_due_at(now) else {
            return;
        };
        if self
            .idle_timer_due_at
            .is_some_and(|scheduled_at| scheduled_at <= due_at && scheduled_at > now)
        {
            return;
        }

        self.idle_timer_due_at = Some(due_at);
        let delay = due_at.saturating_duration_since(now);
        let generation = self.idle_notification_generation;
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            app_event_tx.send(AppEvent::IdleNotifyTimerFired { generation });
        });
    }

    pub(crate) fn check_idle_notification_timer(&mut self) {
        if self.idle_notification_sent {
            return;
        }
        let now = Instant::now();
        if let Some((status, idle_seconds)) = self.idle_notification_due_at(now) {
            self.fire_idle_notification(status, idle_seconds);
        } else {
            self.schedule_next_idle_notification_timer();
        }
    }

    pub(crate) fn handle_idle_notify_timer_fired(&mut self, generation: u64) {
        if generation != self.idle_notification_generation {
            return;
        }
        self.idle_timer_due_at = None;
        self.check_idle_notification_timer();
    }

    fn session_startup_idle_next_due_at(&self, now: Instant) -> Option<Instant> {
        if self.session_startup_idle_notification_sent {
            return None;
        }
        let entered_at = self.session_startup_idle_entered_at?;
        entered_at
            .checked_add(Duration::from_secs(
                self.config
                    .notify_service
                    .notify_service_startup_idle_timeout_secs,
            ))
            .or(Some(now))
    }

    fn schedule_next_session_startup_idle_timer(&mut self) {
        let now = Instant::now();
        let Some(due_at) = self.session_startup_idle_next_due_at(now) else {
            return;
        };
        if self
            .session_startup_idle_timer_due_at
            .is_some_and(|scheduled_at| scheduled_at <= due_at && scheduled_at > now)
        {
            return;
        }

        self.session_startup_idle_timer_due_at = Some(due_at);
        let delay = due_at.saturating_duration_since(now);
        let generation = self.session_startup_idle_generation;
        let app_event_tx = self.app_event_tx.clone();
        tokio::spawn(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            app_event_tx.send(AppEvent::SessionStartupIdleTimerFired { generation });
        });
    }

    fn check_session_startup_idle_timer(&mut self) {
        if self.session_startup_idle_notification_sent {
            return;
        }
        let Some(entered_at) = self.session_startup_idle_entered_at else {
            return;
        };
        let now = Instant::now();
        let Some(elapsed) = now.checked_duration_since(entered_at) else {
            return;
        };
        let timeout = Duration::from_secs(
            self.config
                .notify_service
                .notify_service_startup_idle_timeout_secs,
        );
        if elapsed >= timeout {
            self.fire_session_startup_idle_notification(elapsed.as_secs());
        } else {
            self.schedule_next_session_startup_idle_timer();
        }
    }

    pub(crate) fn handle_session_startup_idle_timer_fired(&mut self, generation: u64) {
        if generation != self.session_startup_idle_generation {
            return;
        }
        self.session_startup_idle_timer_due_at = None;
        self.check_session_startup_idle_timer();
    }

    pub(crate) fn session_started_at(&self) -> Option<Instant> {
        self.session_started_at
    }
}
