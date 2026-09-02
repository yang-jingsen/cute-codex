use super::*;
use pretty_assertions::assert_eq;

fn inter_agent_message_item() -> AppServerThreadItem {
    AppServerThreadItem::InterAgentMessage {
        id: "message-1".to_string(),
        author: "/root/sender".to_string(),
        recipient: "/root/receiver".to_string(),
        other_recipients: vec!["/root/observer".to_string()],
        content: "Please recheck the focused tests.\nKeep the output compact.".to_string(),
        delivery_mode: codex_app_server_protocol::InterAgentDeliveryMode::AfterTurn,
        author_metadata: None,
        recipient_metadata: None,
        task_service_presentation: None,
    }
}

#[tokio::test]
async fn canonical_inter_agent_item_live_and_replay_render_once_and_match() {
    let (mut live_chat, mut live_rx, _live_ops) =
        make_chatwidget_manual(/*model_override*/ None).await;
    let item = inter_agent_message_item();

    live_chat.handle_server_notification(
        ServerNotification::ItemStarted(ItemStartedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
            item: item.clone(),
        }),
        /*replay_kind*/ None,
    );
    assert!(drain_insert_history(&mut live_rx).is_empty());

    live_chat.handle_server_notification(
        ServerNotification::ItemCompleted(ItemCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 1,
            item: item.clone(),
        }),
        /*replay_kind*/ None,
    );
    let live_cells = drain_insert_history(&mut live_rx);
    assert_eq!(live_cells.len(), 1);
    let live_rendered = lines_to_single_string(&live_cells[0]);

    let (mut replay_chat, mut replay_rx, _replay_ops) =
        make_chatwidget_manual(/*model_override*/ None).await;
    replay_chat.replay_thread_item(
        item,
        "turn-1".to_string(),
        ReplayKind::ResumeInitialMessages,
    );
    let replay_cells = drain_insert_history(&mut replay_rx);
    assert_eq!(replay_cells.len(), 1);
    let replay_rendered = lines_to_single_string(&replay_cells[0]);

    assert_eq!(live_rendered, replay_rendered);
    assert_eq!(live_rendered.matches("Agent message").count(), 1);
    assert_chatwidget_snapshot!(
        "canonical_inter_agent_message_live_and_replay",
        live_rendered
    );
}

#[tokio::test]
async fn tagged_cutex_inter_agent_item_uses_live_custom_renderer_only() {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.config.tui_cutex_activity.inbound_message.label =
        Some("{author} → {recipient}\n{role} · {mode}".to_string());
    chat.config.tui_cutex_activity.inbound_message.show_metadata = false;
    chat.config
        .tui_cutex_activity
        .inbound_message
        .content_indent = 4;
    let item = AppServerThreadItem::InterAgentMessage {
        id: "message-1".to_string(),
        author: "/root/sender".to_string(),
        recipient: "/root/receiver".to_string(),
        other_recipients: Vec::new(),
        content: "Review the result.".to_string(),
        delivery_mode: codex_app_server_protocol::InterAgentDeliveryMode::Soon,
        author_metadata: Some(codex_app_server_protocol::CutexParticipantPresentation {
            display_name: Some("Worker".to_string()),
            role: Some("Reviewer".to_string()),
            ..Default::default()
        }),
        recipient_metadata: Some(codex_app_server_protocol::CutexParticipantPresentation {
            display_name: Some("Director".to_string()),
            ..Default::default()
        }),
        task_service_presentation: None,
    };

    chat.handle_server_notification(
        ServerNotification::ItemStarted(ItemStartedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            started_at_ms: 0,
            item: item.clone(),
        }),
        None,
    );
    assert!(drain_insert_history(&mut rx).is_empty());
    chat.handle_server_notification(
        ServerNotification::ItemCompleted(ItemCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 1,
            item,
        }),
        None,
    );

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1);
    let rendered = lines_to_single_string(&cells[0]);
    assert!(rendered.contains("Worker → Director\nReviewer · soon"));
    assert!(rendered.contains("    Review the result."));
    assert!(!rendered.contains("Agent message"));
}

#[tokio::test]
async fn authenticated_task_service_item_uses_dedicated_renderer_without_duplicate_history() {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.config
        .tui_cutex_activity
        .task_service_message
        .assignment
        .template = Some("Assigned {task_name} · {assignment_id}".to_string());
    let item = AppServerThreadItem::InterAgentMessage {
        id: "external-message-not-shown".to_string(),
        author: "/root/cutex_task_service".to_string(),
        recipient: "/root".to_string(),
        other_recipients: Vec::new(),
        content: "raw provider envelope must not be rendered".to_string(),
        delivery_mode: codex_app_server_protocol::InterAgentDeliveryMode::Soon,
        author_metadata: None,
        recipient_metadata: None,
        task_service_presentation: Some(
            codex_app_server_protocol::TaskServiceMessagePresentation {
                class: codex_app_server_protocol::TaskServiceMessageClass::Assignment,
                project_id: None,
                task_name: "shared-config-r11".to_string(),
                assignment_id: "assignment-01".to_string(),
                transition: None,
                semantic_payload: "Implement the exact contract once.".to_string(),
            },
        ),
    };
    chat.handle_server_notification(
        ServerNotification::ItemCompleted(ItemCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 1,
            item,
        }),
        None,
    );
    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1);
    let rendered = lines_to_single_string(&cells[0]);
    assert!(rendered.contains("Assigned shared-config-r11 · assignment-01"));
    assert!(rendered.contains("Implement the exact contract once."));
    assert!(!rendered.contains("raw provider envelope"));
    assert!(!rendered.contains("external-message-not-shown"));
}
