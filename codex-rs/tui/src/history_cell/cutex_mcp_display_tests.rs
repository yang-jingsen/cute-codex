use super::*;
use crate::history_cell::HistoryCell;
use crate::history_cell::mcp::McpInvocation;
use crate::history_cell::mcp::McpToolCallCell;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::time::Duration;

fn cell(server: &str, tool: &str, args: Value) -> McpToolCallCell {
    McpToolCallCell::new(
        "nested-exec-id".into(),
        McpInvocation {
            server: server.into(),
            tool: tool.into(),
            arguments: Some(args),
        },
        false,
    )
}
fn rendered(lines: Vec<ratatui::text::Line<'static>>) -> String {
    lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn compact_lifecycle_and_full_invocation() {
    let args = json!({"actionId":"job-action-🦀", "argv":["echo","full argument retained"],"cwd":"/private"});
    let mut call = cell("cutex_job", "submit", args);
    let running = rendered(call.display_lines(40));
    call.complete(
        Duration::ZERO,
        Ok(codex_protocol::mcp::CallToolResult {
            content: vec![
                json!({"type":"text","text":"accepted_pending; execution not confirmed"}),
            ],
            structured_content: None,
            is_error: Some(false),
            meta: None,
        }),
    );
    let before = (call.transcript_lines(120), call.raw_lines());
    let normal = rendered(call.display_lines(40));
    let narrow = rendered(call.display_lines(20));
    assert_eq!((call.transcript_lines(120), call.raw_lines()), before);
    assert!(rendered(call.raw_lines()).contains("full argument retained"));
    insta::assert_snapshot!(format!(
        "running:\n{running}\nnormal:\n{normal}\nnarrow:\n{narrow}\ntranscript:\n{}\nraw:\n{}",
        rendered(before.0),
        rendered(before.1)
    ));
    call.mark_failed();
    insta::assert_snapshot!("cutex_error", rendered(call.display_lines(40)));
}
#[test]
fn exact_names_shapes_and_unknown_fallback() {
    for (server, tool, args) in [
        ("other", "query", json!({"jobId":"x"})),
        ("cutex_job_extra", "query", json!({"jobId":"x"})),
        ("cutex_job", "future", json!({"jobId":"x"})),
        ("cutex_job", "query", json!({"jobId":"x","version":2})),
        ("cutex_job", "query", json!({"jobId":3})),
        (
            "cutex_job",
            "submit",
            json!({"actionId":"x","argv":"echo","cwd":"/"}),
        ),
        (
            "cutex_job",
            "read_output",
            json!({"jobId":"x","stream":"all"}),
        ),
        ("node_repl", "js", json!({"title":"Existing Node REPL"})),
    ] {
        assert_eq!(
            title(&McpInvocation {
                server: server.into(),
                tool: tool.into(),
                arguments: Some(args.clone())
            }),
            None
        );
        let call = cell(server, tool, args);
        if server != "node_repl" {
            assert_eq!(call.display_lines(120), call.transcript_lines(120));
        }
    }
}
#[test]
fn known_headers_and_sanitized_layout() {
    let entries = [
        ("cutex_job", "query", json!({"jobId":"x"})),
        (
            "cutex_job",
            "read_output",
            json!({"jobId":"x","stream":"stdout"}),
        ),
        (
            "cutex_job",
            "cancel",
            json!({"jobId":"x","expectedRevision":1}),
        ),
        (
            "cutex",
            "send",
            json!({"to":"worker\u{1b}[31m\n二", "message":"body", "external_message_id":"m1","delivery_mode":"soon"}),
        ),
        (
            "cutex",
            "cutex_agent_management",
            json!({"operation":"create","action_id":"a","start_mode":"bootstrap_only","spec":{"name":"worker","cwd":"/private","profile":"p","runtime_backend":"native","model":"test","reasoning":"low","permissions":"read-only","approval_policy":"on-request","sandbox_mode":"read-only","groups":[]}}),
        ),
        (
            "cutex",
            "cutex_agent_management",
            json!({"operation":"restart","action_id":"a","cutex_session_id":"worker"}),
        ),
        (
            "cutex",
            "cutex_task_service_terminal",
            json!({"operation":"request_changes","action_id":"a","assignment_id":"t","decision_reference":"fix"}),
        ),
        (
            "cutex",
            "cutex_task_service_director",
            json!({"operation":"assign","action_id":"a","project_id":"p","task_id":"t","task_revision":1,"assignment_id":"as","assignee_cutex_session_id":"w","summary":"work"}),
        ),
    ];
    let all = entries
        .into_iter()
        .map(|(s, t, a)| rendered(cell(s, t, a).display_lines(60)))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!all.contains('\u{1b}'));
    insta::assert_snapshot!(all);
    insta::assert_snapshot!(
        "no_bullet",
        rendered(crate::history_cell::event_presentation::render_event(
            Some("Notice · workspace"),
            &["Plain untrusted body\u{1b}[31m\nsecond line".into()],
            None,
            24,
        ))
    );
}

#[test]
fn error_result_cannot_become_success_from_arguments() {
    let mut call = cell("cutex_job", "query", json!({"jobId":"success"}));
    call.complete(
        Duration::ZERO,
        Ok(codex_protocol::mcp::CallToolResult {
            content: vec![json!({"type":"text","text":"request_uncertain — receipt retained"})],
            structured_content: None,
            is_error: Some(true),
            meta: None,
        }),
    );
    let lines = call.display_lines(80);
    assert_eq!(lines[0].spans[0].style.fg, Some(ratatui::style::Color::Red));
    insta::assert_snapshot!(rendered(lines));
}

#[test]
fn only_known_receipt_marks_uncertainty() {
    assert!(outcome_uncertain(
        r#"{"schema":"cutex/agent-management/v1","outcome":{"status":"no_write","code":"response_uncertain"}}"#
    ));
    assert!(!outcome_uncertain(
        r#"{"schema":"cutex/agent-management/v2","outcome":{"code":"response_uncertain"}}"#
    ));
    let mut call = cell(
        "cutex",
        "cutex_agent_management",
        json!({"operation":"restart","action_id":"a","cutex_session_id":"worker"}),
    );
    call.complete(Duration::ZERO, Ok(codex_protocol::mcp::CallToolResult {
        content: vec![json!({"type":"text","text":r#"{"schema":"cutex/agent-management/v1","outcome":{"status":"no_write","code":"response_uncertain"}}"#})],
        structured_content: None, is_error: Some(true), meta: None,
    }));
    assert!(rendered(call.display_lines(80)).starts_with("• Outcome uncertain"));
}

#[test]
fn future_result_schema_uses_upstream() {
    let mut call = cell("cutex_job", "query", json!({"jobId":"j"}));
    call.complete(
        Duration::ZERO,
        Ok(codex_protocol::mcp::CallToolResult {
            content: vec![
                json!({"type":"text","text":r#"{"schema":"future/v2","value":"retained"}"#}),
            ],
            structured_content: None,
            is_error: Some(false),
            meta: None,
        }),
    );
    assert_eq!(call.display_lines(80), call.transcript_lines(80));
}
