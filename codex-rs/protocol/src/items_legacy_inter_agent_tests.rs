use crate::items::TurnItem;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn legacy_inter_agent_preserves_fields_without_legacy_fanout() {
    for mode in ["after_turn", "soon", "passive", "interrupt"] {
        let value = json!({"type":"InterAgentMessage","id":"old-id","author":"/root/worker",
            "recipient":"/root","otherRecipients":["/root/peer"],"content":"Historical 世界\ntext", "deliveryMode":mode});
        let item: TurnItem = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(&item).unwrap(), value);
        assert_eq!(item.id(), "old-id");
        assert!(
            item.as_legacy_events(/*show_raw_agent_reasoning*/ false)
                .is_empty()
        );
        let mut unknown = value;
        unknown["authority"] = json!("system");
        assert!(serde_json::from_value::<TurnItem>(unknown).is_err());
    }
}
