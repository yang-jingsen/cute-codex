use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
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
async fn approval_idle_uses_approval_notification_timeout() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    let entered_at = Instant::now();
    enter_waiting_approval_idle(&mut chat, entered_at);

    chat.bottom_pane.insert_str("y");

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
async fn exec_begin_clears_waiting_approval_idle() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    enter_waiting_approval_idle(&mut chat, Instant::now());

    begin_exec(&mut chat, "call-1", "echo ok");

    assert_eq!(chat.idle_notification_status, None);
}

#[tokio::test]
async fn patch_apply_begin_clears_waiting_approval_idle() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    enter_waiting_approval_idle(&mut chat, Instant::now());

    let mut changes = HashMap::new();
    changes.insert(
        PathBuf::from("foo.txt"),
        FileChange::Add {
            content: "hello\n".to_string(),
        },
    );
    chat.on_patch_apply_begin(changes);

    assert_eq!(chat.idle_notification_status, None);
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
async fn idle_state_schedules_app_event_timer_without_draw_tick() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(None).await;
    enable_notify_service(&mut chat);
    chat.config.notify_service.notify_service_idle_timeout_secs = 0;

    chat.enter_idle_state(
        crate::notify_service::IdleNotifyStatus::TaskCompleted,
        Some(7),
    );
    let expected_generation = chat.idle_notification_generation;

    let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("idle timer event")
        .expect("event channel open");
    match event {
        AppEvent::IdleNotifyTimerFired { generation } => {
            assert_eq!(generation, expected_generation);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn notify_service_defaults_are_real_timeouts() {
    let defaults = codex_config::types::NotifyServiceSettings::default();
    assert_eq!(defaults.notify_service_idle_timeout_secs, 60);
    assert_eq!(defaults.notify_service_composer_idle_timeout_secs, 600);
    assert_eq!(defaults.notify_service_approval_timeout_secs, 30);
    assert!(
        defaults
            .notify_service_events
            .contains(&codex_config::types::NotifyServiceEvent::TaskCompleted)
    );
    assert!(
        defaults
            .notify_service_events
            .contains(&codex_config::types::NotifyServiceEvent::ThinkingTooLong)
    );
    assert!(
        defaults
            .notify_service_events
            .contains(&codex_config::types::NotifyServiceEvent::WaitingApproval)
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
    let cwd = std::path::Path::new("/tmp/cutex-test");
    let session_started_at = Instant::now() - Duration::from_secs(123);

    let payload = crate::notify_service::build_payload_with_details(
        crate::notify_service::IdleNotifyStatus::TaskCompleted,
        cwd,
        "codex",
        Some("thread"),
        None,
        Some(session_started_at),
        Some(7),
        &TokenUsage::default(),
        60,
        None,
    );

    assert_eq!(payload.duration_seconds, 7);
    assert_eq!(payload.turn_duration_seconds, Some(7));
    assert!(payload.session_duration_seconds >= 123);
}

#[test]
fn payload_can_include_thread_id_and_event_details() {
    let payload = crate::notify_service::build_payload_with_details(
        crate::notify_service::IdleNotifyStatus::SessionStarted,
        std::path::Path::new("/tmp/cutex-test"),
        "codex",
        Some("thread"),
        Some("thread-id".to_string()),
        None,
        None,
        &TokenUsage::default(),
        0,
        Some(serde_json::json!({"session": {"resumed": false}})),
    );

    assert_eq!(payload.thread_id.as_deref(), Some("thread-id"));
    assert_eq!(
        payload
            .event_details
            .as_ref()
            .and_then(|details| details.pointer("/session/resumed"))
            .and_then(serde_json::Value::as_bool),
        Some(false)
    );
}

#[tokio::test]
async fn user_message_content_defaults_to_metadata_only() {
    let (chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    let user_message = UserMessage::from("sample prompt");

    let details = chat.user_message_notify_details(&user_message);
    let user_message_details = details
        .pointer("/user_message")
        .expect("user message details");

    assert_eq!(
        user_message_details
            .get("text_chars")
            .and_then(serde_json::Value::as_u64),
        Some(13)
    );
    assert!(user_message_details.get("text").is_none());
    assert!(user_message_details.get("text_preview").is_none());
}

#[tokio::test]
async fn user_message_content_preview_is_truncated() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    chat.config
        .notify_service
        .notify_service_user_message_content =
        codex_config::types::NotifyServiceUserMessageContent::Preview;
    chat.config
        .notify_service
        .notify_service_user_message_preview_chars = 6;
    let user_message = UserMessage::from("sample prompt");

    let details = chat.user_message_notify_details(&user_message);

    assert_eq!(
        details
            .pointer("/user_message/text_preview")
            .and_then(serde_json::Value::as_str),
        Some("sample-token")
    );
    assert_eq!(
        details
            .pointer("/user_message/text_truncated")
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
}

#[tokio::test]
async fn user_message_content_full_includes_text() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(None).await;
    chat.config
        .notify_service
        .notify_service_user_message_content =
        codex_config::types::NotifyServiceUserMessageContent::Full;
    let user_message = UserMessage::from("sample prompt");

    let details = chat.user_message_notify_details(&user_message);

    assert_eq!(
        details
            .pointer("/user_message/text")
            .and_then(serde_json::Value::as_str),
        Some("sample prompt")
    );
    assert!(details.pointer("/user_message/text_preview").is_none());
}

#[tokio::test]
async fn user_message_sent_is_opt_in_and_marks_queued_message_once() {
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
