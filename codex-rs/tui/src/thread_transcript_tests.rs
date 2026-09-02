use super::*;
use crate::legacy_core::config::ConfigBuilder;
use codex_app_server_protocol::AgentManagementPhase;
use codex_app_server_protocol::InterAgentDeliveryMode;
use codex_app_server_protocol::ManagedAgentActivityItem;
use codex_app_server_protocol::ManagedAgentActivityStatus;
use codex_app_server_protocol::ManagedAgentOperation;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnItemsView;
use codex_app_server_protocol::TurnStatus;
use codex_utils_absolute_path::AbsolutePathBuf;

#[test]
fn reconstructed_transcript_inter_agent_message_snapshot() {
    let thread_id = ThreadId::new();
    let thread = Thread {
        id: thread_id.to_string(),
        extra: None,
        session_id: thread_id.to_string(),
        forked_from_id: None,
        parent_thread_id: None,
        project_id: None,
        preview: "preview".to_string(),
        ephemeral: false,
        section: None,
        section_entered_at: None,
        history_mode: Default::default(),
        model_provider: "openai".to_string(),
        created_at: 1,
        updated_at: 2,
        recency_at: Some(2),
        status: ThreadStatus::Idle,
        path: None,
        cwd: AbsolutePathBuf::try_from("/tmp").expect("absolute cwd"),
        cli_version: "0.0.0".to_string(),
        source: SessionSource::Cli,
        can_accept_direct_input: None,
        thread_source: None,
        agent_nickname: None,
        agent_role: None,
        git_info: None,
        name: None,
        turns: vec![Turn {
            id: "turn-1".to_string(),
            items_view: TurnItemsView::Full,
            items: vec![ThreadItem::InterAgentMessage {
                id: "message-1".to_string(),
                author: "/root/sender".to_string(),
                recipient: "/root/receiver".to_string(),
                other_recipients: vec!["/root/observer".to_string()],
                content: "Persisted inbound content".to_string(),
                delivery_mode: InterAgentDeliveryMode::Passive,
                author_metadata: None,
                recipient_metadata: None,
                task_service_presentation: None,
            }],
            status: TurnStatus::Completed,
            error: None,
            started_at: None,
            completed_at: None,
            duration_ms: None,
        }],
    };

    let rendered = thread_to_transcript_cells(
        thread,
        RawReasoningVisibility::Hidden,
        /*codex_home*/ None,
    )
    .into_iter()
    .flat_map(|cell| cell.transcript_lines(/*width*/ 80))
    .map(|line| line.to_string())
    .collect::<Vec<_>>()
    .join("\n");

    insta::assert_snapshot!(rendered);
}

#[tokio::test]
async fn thread_read_uses_current_cutex_presentation_without_old_dividers() {
    let codex_home = tempfile::tempdir().expect("temporary Codex home");
    let mut config = ConfigBuilder::default()
        .codex_home(codex_home.path().to_path_buf())
        .build()
        .await
        .expect("build test config");
    config.tui_cutex_activity.managed_agent.completed = Some("{agent} 已取餐".into());
    let item = ThreadItem::ManagedAgentActivity {
        activity: ManagedAgentActivityItem {
            id: "action-1".into(),
            event_id: "event-1".into(),
            sequence: 1,
            occurred_at_ms: 1_725_000_123_000,
            project_id: None,
            operation: ManagedAgentOperation::Create,
            status: ManagedAgentActivityStatus::Completed,
            action_id: None,
            phase_event_id: None,
            phase: None,
            managed_agent_id: "cutex.worker".into(),
            managed_agent_name: Some("Agent".into()),
            managed_agent_metadata: None,
            predecessor_agent_id: None,
            predecessor_agent_name: None,
            predecessor_metadata: None,
            successor_agent_id: None,
            successor_agent_name: None,
            successor_metadata: None,
            replace_policy: None,
            rotation_mode: None,
            authority_epoch: None,
            managed_agent_role: None,
            initial_task_preview: None,
            detail: None,
            runtime_generation: None,
        },
    };

    let cells = thread_items_to_transcript_cells(
        None,
        &AbsolutePathBuf::try_from("/tmp").expect("absolute cwd"),
        [item.clone()],
        RawReasoningVisibility::Hidden,
        Some(&config),
    );
    let rendered = cells
        .iter()
        .flat_map(|cell| cell.transcript_lines(/*width*/ 80))
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Agent 已取餐"));
    assert!(!rendered.contains("Worked for"));
    assert!(!rendered.contains('─'));

    config.tui_cutex_activity.managed_agent_visible = false;
    assert!(
        thread_items_to_transcript_cells(
            None,
            &AbsolutePathBuf::try_from("/tmp").expect("absolute cwd"),
            [item],
            RawReasoningVisibility::Hidden,
            Some(&config),
        )
        .is_empty()
    );
}

#[tokio::test]
async fn thread_transcript_does_not_reconstruct_live_agent_management_phases() {
    let item = ThreadItem::ManagedAgentActivity {
        activity: ManagedAgentActivityItem {
            id: "action-1".into(),
            event_id: "envelope-44".into(),
            sequence: 44,
            occurred_at_ms: 1_725_000_123_000,
            project_id: Some("project-1".into()),
            operation: ManagedAgentOperation::Create,
            status: ManagedAgentActivityStatus::InProgress,
            action_id: Some("action-1".into()),
            phase_event_id: Some("agent-management:action-1:phase:3".into()),
            phase: Some(AgentManagementPhase::Configured),
            managed_agent_id: "cutex.worker".into(),
            managed_agent_name: Some("Worker".into()),
            managed_agent_metadata: None,
            predecessor_agent_id: None,
            predecessor_agent_name: None,
            predecessor_metadata: None,
            successor_agent_id: None,
            successor_agent_name: None,
            successor_metadata: None,
            replace_policy: None,
            rotation_mode: None,
            authority_epoch: None,
            managed_agent_role: None,
            initial_task_preview: None,
            detail: None,
            runtime_generation: None,
        },
    };
    let cells = thread_items_to_transcript_cells(
        None,
        &AbsolutePathBuf::try_from("/tmp").expect("absolute cwd"),
        [item],
        RawReasoningVisibility::Hidden,
        None,
    );
    assert!(cells.is_empty());
}
