//! Conservative display interpretation of known returned receipts. No effects or authority.
use super::*;

pub(in crate::history_cell) struct Outcome {
    pub(in crate::history_cell) text: String,
    pub(in crate::history_cell) failed: bool,
    pub(in crate::history_cell) uncertain: bool,
}

pub(in crate::history_cell) fn outcome(invocation: &McpInvocation, body: &str) -> Option<Outcome> {
    let preset = lookup(invocation)?;
    let value: Value = serde_json::from_str(body).ok()?;
    let args = invocation.arguments.as_ref()?;
    let mut failed = false;
    let mut uncertain = false;
    let summary = if preset.server == "cutex_job" {
        if preset.tool == "read_output" {
            if value["jobId"] != args["jobId"]
                || value["stream"] != args["stream"]
                || value["fromOffset"].as_u64().is_none()
                || value["nextOffset"].as_u64().is_none()
                || value["bytesHex"].as_str().is_none()
            {
                return None;
            }
            format!(
                "Read job output · {} · {}",
                value["jobId"].as_str()?,
                value["stream"].as_str()?
            )
        } else {
            let job = if preset.tool == "submit" {
                if value["status"] != "committed" || !value["deduplicated"].is_boolean() {
                    return None;
                }
                &value["job"]
            } else {
                &value
            };
            if job["schema"] != "cutex/job-service-core/v1" || job["revision"].as_u64()? == 0 {
                return None;
            }
            let id = job["jobId"].as_str()?;
            if preset.tool == "submit" && job["request"]["actionId"] != args["actionId"] {
                return None;
            }
            if preset.tool != "submit" && job["jobId"] != args["jobId"] {
                return None;
            }
            let state = job["state"].as_str()?;
            if !matches!(
                state,
                "launch_pending"
                    | "running"
                    | "exited"
                    | "failed"
                    | "cancelled"
                    | "interrupted"
                    | "launch_unknown"
            ) {
                return None;
            }
            failed = matches!(state, "failed" | "interrupted")
                || (state == "exited" && job["exitCode"].as_i64().is_some_and(|n| n != 0));
            uncertain = state == "launch_unknown";
            let verb = match preset.tool {
                "submit" => "Submitted job",
                "cancel" => "Job cancellation returned",
                _ => "Queried job",
            };
            format!("{verb} · {id} · {state}")
        }
    } else if preset.tool == "send" {
        if value["ok"] != true || value["to"] != args["to"] || value["id"].as_str().is_none() {
            return None;
        }
        if value["queued"] != true {
            return None;
        }
        format!("Queued message · {}", target(invocation, preset))
    } else if preset.tool == "cutex_agent_list" {
        if value["ok"] != true || value["scope"] != "local_group_visible" {
            return None;
        }
        format!(
            "Listed agents · {} visible",
            value["agents"].as_array()?.len()
        )
    } else if matches!(preset.tool, "cutex_agent_management" | "query_managed") {
        if value["schema"] != "cutex/agent-management/v1" || value["action_id"] != args["action_id"]
        {
            return None;
        }
        let result = &value["outcome"];
        match result["status"].as_str()? {
            "no_write" => {
                uncertain = result["code"] == "response_uncertain";
                failed = !uncertain;
                format!(
                    "{} · {}",
                    if uncertain {
                        "Outcome uncertain"
                    } else {
                        "No successful receipt"
                    },
                    target(invocation, preset)
                )
            }
            "owner_action_required" => {
                failed = true;
                format!("Owner action required · {}", target(invocation, preset))
            }
            "complete" => {
                let receipt = &result["receipt"];
                let op = if preset.operation.is_empty() {
                    "query_managed"
                } else {
                    preset.operation
                };
                if receipt["schema"] != "cutex/agent-management-receipt/v1"
                    || receipt["action_id"] != args["action_id"]
                    || receipt["operation"] != op
                {
                    return None;
                }
                let (kind, verb) = match op {
                    "create" => ("created", "Created agent"),
                    "query_managed" => ("query_managed", "Queried managed agents"),
                    "replace" => ("replaced", "Replaced agent"),
                    "director_rotate" => ("director_rotated", "Rotated director"),
                    "online" => ("lifecycle", "Agent online action recorded"),
                    "offline" => ("lifecycle", "Agent offline action recorded"),
                    "restart" => ("lifecycle", "Agent restart recorded"),
                    "close" => ("lifecycle", "Agent close recorded"),
                    _ => return None,
                };
                if receipt["result"]["kind"] != kind {
                    return None;
                }
                format!("{verb} · {}", target(invocation, preset))
            }
            _ => return None,
        }
    } else {
        let schema = if preset.tool == "cutex_task_service" {
            "cutex/task-service-tool-receipt/v1"
        } else {
            "cutex/task-service-director-tool-receipt/v1"
        };
        if value["schema"] != schema
            || value
                .get("action_id")
                .is_some_and(|id| id != &args["action_id"])
        {
            return None;
        }
        let status = value["status"].as_str()?;
        let label = match status {
            "response_uncertain" => {
                uncertain = true;
                "Outcome uncertain"
            }
            "no_write" | "conflict" => {
                failed = true;
                "Task action not confirmed"
            }
            "current_state" => "Task state returned",
            "committed" => match preset.operation {
                "start" => "Started task",
                "report_status" => "Reported task status",
                "block" => "Blocked task",
                "resume" => "Resumed task",
                "submit" => "Submitted task result",
                "decline" => "Declined task",
                "abort_attempt" => "Aborted task attempt",
                "create_revision" => "Created task revision",
                "assign" => "Assigned task",
                "create_and_assign" => "Created and assigned task",
                "query" => "Queried tasks",
                "accept_result" => "Accepted task result",
                "request_changes" => "Requested task changes",
                "fail_result" => "Rejected task result",
                "cancel" => "Cancelled task",
                _ => return None,
            },
            _ => return None,
        };
        format!("{label} · {}", target(invocation, preset))
    };
    Some(Outcome {
        text: summary.trim_end_matches(" · ").into(),
        failed,
        uncertain,
    })
}
