use super::ThreadHistoryBuilder;
use codex_history::RolloutItem;
use codex_protocol::protocol::EventMsg;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn legacy_inter_agent_history_preserves_order_identity_and_data() {
    let mut builder = ThreadHistoryBuilder::new();
    let start: EventMsg = serde_json::from_value(json!({"type":"turn_started","turn_id":"old-turn","model_context_window":null,"collaboration_mode_kind":"default"})).unwrap();
    builder.handle_event(&start);
    for id in ["first", "second", "second"] {
        let event: EventMsg = serde_json::from_value(json!({"type":"item_completed","thread_id":"00000000-0000-0000-0000-000000000001","turn_id":"old-turn","item":{
            "type":"InterAgentMessage","id":id,"author":"/root/worker","recipient":"/root","otherRecipients":["/root/peer"],"content":"Same content 世界","deliveryMode":"interrupt"}})).unwrap();
        builder.handle_rollout_item(&RolloutItem::EventMsg(event));
    }
    let turns = builder.finish_checked().unwrap();
    assert_eq!(turns.len(), 1);
    assert_eq!(turns[0].id, "old-turn");
    assert_eq!(
        turns[0].items.iter().map(|i| i.id()).collect::<Vec<_>>(),
        vec!["first", "second"]
    );
    for item in &turns[0].items {
        let v = serde_json::to_value(item).unwrap();
        assert_eq!(v["type"], "legacyInterAgentMessage");
        assert_eq!(v["content"], "Same content 世界");
        assert_eq!(v["otherRecipients"], json!(["/root/peer"]));
        assert_eq!(v["deliveryMode"], "interrupt");
    }
}
