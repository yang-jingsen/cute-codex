use super::*;
use pretty_assertions::assert_eq;

#[test]
fn legacy_inter_agent_api_read_does_not_apply_current_write_filter() {
    let event: EventMsg = serde_json::from_value(serde_json::json!({
        "type":"item_completed", "thread_id":"00000000-0000-0000-0000-000000000001",
        "turn_id":"historical-turn", "item": {"type":"InterAgentMessage", "id":"original",
        "author":"/root/worker", "recipient":"/root", "otherRecipients":[],
        "content":"historical 世界", "deliveryMode":"interrupt"}
    }))
    .unwrap();
    let item = RolloutItem::EventMsg(event);
    assert!(!is_persisted_rollout_item(
        &item,
        codex_protocol::protocol::ThreadHistoryMode::Legacy
    ));
    let start: EventMsg = serde_json::from_value(serde_json::json!({
        "type":"turn_started", "turn_id":"historical-turn",
        "model_context_window":null, "collaboration_mode_kind":"default"
    }))
    .unwrap();
    let turns = build_legacy_api_turns_from_rollout_items(&[RolloutItem::EventMsg(start), item]);
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].id, "historical-turn");
    assert_eq!(turns[0].items[0].id(), "original");
}
