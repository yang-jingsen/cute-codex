use super::*;
use crate::history_cell::HistoryCell;
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
fn finish(call: &mut McpToolCallCell, value: Value, failed: bool) {
    call.complete(
        Duration::ZERO,
        Ok(codex_protocol::mcp::CallToolResult {
            content: vec![json!({"type":"text","text":value.to_string()})],
            structured_content: Some(value),
            is_error: Some(failed),
            meta: None,
        }),
    );
}
fn render(lines: Vec<ratatui::text::Line<'static>>) -> String {
    lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
fn job(state: &str) -> Value {
    json!({"schema":"cutex/job-service-core/v1","jobId":"job-1","revision":1,"state":state,"request":{"actionId":"action-🦀"}})
}
#[test]
fn job_lifecycle_pink_and_full_invocation() {
    let mut call = cell(
        "cutex_job",
        "submit",
        json!({"actionId":"action-🦀","argv":["echo","full argument retained"],"cwd":"/private"}),
    );
    let running = render(call.display_lines(40));
    finish(
        &mut call,
        json!({"status":"committed","job":job("launch_pending"),"deduplicated":false}),
        false,
    );
    let detailed = (call.transcript_lines(120), call.raw_lines());
    let display = call.display_lines(40);
    assert_eq!(
        display[0].spans[0].style.fg,
        Some(crate::terminal_palette::rgb_color((0xF6, 0xA3, 0xC8)))
    );
    assert!(render(detailed.1.clone()).contains("full argument retained"));
    let narrow = render(call.display_lines(20));
    assert_eq!((call.transcript_lines(120), call.raw_lines()), detailed);
    insta::assert_snapshot!(format!(
        "running:\n{running}\nnormal:\n{}\nnarrow:\n{narrow}\nraw:\n{}",
        render(display),
        render(detailed.1)
    ));
}
#[test]
fn known_states_never_upgrade_transport_success() {
    let mut call = cell("cutex_job", "query", json!({"jobId":"job-1"}));
    finish(&mut call, json!({"unknown":"success"}), false);
    assert_eq!(call.display_lines(80), call.transcript_lines(80));
    finish(&mut call, job("launch_unknown"), false);
    assert!(render(call.display_lines(80)).contains("launch_unknown"));
    assert_ne!(
        call.display_lines(80)[0].spans[0].style.fg,
        Some(crate::terminal_palette::rgb_color((0xF6, 0xA3, 0xC8)))
    );
    finish(&mut call, job("failed"), false);
    assert_eq!(
        call.display_lines(80)[0].spans[0].style.fg,
        Some(ratatui::style::Color::Red)
    );
    let mut future = job("running");
    future["schema"] = json!("cutex/job-service-core/v2");
    finish(&mut call, future, false);
    assert_eq!(call.display_lines(80), call.transcript_lines(80));
    let mut wrong = job("running");
    wrong["jobId"] = json!("other");
    finish(&mut call, wrong, false);
    assert_eq!(call.display_lines(80), call.transcript_lines(80));
}
#[test]
fn registry_actions_and_shape_fallback() {
    let spec = json!({"name":"worker","cwd":"/private","profile":"p","runtime_backend":"native","model":"test","reasoning":"low","permissions":"read-only","approval_policy":"on-request","sandbox_mode":"read-only","groups":[]});
    let mut headers = Vec::new();
    for op in [
        "create",
        "query_managed",
        "online",
        "offline",
        "restart",
        "close",
        "replace",
        "director_rotate",
    ] {
        let mut args = json!({"operation":op,"action_id":"a"});
        match op {
            "create" => {
                args["spec"] = spec.clone();
                args["start_mode"] = json!("bootstrap_only");
            }
            "query_managed" => {}
            "replace" => {
                args["predecessor_cutex_session_id"] = json!("old");
                args["policy"] = json!("close_before_create");
                args["successor"] = spec.clone();
                args["start_mode"] = json!("bootstrap_only");
            }
            "director_rotate" => {
                args["expected_predecessor_cutex_session"] = json!("old");
                args["expected_authority_epoch"] = json!(1);
                args["mode"] = json!("retain_predecessor_bootstrap_only");
                args["successor"] = spec.clone();
            }
            _ => args["cutex_session_id"] = json!("worker"),
        }
        headers.push(render(
            cell("cutex", "cutex_agent_management", args.clone()).display_lines(80),
        ));
        let mut completed = cell("cutex", "cutex_agent_management", args.clone());
        let kind = match op {
            "create" => "created",
            "query_managed" => "query_managed",
            "replace" => "replaced",
            "director_rotate" => "director_rotated",
            _ => "lifecycle",
        };
        finish(
            &mut completed,
            json!({"schema":"cutex/agent-management/v1","action_id":"a","outcome":{"status":"complete","receipt":{"schema":"cutex/agent-management-receipt/v1","action_id":"a","operation":op,"result":{"kind":kind}}}}),
            false,
        );
        assert_ne!(completed.display_lines(80), completed.transcript_lines(80));
        headers.push(
            render(completed.display_lines(80))
                .lines()
                .next()
                .unwrap()
                .into(),
        );
        args["unreviewed_field"] = json!(true);
        let fallback = cell("cutex", "cutex_agent_management", args);
        assert_eq!(fallback.display_lines(80), fallback.transcript_lines(80));
    }
    for op in [
        "start",
        "report_status",
        "block",
        "resume",
        "submit",
        "decline",
        "abort_attempt",
    ] {
        let mut args = json!({"operation":op,"action_id":"a","assignment_id":"assignment"});
        if matches!(op, "report_status" | "block") {
            args["summary"] = json!("status");
        }
        if op == "submit" {
            args["result_sha256"] = json!("a".repeat(64));
            args["result_reference"] = json!("result");
        }
        let mut call = cell("cutex", "cutex_task_service", args);
        headers.push(render(call.display_lines(80)));
        finish(
            &mut call,
            json!({"schema":"cutex/task-service-tool-receipt/v1","status":"committed"}),
            false,
        );
        headers.push(
            render(call.display_lines(80))
                .lines()
                .next()
                .unwrap()
                .to_owned(),
        );
    }
    insta::assert_snapshot!(headers.join("\n"));
    for (server, tool, args) in [
        ("other", "query", json!({"jobId":"j"})),
        ("cutex_job_extra", "query", json!({"jobId":"j"})),
        ("cutex_job", "query", json!({"jobId":3})),
        (
            "cutex_job",
            "read_output",
            json!({"jobId":"j","stream":"all"}),
        ),
        ("cutex", "cutex_agent_list", json!({"all_hosts":true})),
        (
            "cutex",
            "cutex_task_service",
            json!({"operation":"invented","action_id":"a","assignment_id":"x"}),
        ),
    ] {
        let call = cell(server, tool, args);
        assert_eq!(call.display_lines(80), call.transcript_lines(80));
    }
}
#[test]
fn messages_receipts_and_terminal_controls() {
    let args = json!({"to":"worker\u{1b}[31m\n二","message":"body","external_message_id":"m","delivery_mode":"soon"});
    let mut call = cell("cutex", "send", args.clone());
    finish(
        &mut call,
        json!({"ok":true,"to":args["to"],"id":"m","queued":true}),
        false,
    );
    let display = render(call.display_lines(30));
    assert!(!display.contains('\u{1b}'));
    assert!(display.contains("Queued message"));
    insta::assert_snapshot!(display);
    let mut management = cell(
        "cutex",
        "cutex_agent_management",
        json!({"operation":"restart","action_id":"a","cutex_session_id":"w"}),
    );
    finish(
        &mut management,
        json!({"schema":"cutex/agent-management/v1","action_id":"a","outcome":{"status":"no_write","code":"response_uncertain"}}),
        true,
    );
    assert!(render(management.display_lines(80)).starts_with("• Outcome uncertain"));
    insta::assert_snapshot!(
        "no_bullet",
        render(crate::history_cell::event_presentation::render_event(
            Some("Notice · workspace"),
            &["Plain body\u{1b}[31m\nsecond line".into()],
            None,
            24
        ))
    );
}

#[test]
fn director_terminal_and_observation_receipts() {
    let mut headers = Vec::new();
    for tool in ["cutex_task_service_director", "cutex_task_service_terminal"] {
        let operations: &[&str] = if tool.ends_with("terminal") {
            &["accept_result", "request_changes", "fail_result"]
        } else {
            &[
                "create_revision",
                "assign",
                "create_and_assign",
                "query",
                "accept_result",
                "request_changes",
                "fail_result",
                "cancel",
            ]
        };
        for op in operations {
            let mut args = json!({"operation":op,"action_id":"action"});
            match *op {
                "create_revision" | "create_and_assign" => {
                    args["project_id"] = json!("p");
                    args["workflow_id"] = json!("w");
                    args["task_id"] = json!("t");
                    args["task_revision"] = json!(1);
                    args["opaque_contract"] = json!("full contract");
                    args["completion_policy"] = json!("director_acceptance");
                }
                "query" => args["selector"] = json!({"kind":"all"}),
                _ => {}
            }
            if matches!(*op, "assign" | "create_and_assign") {
                args["project_id"] = json!("p");
                args["task_id"] = json!("t");
                args["task_revision"] = json!(1);
                args["assignment_id"] = json!("assignment");
                args["assignee_cutex_session_id"] = json!("worker");
                args["summary"] = json!("full summary");
            }
            if matches!(
                *op,
                "accept_result" | "request_changes" | "fail_result" | "cancel"
            ) {
                args["assignment_id"] = json!("assignment");
            }
            if *op == "request_changes" {
                args["decision_reference"] = json!("review");
            }
            let mut call = cell("cutex", tool, args);
            assert_ne!(call.display_lines(80), call.transcript_lines(80));
            headers.push(render(call.display_lines(80)));
            let receipt = json!({"schema":"cutex/task-service-director-tool-receipt/v1","status":"committed","action_id":"action"});
            finish(&mut call, receipt.clone(), false);
            headers.push(
                render(call.display_lines(80))
                    .lines()
                    .next()
                    .unwrap()
                    .into(),
            );
            let mut wrong = receipt;
            wrong["action_id"] = json!("other");
            finish(&mut call, wrong, false);
            assert_eq!(call.display_lines(80), call.transcript_lines(80));
        }
    }
    for (tool, args, result) in [
        (
            "cutex_agent_list",
            json!({}),
            json!({"ok":true,"scope":"local_group_visible","agents":[]}),
        ),
        (
            "query_managed",
            json!({"action_id":"a"}),
            json!({"schema":"cutex/agent-management/v1","action_id":"a","outcome":{"status":"complete","receipt":{"schema":"cutex/agent-management-receipt/v1","action_id":"a","operation":"query_managed","result":{"kind":"query_managed"}}}}),
        ),
    ] {
        let mut call = cell("cutex", tool, args);
        finish(&mut call, result, false);
        assert_ne!(call.display_lines(80), call.transcript_lines(80));
        headers.push(
            render(call.display_lines(80))
                .lines()
                .next()
                .unwrap()
                .into(),
        );
    }
    for (tool, args, result) in [
        (
            "cancel",
            json!({"jobId":"job-1","expectedRevision":1}),
            job("cancelled"),
        ),
        (
            "read_output",
            json!({"jobId":"job-1","stream":"stdout"}),
            json!({"jobId":"job-1","stream":"stdout","fromOffset":0,"nextOffset":1,"bytesHex":"61","gap":false,"truncated":false}),
        ),
    ] {
        let mut call = cell("cutex_job", tool, args);
        finish(&mut call, result, false);
        assert_ne!(call.display_lines(80), call.transcript_lines(80));
        headers.push(
            render(call.display_lines(80))
                .lines()
                .next()
                .unwrap()
                .into(),
        );
    }
    insta::assert_snapshot!(headers.join("\n"));
}

#[test]
fn transport_error_and_contradictory_receipt() {
    let mut call = cell("cutex_job", "query", json!({"jobId":"job-1"}));
    call.complete(Duration::ZERO, Err("connection lost".into()));
    insta::assert_snapshot!(render(call.display_lines(40)));
    finish(&mut call, job("running"), true);
    assert_eq!(call.display_lines(80), call.transcript_lines(80));
}

#[test]
fn conflicting_structured_result_preserves_upstream_detail() {
    let mut call = cell("cutex_job", "query", json!({"jobId":"job-1"}));
    call.complete(
        Duration::ZERO,
        Ok(codex_protocol::mcp::CallToolResult {
            content: vec![json!({"type":"text","text":job("running").to_string()})],
            structured_content: Some(job("failed")),
            is_error: Some(false),
            meta: None,
        }),
    );
    assert_eq!(call.display_lines(80), call.transcript_lines(80));
}
