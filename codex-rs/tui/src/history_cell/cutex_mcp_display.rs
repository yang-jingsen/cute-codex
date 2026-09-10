//! Exact configured-name styling, never identity verification or execution policy.
//! Unknown tools/argument shapes retain upstream presentation.
use super::mcp::McpInvocation;
use serde_json::Map;
use serde_json::Value;
use unicode_segmentation::UnicodeSegmentation;

fn text<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key)?.as_str().filter(|s| !s.is_empty())
}

fn known_fields(args: &Map<String, Value>, fields: &[&str]) -> bool {
    args.keys().all(|key| fields.contains(&key.as_str()))
}

pub(super) fn title(invocation: &McpInvocation) -> Option<String> {
    let args = invocation.arguments.as_ref()?.as_object()?;
    let (action, target) = match (invocation.server.as_str(), invocation.tool.as_str()) {
        ("cutex_job", "submit") if known_fields(args, &["actionId", "argv", "cwd"]) => {
            text(args, "cwd")?;
            let argv = args.get("argv")?.as_array()?;
            if argv.is_empty() || !argv.iter().all(Value::is_string) {
                return None;
            }
            ("Job submit", text(args, "actionId")?)
        }
        ("cutex_job", "query") if known_fields(args, &["jobId"]) => {
            ("Job query", text(args, "jobId")?)
        }
        ("cutex_job", "read_output")
            if known_fields(args, &["jobId", "stream", "offset", "maxBytes"]) =>
        {
            if !matches!(text(args, "stream")?, "stdout" | "stderr") {
                return None;
            }
            if args.get("offset").is_some_and(|v| v.as_u64().is_none())
                || args
                    .get("maxBytes")
                    .is_some_and(|v| !v.as_u64().is_some_and(|n| (1..=1048576).contains(&n)))
            {
                return None;
            }
            ("Job read output", text(args, "jobId")?)
        }
        ("cutex_job", "cancel") if known_fields(args, &["jobId", "expectedRevision"]) => {
            if args.get("expectedRevision")?.as_u64()? == 0 {
                return None;
            }
            ("Job cancel", text(args, "jobId")?)
        }
        ("cutex", "send")
            if known_fields(
                args,
                &["to", "message", "external_message_id", "delivery_mode"],
            ) =>
        {
            text(args, "message")?;
            text(args, "external_message_id")?;
            if !matches!(
                text(args, "delivery_mode")?,
                "after_turn" | "soon" | "passive" | "interrupt"
            ) {
                return None;
            }
            ("Message send", text(args, "to")?)
        }
        ("cutex", "cutex_agent_management") => {
            if !known_fields(
                args,
                &[
                    "operation",
                    "action_id",
                    "project_id",
                    "cutex_session_id",
                    "spec",
                    "start_mode",
                    "frozen_message",
                    "bootstrap_intent",
                ],
            ) {
                return None;
            }
            text(args, "action_id")?;
            for key in [
                "project_id",
                "frozen_message",
                "bootstrap_intent",
                "cutex_session_id",
            ] {
                if args.get(key).is_some_and(|v| !v.is_string()) {
                    return None;
                }
            }
            match text(args, "operation")? {
                "create" if !args.contains_key("cutex_session_id") => {
                    if !matches!(
                        text(args, "start_mode")?,
                        "bootstrap_only" | "custom_message"
                    ) {
                        return None;
                    }
                    if text(args, "start_mode")? == "custom_message" {
                        text(args, "frozen_message")?;
                    }
                    let spec = args.get("spec")?.as_object()?;
                    if !known_fields(
                        spec,
                        &[
                            "name",
                            "cwd",
                            "profile",
                            "runtime_backend",
                            "model",
                            "reasoning",
                            "permissions",
                            "approval_policy",
                            "sandbox_mode",
                            "groups",
                            "expose_to_im",
                            "pin",
                        ],
                    ) {
                        return None;
                    }
                    for key in [
                        "name",
                        "cwd",
                        "profile",
                        "runtime_backend",
                        "model",
                        "reasoning",
                        "permissions",
                        "approval_policy",
                        "sandbox_mode",
                    ] {
                        text(spec, key)?;
                    }
                    if !spec.get("groups")?.as_array()?.iter().all(Value::is_string)
                        || ["expose_to_im", "pin"]
                            .iter()
                            .any(|key| spec.get(*key).is_some_and(|v| !v.is_boolean()))
                    {
                        return None;
                    }
                    ("Agent create", text(spec, "name")?)
                }
                "restart"
                    if known_fields(
                        args,
                        &["operation", "action_id", "project_id", "cutex_session_id"],
                    ) =>
                {
                    ("Agent restart", text(args, "cutex_session_id")?)
                }
                _ => return None,
            }
        }
        ("cutex", "cutex_task_service_terminal" | "cutex_task_service_director")
            if known_fields(
                args,
                &[
                    "operation",
                    "action_id",
                    "assignment_id",
                    "decision_reference",
                ],
            ) =>
        {
            text(args, "action_id")?;
            if args
                .get("decision_reference")
                .is_some_and(|v| !v.is_string())
            {
                return None;
            }
            let action = match text(args, "operation")? {
                "accept_result" => "Task accept result",
                "request_changes" => {
                    text(args, "decision_reference")?;
                    "Task request changes"
                }
                "fail_result" => "Task fail result",
                _ => return None,
            };
            (action, text(args, "assignment_id")?)
        }
        ("cutex", "cutex_task_service_director")
            if known_fields(
                args,
                &[
                    "operation",
                    "action_id",
                    "project_id",
                    "task_id",
                    "task_revision",
                    "assignment_id",
                    "assignee_cutex_session_id",
                    "summary",
                ],
            ) =>
        {
            if text(args, "operation")? != "assign" {
                return None;
            }
            for key in [
                "action_id",
                "project_id",
                "task_id",
                "assignee_cutex_session_id",
                "summary",
            ] {
                text(args, key)?;
            }
            if !args
                .get("task_revision")?
                .as_u64()
                .is_some_and(|n| (1..=9007199254740991).contains(&n))
            {
                return None;
            }
            ("Task assign", text(args, "assignment_id")?)
        }
        _ => return None,
    };
    let clean = super::messages::sanitize_user_text(target.into());
    let target = clean.split_whitespace().collect::<Vec<_>>().join(" ");
    let target = target.graphemes(true).take(80).collect::<String>();
    Some(format!("{action} · {target}"))
}

/// An explicitly versioned but unknown result must keep the upstream presentation.
pub(super) fn known_result_version(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return true;
    };
    match value.get("schema") {
        None => true,
        Some(Value::String(schema)) => matches!(
            schema.as_str(),
            "cutex/agent-management/v1"
                | "cutex/task-service-tool-receipt/v1"
                | "cutex/task-service-director-tool-receipt/v1"
        ),
        Some(_) => false,
    }
}

/// Only known returned receipt schemas can refine the display status. Never inspect arguments.
pub(super) fn outcome_uncertain(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return false;
    };
    match value.get("schema").and_then(Value::as_str) {
        Some("cutex/agent-management/v1") => value["outcome"]["code"] == "response_uncertain",
        Some(
            "cutex/task-service-tool-receipt/v1" | "cutex/task-service-director-tool-receipt/v1",
        ) => value["status"] == "response_uncertain",
        _ => false,
    }
}

#[cfg(test)]
#[path = "cutex_mcp_display_tests.rs"]
mod tests;
