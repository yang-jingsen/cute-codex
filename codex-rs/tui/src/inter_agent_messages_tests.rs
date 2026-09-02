use super::*;
use crate::history_cell::HistoryCell;
use pretty_assertions::assert_eq;

#[test]
fn canonical_inter_agent_message_history_card_snapshot() {
    let other_recipients = vec!["/root/observer".to_string()];
    let presentation = InterAgentMessagePresentation::new(
        "message-1",
        "/root/sender",
        "/root/receiver",
        &other_recipients,
        "First line\nSecond line",
        InterAgentDeliveryMode::AfterTurn,
    );
    let rendered = presentation
        .history_cell()
        .display_lines(/*width*/ 80)
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    insta::assert_snapshot!(rendered);
}

#[test]
fn compact_summary_normalizes_and_bounds_content() {
    let content = format!("first\n\nsecond {}", "x".repeat(1_000));
    let presentation = InterAgentMessagePresentation::new(
        "message-1",
        "/root/sender",
        "/root/receiver",
        &[],
        &content,
        InterAgentDeliveryMode::AfterTurn,
    );
    let summary = presentation.compact_summary();

    assert!(summary.starts_with("Agent message [after-turn] from /root/sender: first second "));
    assert_eq!(summary.chars().count(), COMPACT_SUMMARY_GRAPHEMES);
}

fn participant(name: &str) -> CutexParticipantPresentation {
    CutexParticipantPresentation {
        display_name: Some(name.to_string()),
        cutex_session_id: Some("cutex.worker.1".to_string()),
        profile: Some("worker".to_string()),
        model: Some("gpt-5".to_string()),
        reasoning: Some("high".to_string()),
        role: Some("Worker".to_string()),
        runtime_backend: Some("codex".to_string()),
    }
}

#[test]
fn tagged_cutex_message_uses_safe_default_presentation() {
    let author = participant("Agent 已取餐");
    let recipient = participant("Director");
    let presentation = InterAgentMessagePresentation::new(
        "message-1",
        "/root/sender",
        "/root/receiver",
        &[],
        "First line\nSecond line",
        InterAgentDeliveryMode::Soon,
    )
    .with_cutex_metadata(Some(&author), Some(&recipient), None);
    let rendered = presentation
        .history_cell()
        .display_lines(80)
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("Cutex message [soon] from Agent 已取餐"));
    assert!(rendered.contains("session=cutex.worker.1"));
    assert!(rendered.contains("profile=worker"));
    assert!(rendered.contains("  First line\n  Second line"));
    assert!(!rendered.contains("Agent message"));
    insta::assert_snapshot!(rendered, @r###"
    Cutex message [soon] from Agent 已取餐
    id=message-1 | from=/root/sender | to=/root/receiver | session=cutex.worker.1 | profile=worker | model=gpt-5 | reasoning=high | role=Worker | runtime=codex
      First line
      Second line
    "###);
}

#[test]
fn tagged_cutex_message_supports_bounded_multiline_template_style_and_visibility() {
    let author = participant("Worker");
    let recipient = participant("Director");
    let settings = TuiCutexInboundMessageSettings {
        label: Some("{author} → {recipient}\n{role} · {model} · {mode}".to_string()),
        style: TuiCutexTextStyle {
            foreground: Some(TuiCutexForegroundColor::Magenta),
            bold: Some(false),
            dim: Some(true),
            italic: Some(true),
        },
        show_metadata: false,
        show_ids: false,
        content_indent: 6,
    };
    let presentation = InterAgentMessagePresentation::new(
        "message-1",
        "/root/sender",
        "/root/receiver",
        &[],
        "hello",
        InterAgentDeliveryMode::AfterTurn,
    )
    .with_cutex_metadata(Some(&author), Some(&recipient), Some(&settings));
    let lines = presentation.history_cell().display_lines(80);

    assert_eq!(lines[0].to_string(), "Worker → Director");
    assert_eq!(lines[1].to_string(), "Worker · gpt-5 · after-turn");
    assert_eq!(lines[2].to_string(), "      hello");
    assert_eq!(lines[0].spans[0].style.fg, Some(Color::Magenta));
    assert!(lines[0].spans[0].style.add_modifier.contains(Modifier::DIM));
    assert!(
        lines[0].spans[0]
            .style
            .add_modifier
            .contains(Modifier::ITALIC)
    );
}

#[test]
fn malformed_cutex_templates_and_excess_indent_fall_back_and_clamp() {
    let author = participant("Worker");
    for malformed in [
        "{unknown}",
        "{author",
        "one\ntwo\nthree\nfour\nfive",
        "bad\ttemplate",
    ] {
        let settings = TuiCutexInboundMessageSettings {
            label: Some(malformed.to_string()),
            content_indent: u8::MAX,
            ..TuiCutexInboundMessageSettings::default()
        };
        let presentation = InterAgentMessagePresentation::new(
            "message-1",
            "/root/sender",
            "/root/receiver",
            &[],
            "hello",
            InterAgentDeliveryMode::Passive,
        )
        .with_cutex_metadata(Some(&author), None, Some(&settings));
        let rendered = presentation
            .history_cell()
            .display_lines(80)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.starts_with("Cutex message [passive] from Worker"));
        assert!(rendered.contains("        hello"));
        assert!(!rendered.contains("unknown"));
    }
}
