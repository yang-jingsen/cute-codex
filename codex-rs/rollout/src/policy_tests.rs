use super::should_persist_event_msg;
use codex_protocol::ThreadId;
use codex_protocol::items::CUTEX_UI_ONLY_ACTIVITY_LANE_ID;
use codex_protocol::items::CutexUiActivityCheckpoint;
use codex_protocol::items::CutexUiActivityDelivery;
use codex_protocol::items::CutexUiActivityDeliveryClass;
use codex_protocol::items::CutexUiActivityDeliverySchema;
use codex_protocol::items::ManagedAgentActivityItem;
use codex_protocol::items::ManagedAgentActivityStatus;
use codex_protocol::items::ManagedAgentOperation;
use codex_protocol::items::TurnItem;
use codex_protocol::protocol::CutexUiActivityEvent;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ItemCompletedEvent;
use codex_protocol::protocol::ItemStartedEvent;
use codex_protocol::protocol::ThreadHistoryMode;

fn activity(status: ManagedAgentActivityStatus) -> TurnItem {
    TurnItem::ManagedAgentActivity(ManagedAgentActivityItem {
        id: "action-1".to_string(),
        event_id: "event-1".to_string(),
        sequence: 1,
        occurred_at_ms: 1_725_000_123_456,
        project_id: None,
        operation: ManagedAgentOperation::Create,
        status,
        action_id: None,
        phase_event_id: None,
        phase: None,
        managed_agent_id: "cutex.worker-1".to_string(),
        managed_agent_name: Some("Worker".to_string()),
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
        managed_agent_role: Some("worker".to_string()),
        initial_task_preview: None,
        detail: None,
        runtime_generation: Some(1),
    })
}

#[test]
fn reserved_cutex_lane_is_transient_while_explicit_turn_activity_remains_durable() {
    let thread_id = ThreadId::new();
    let reserved_started = EventMsg::ItemStarted(ItemStartedEvent {
        thread_id,
        turn_id: CUTEX_UI_ONLY_ACTIVITY_LANE_ID.to_string(),
        item: activity(ManagedAgentActivityStatus::InProgress),
        started_at_ms: 1_725_000_123_456,
    });
    let reserved_completed = EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id,
        turn_id: CUTEX_UI_ONLY_ACTIVITY_LANE_ID.to_string(),
        item: activity(ManagedAgentActivityStatus::Completed),
        started_at_ms: None,
        completed_at_ms: 1_725_000_123_456,
    });
    let explicit_completed = EventMsg::ItemCompleted(ItemCompletedEvent {
        thread_id,
        turn_id: "authoritative-turn-1".to_string(),
        item: activity(ManagedAgentActivityStatus::Completed),
        started_at_ms: None,
        completed_at_ms: 1_725_000_123_456,
    });
    let checkpoint = CutexUiActivityCheckpoint {
        stream_id: "management-v2".into(),
        cursor: "cursor-1".into(),
        sequence: 1,
    };
    let delivery = EventMsg::CutexUiActivity(CutexUiActivityEvent {
        thread_id,
        delivery: CutexUiActivityDelivery {
            schema: CutexUiActivityDeliverySchema::V1,
            class: CutexUiActivityDeliveryClass::CatchUp,
            recovered: true,
            batch_id: "catch_up:management-v2:1:1".into(),
            batch_index: 0,
            batch_size: 1,
            source_checkpoint: checkpoint.clone(),
            batch_checkpoint: checkpoint,
        },
        item: activity(ManagedAgentActivityStatus::Completed),
    });

    for history_mode in [ThreadHistoryMode::Legacy, ThreadHistoryMode::Paginated] {
        assert!(!should_persist_event_msg(&reserved_started, history_mode));
        assert!(!should_persist_event_msg(&reserved_completed, history_mode));
        assert!(!should_persist_event_msg(&delivery, history_mode));
        assert!(should_persist_event_msg(&explicit_completed, history_mode));
    }
}
