use super::*;
use std::time::Duration;
use std::time::Instant;

fn enable_notify_service(chat: &mut ChatWidget) {
    chat.config.notify_service.notify_service_url = Some("http://127.0.0.1:9/notify".to_string());
    chat.config.notify_service.notify_service_idle_timeout_secs = 60;
    chat.config
        .notify_service
        .notify_service_composer_idle_timeout_secs = 600;
    chat.config
        .notify_service
        .notify_service_approval_timeout_secs = 30;
}

fn enter_task_completed_idle(chat: &mut ChatWidget, entered_at: Instant) {
    chat.idle_entered_at = Some(entered_at);
    chat.idle_notification_status = Some(crate::notify_service::IdleNotifyStatus::TaskCompleted);
    chat.idle_composer_activity_generation = Some(chat.bottom_pane.composer_activity_generation());
    chat.idle_turn_duration_seconds = Some(7);
    chat.idle_notification_sent = false;
}

fn enter_waiting_approval_idle(chat: &mut ChatWidget, entered_at: Instant) {
    chat.idle_entered_at = Some(entered_at);
    chat.idle_notification_status = Some(crate::notify_service::IdleNotifyStatus::WaitingApproval);
    chat.idle_composer_activity_generation = Some(chat.bottom_pane.composer_activity_generation());
    chat.idle_turn_duration_seconds = None;
    chat.idle_notification_sent = false;
}

#[tokio::test]
async fn unchanged_composer_uses_short_idle_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    let entered_at = Instant::now();
    enter_task_completed_idle(&mut chat, entered_at);

    assert_eq!(
        chat.idle_notification_due_at(entered_at + Duration::from_secs(60)),
        Some((crate::notify_service::IdleNotifyStatus::TaskCompleted, 60))
    );
}

#[tokio::test]
async fn edited_then_empty_composer_skips_short_idle_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    let entered_at = Instant::now();
    enter_task_completed_idle(&mut chat, entered_at);

    chat.bottom_pane.insert_str("x");
    let _ = chat
        .bottom_pane
        .handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));

    assert!(chat.bottom_pane.composer_is_empty());
    assert_eq!(
        chat.idle_notification_due_at(entered_at + Duration::from_secs(60)),
        None
    );
}

#[tokio::test]
async fn changed_composer_uses_long_thinking_notification() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    let entered_at = Instant::now();
    enter_task_completed_idle(&mut chat, entered_at);

    chat.bottom_pane.insert_str("draft");
    let last_activity = chat
        .bottom_pane
        .last_composer_activity_at()
        .expect("composer activity recorded");

    assert_eq!(
        chat.idle_notification_due_at(last_activity + Duration::from_secs(600)),
        Some((
            crate::notify_service::IdleNotifyStatus::ThinkingTooLong,
            600
        ))
    );
}

#[tokio::test]
async fn approval_idle_uses_approval_timeout_even_after_composer_activity() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    let entered_at = Instant::now();
    enter_waiting_approval_idle(&mut chat, entered_at);

    chat.bottom_pane.insert_str("draft");

    assert_eq!(
        chat.idle_notification_due_at(entered_at + Duration::from_secs(29)),
        None
    );
    assert_eq!(
        chat.idle_notification_due_at(entered_at + Duration::from_secs(30)),
        Some((crate::notify_service::IdleNotifyStatus::WaitingApproval, 30))
    );
}

#[tokio::test]
async fn approval_response_clears_waiting_approval_idle_timer() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    let thread_id = ThreadId::new();
    chat.thread_id = Some(thread_id);
    enter_waiting_approval_idle(&mut chat, Instant::now());
    let pending_generation = chat.idle_notification_generation;

    chat.clear_waiting_approval_idle_state_for_thread(thread_id);

    assert_eq!(chat.idle_notification_status, None);
    assert_ne!(chat.idle_notification_generation, pending_generation);
    chat.handle_idle_notify_timer_fired(pending_generation);
    assert!(!chat.idle_notification_sent);
}

#[tokio::test]
async fn external_editor_state_change_recomputes_idle_timer() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    chat.config.notify_service.notify_service_idle_timeout_secs = 0;
    chat.enter_idle_state(
        crate::notify_service::IdleNotifyStatus::TaskCompleted,
        Some(7),
    );
    let generation = chat.idle_notification_generation;
    chat.set_external_editor_state(ExternalEditorState::Active);

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("idle timer event")
        .expect("event channel open");
    assert!(matches!(
        event,
        AppEvent::IdleNotifyTimerFired { generation: event_generation }
            if event_generation == generation
    ));
}

#[tokio::test]
async fn exit_clear_invalidates_pending_idle_timer() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    enter_task_completed_idle(&mut chat, Instant::now());
    let pending_generation = chat.idle_notification_generation;

    chat.clear_idle_state_for_exit();

    assert_eq!(chat.idle_notification_status, None);
    assert_ne!(chat.idle_notification_generation, pending_generation);
    chat.handle_idle_notify_timer_fired(pending_generation);
    assert!(!chat.idle_notification_sent);
}

