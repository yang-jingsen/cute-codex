use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;

fn page(bytes: &[u8]) -> Value {
    json!({"jobId":"j", "stream":"stdout", "fromOffset":7,
        "nextOffset":7 + bytes.len(), "bytesHex":bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "gap":false,"truncated":false})
}
fn parsed(value: &Value) -> Option<OutputPage> {
    OutputPage::parse(value, &json!({"jobId":"j","stream":"stdout"}))
}
#[test]
fn output_page_plain_unicode_and_source_flags() {
    let mut value = page("**plain** https://example.invalid\n🦀e\u{301}\u{1b}[31m".as_bytes());
    value["gap"] = json!(true);
    value["truncated"] = json!(true);
    let lines = parsed(&value).unwrap().lines(28);
    insta::assert_snapshot!(
        lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(!lines.iter().any(|line| line.to_string().contains('\u{1b}')));
}
#[test]
fn output_page_validates_exact_bytes_and_offsets() {
    for (key, replacement) in [
        ("bytesHex", json!("0x")),
        ("bytesHex", json!("f")),
        ("bytesHex", json!("é")),
        ("nextOffset", json!(99)),
        ("gap", json!(null)),
        ("stream", json!("other")),
    ] {
        let mut value = page(b"abc");
        value[key] = replacement;
        assert!(parsed(&value).is_none(), "{key}");
    }
    assert_eq!(parsed(&page(b"abc")).unwrap().bytes, b"abc");
}
#[test]
fn output_page_binary_empty_and_preview_window() {
    let binary = parsed(&page(&[0xff, 0xfe])).unwrap().lines(80);
    assert!(
        binary
            .iter()
            .any(|line| line.to_string().contains("Binary output · 2 B"))
    );
    let empty = parsed(&page(b"")).unwrap().lines(80);
    assert!(empty[0].to_string().contains("empty returned page"));
    for text in [
        "a\n".repeat(20),
        "🦀".repeat(1024),
        format!("{}e\u{301}", "x".repeat(2047)),
    ] {
        let lines = parsed(&page(text.as_bytes())).unwrap().lines(80);
        assert!(
            lines
                .iter()
                .any(|line| line.to_string().contains("Preview shortened"))
        );
        assert!(lines.len() <= 8);
    }
}
