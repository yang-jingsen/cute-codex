use super::*;
use codex_protocol::presentation::Presentation;
use codex_protocol::presentation::PresentationAppended;
use codex_protocol::presentation::PresentationFormat;
fn notice(id: &str) -> PresentationAppended {
    let mut record = PresentationAppended {
        version: 1,
        owner_id: "owner".into(),
        origin_thread_id: "origin".into(),
        presentation: Presentation {
            id: id.into(),
            source: codex_protocol::external_input::Source {
                kind: codex_protocol::external_input::SourceKind::Service,
                id: "build\u{1b}[31m".into(),
            },
            title: "Report 世界".into(),
            body: "**safe bold**\n第二行\u{7}".into(),
            format: PresentationFormat::Markdown,
            references: vec![],
        },
        semantic_sha256: String::new(),
        receipt_id: String::new(),
    };
    record.semantic_sha256 = record.semantic_digest();
    record.receipt_id = record.receipt_digest();
    record
}
#[tokio::test]
async fn durable_notice_live_replay_identity_and_safe_render() {
    let (mut chat, mut rx, _ops) = make_chatwidget_manual(None).await;
    let id = ThreadId::new();
    chat.thread_id = Some(id);
    let _ = drain_insert_history(&mut rx);
    let record = notice("one");
    let notification = ServerNotification::ThreadPresentationAppended(
        codex_app_server_protocol::PresentationAppendedNotification {
            thread_id: id.to_string(),
            position: 1,
            item: record.clone(),
        },
    );
    chat.handle_server_notification(notification.clone(), None);
    let mut cells = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let AppEvent::InsertHistoryCell(cell) = event {
            cells.push(cell);
        }
    }
    assert_eq!(cells.len(), 1);
    let display = cells[0]
        .display_lines(24)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!display.contains('\u{1b}'));
    assert!(!display.contains('\u{7}'));
    assert!(display.contains("Notice"));
    insta::assert_snapshot!(display);
    let raw = cells[0]
        .raw_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(raw.contains("**safe bold**"));
    assert!(!raw.contains('•'));
    chat.handle_server_notification(notification, None);
    chat.replay_presentation_timeline(
        vec![],
        vec![
            codex_app_server_protocol::ThreadTimelineEntry::Presentation {
                position: 77,
                item: record.clone(),
            },
        ],
        ReplayKind::ThreadSnapshot,
    );
    assert!(drain_insert_history(&mut rx).is_empty());
    chat.on_presentation(notice("two"));
    assert_eq!(drain_insert_history(&mut rx).len(), 1);
    let mut conflict = record;
    conflict.presentation.body = "changed".into();
    conflict.semantic_sha256 = conflict.semantic_digest();
    conflict.receipt_id = conflict.receipt_digest();
    chat.on_presentation(conflict);
    assert!(drain_insert_history(&mut rx).iter().any(|cell| {
        cell.iter()
            .any(|line| line.to_string().contains("Conflicting"))
    }));
}
#[tokio::test]
async fn durable_notice_idle_timeline_order_and_no_op() {
    let (mut chat, mut rx, mut ops) = make_chatwidget_manual(None).await;
    chat.thread_id = Some(ThreadId::new());
    let _ = drain_insert_history(&mut rx);
    chat.transcript.presentation_timeline = Some(vec![
        codex_app_server_protocol::ThreadTimelineEntry::Presentation {
            position: 1,
            item: notice("first"),
        },
        codex_app_server_protocol::ThreadTimelineEntry::Presentation {
            position: 2,
            item: notice("second"),
        },
    ]);
    chat.replay_thread_turns(vec![], ReplayKind::ResumeInitialMessages);
    assert_eq!(drain_insert_history(&mut rx).len(), 2);
    assert!(ops.try_recv().is_err());
    assert!(chat.turn_lifecycle.last_turn_id.is_none());
}

#[test]
fn durable_notice_plain_linked_supplement_keeps_explicit_identity() {
    let mut record = notice("linked");
    record.presentation.format = PresentationFormat::PlainText;
    record.presentation.references = vec![codex_protocol::presentation::PresentationReference {
        kind: codex_protocol::presentation::PresentationReferenceKind::McpInvocation,
        id: "call-17".into(),
    }];
    record.semantic_sha256 = record.semantic_digest();
    record.receipt_id = record.receipt_digest();
    let cell = history_cell::PresentationHistoryCell::new(record).unwrap();
    let display = cell
        .display_lines(24)
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(display.contains("**safe bold**"));
    assert!(display.contains("call-17"));
    insta::assert_snapshot!(display);
}
