use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

#[test]
fn legacy_inter_agent_safe_display_and_full_details() {
    let value = json!({"id":"old-item","author":"/root/worker","recipient":"/root","otherRecipients":["/root/peer"],"content":"Historical 世界\nplain **text**\u{1b}[31m","deliveryMode":"interrupt"});
    let cell = LegacyInterAgentHistoryCell(serde_json::from_value(value.clone()).unwrap());
    let lines = cell.display_lines(28);
    let text = lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(text);
    assert!(!text.contains('\u{1b}'));
    assert_eq!(serde_json::to_value(&cell.0).unwrap(), value);
    let raw = cell
        .raw_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(raw.contains("old-item") && raw.contains("/root/peer") && raw.contains("interrupt"));
    assert!(!raw.contains('\u{1b}'));
}