#[tokio::test]
async fn startup_idle_timer_is_generation_guarded() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    chat.config.notify_service.notify_service_events =
        vec![codex_config::types::NotifyServiceEvent::SessionStartupIdle];
    chat.config
        .notify_service
        .notify_service_startup_idle_timeout_secs = 0;
    chat.session_startup_idle_entered_at = Some(Instant::now());
    chat.session_startup_idle_notification_sent = false;
    chat.session_startup_idle_generation = 11;
    chat.schedule_next_session_startup_idle_timer();

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("startup idle timer event")
        .expect("event channel open");
    assert!(matches!(
        event,
        AppEvent::SessionStartupIdleTimerFired { generation: 11 }
    ));

    chat.clear_session_startup_idle_state();
    chat.handle_session_startup_idle_timer_fired(11);
    assert!(!chat.session_startup_idle_notification_sent);
}

#[test]
fn notify_service_defaults_are_real_timeouts() {
    let defaults = codex_config::types::NotifyServiceSettings::default();
    assert_eq!(defaults.notify_service_idle_timeout_secs, 60);
    assert_eq!(defaults.notify_service_composer_idle_timeout_secs, 600);
    assert_eq!(defaults.notify_service_approval_timeout_secs, 30);
    assert_eq!(defaults.notify_service_startup_idle_timeout_secs, 180);
    assert!(
        defaults
            .notify_service_events
            .contains(&codex_config::types::NotifyServiceEvent::TaskCompleted)
    );
    assert!(
        defaults
            .notify_service_events
            .contains(&codex_config::types::NotifyServiceEvent::SessionExit)
    );
    assert!(
        !defaults
            .notify_service_events
            .contains(&codex_config::types::NotifyServiceEvent::UserMessageSent)
    );
}

#[test]
fn payload_includes_turn_and_session_durations() {
    let payload = crate::notify_service::build_payload_with_details(
        crate::notify_service::IdleNotifyStatus::TaskCompleted,
        std::path::Path::new("/tmp/cutex-test"),
        "codex",
        Some("thread"),
        None,
        Some(Instant::now() - Duration::from_secs(123)),
        Some(7),
        &TokenUsage::default(),
        60,
        Some(serde_json::json!({"turn": {"follow_up_started": false}})),
    );

    assert_eq!(payload.duration_seconds, 7);
    assert_eq!(payload.turn_duration_seconds, Some(7));
    assert!(payload.session_duration_seconds >= 123);
    assert_eq!(
        payload
            .event_details
            .as_ref()
            .and_then(|details| details.pointer("/turn/follow_up_started"))
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn user_message_content_modes_are_bounded() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    let message = UserMessage::from("sample prompt");

    let metadata = chat.user_message_notify_details(&message);
    assert!(metadata.pointer("/user_message/text").is_none());

    chat.config
        .notify_service
        .notify_service_user_message_content =
        codex_config::types::NotifyServiceUserMessageContent::Preview;
    chat.config
        .notify_service
        .notify_service_user_message_preview_chars = 6;
    let preview = chat.user_message_notify_details(&message);
    assert_eq!(
        preview
            .pointer("/user_message/text_preview")
            .and_then(serde_json::Value::as_str),
        Some("sample")
    );
    assert_eq!(
        preview
            .pointer("/user_message/text_truncated")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );

    chat.config
        .notify_service
        .notify_service_user_message_content =
        codex_config::types::NotifyServiceUserMessageContent::Full;
    let full = chat.user_message_notify_details(&message);
    assert_eq!(
        full.pointer("/user_message/text")
            .and_then(serde_json::Value::as_str),
        Some("sample prompt")
    );
}

#[tokio::test]
async fn queued_message_marks_sent_notification_only_when_enabled() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);

    chat.queue_user_message(UserMessage::from("queued"));
    assert!(
        !chat
            .input_queue
            .queued_user_messages
            .front()
            .expect("queued message")
            .notify_sent
    );

    chat.input_queue.queued_user_messages.clear();
    chat.config.notify_service.notify_service_events =
        vec![codex_config::types::NotifyServiceEvent::UserMessageSent];
    chat.queue_user_message(UserMessage::from("queued"));
    assert!(
        chat.input_queue
            .queued_user_messages
            .front()
            .expect("queued message")
            .notify_sent
    );
}

#[tokio::test]
async fn replay_suppresses_lifecycle_side_effect_state() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    chat.config.notify_service.notify_service_events = vec![
        codex_config::types::NotifyServiceEvent::ContextCompacted,
        codex_config::types::NotifyServiceEvent::SessionStartupIdle,
    ];

    chat.handle_server_notification(
        ServerNotification::ContextCompacted(
            codex_app_server_protocol::ContextCompactedNotification {
                thread_id: "thread".to_string(),
                turn_id: "turn".to_string(),
            },
        ),
        Some(ReplayKind::ThreadSnapshot),
    );

    assert!(!chat.notify_service_replay_suppressed);
    assert_eq!(chat.idle_notification_status, None);
}

#[tokio::test]
async fn rejected_steer_retry_keeps_sent_disposition() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    chat.input_queue
        .rejected_steers_queue
        .push_back(UserMessage::from("rejected"));
    chat.input_queue
        .rejected_steer_history_records
        .push_back(UserMessageHistoryRecord::UserMessageText);

    let (queued, _) = chat
        .pop_next_queued_user_message()
        .expect("rejected steer should be retryable");
    assert!(queued.notify_sent);
}
