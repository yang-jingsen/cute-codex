use super::*;
use serde_json::json;

#[test]
fn structured_input_display_transcript_and_unknown_fallback() {
    let output = FunctionCallOutputBody::Text(json!({"source":{"kind":"service","id":"fixture"},"type":"completion","text":"Original committed model text."}).to_string());
    let mut view = codex_protocol::external_input_view::View {
        schema: "cutex.job-completion.v1".into(),
        data: json!({"jobId":"job_1234567890abcdefabcd","jobRevision":1,"terminalStatus":"exited","actionId":"build\u{1b}[31m世界","exitCode":0}),
    };
    let cell = ExternalInputHistoryCell::parse(
        "id",
        "external_event",
        Some("external"),
        &output,
        Some(&view),
    )
    .unwrap();
    let normal = cell
        .display_lines(28)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(normal);
    assert!(!normal.contains('\u{1b}'));
    let raw = cell
        .raw_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(raw.contains("Original committed model text."));
    assert!(raw.contains("Display facts (not model input)"));
    assert!(raw.contains("job_1234567890abcdefabcd"));
    view.schema = "unknown.v1".into();
    let fallback = ExternalInputHistoryCell::parse(
        "id",
        "external_event",
        Some("external"),
        &output,
        Some(&view),
    )
    .unwrap();
    assert!(
        fallback
            .display_lines(120)
            .iter()
            .any(|line| line.to_string().contains("Original committed model text."))
    );
    assert_eq!(output.to_text().unwrap(), json!({"source":{"kind":"service","id":"fixture"},"type":"completion","text":"Original committed model text."}).to_string());
}

#[test]
fn structured_input_short_collision_expands_without_action_lookup() {
    let mut labels = super::super::JobLabels::default();
    let first = "job_12345678aaaaaaabcd";
    let second = "job_12345678bbbbbbabcd";
    let short = labels.observe_display_id(first);
    assert_ne!(short, first);
    assert_eq!(labels.observe_display_id(second), second);
    assert_eq!(labels.label(first), first);
}

#[test]
fn job_metadata_dim_wrap_and_raw_facts() {
    use ratatui::style::Modifier;
    let output = FunctionCallOutputBody::Text(json!({"source":{"kind":"service","id":"fixture"},"type":"completion","text":"Frozen model text"}).to_string());
    let mut view = codex_protocol::external_input_view::View {
        schema: "cutex.job-completion.v1".into(),
        data: json!({"jobId":"job_example","jobRevision":1,"terminalStatus":"exited","exitCode":0,
            "execution":{"basis":"runner_release_to_wait_v1","observedRunDurationMillis":1048,"startObservedAtEpochMillis":1000,"exitObservedAtEpochMillis":2048},
            "stdout":{"observedBytes":19,"retainedBytes":19,"truncated":false},
            "stderr":{"observedBytes":0,"retainedBytes":0,"truncated":false}}),
    };
    let mut snapshots = Vec::new();
    for exceptional in [false, true] {
        if exceptional {
            view.data["stdout"] = json!({"observedBytes":100,"retainedBytes":19,"truncated":true});
            view.data.as_object_mut().unwrap().remove("exitCode");
            view.data.as_object_mut().unwrap().remove("execution");
        }
        let original = view.clone();
        let cell = ExternalInputHistoryCell::parse(
            "id",
            "external_event",
            Some("external"),
            &output,
            Some(&view),
        )
        .unwrap();
        for width in [100, 28] {
            let lines = cell.display_lines(width);
            let metadata: Vec<_> = lines
                .iter()
                .filter(|line| line.style.add_modifier.contains(Modifier::DIM))
                .collect();
            assert!(!metadata.is_empty());
            assert!(
                lines[0]
                    .spans
                    .iter()
                    .any(|span| span.style.add_modifier.contains(Modifier::BOLD))
            );
            snapshots.push(
                lines
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        assert_eq!(view, original);
        let raw = cell
            .raw_lines()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        assert!(raw.contains(&view.canonical_json()));
        assert!(raw.contains("Frozen model text"));
        assert_eq!(cell.raw_lines(), cell.transcript_lines(u16::MAX));
    }
    insta::assert_snapshot!(snapshots.join("\n---\n"));
}
