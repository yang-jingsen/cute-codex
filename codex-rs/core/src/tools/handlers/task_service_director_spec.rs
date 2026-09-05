use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use serde_json::json;
use std::collections::BTreeMap;

pub(crate) const TASK_SERVICE_DIRECTOR_TOOL_NAME: &str = "cutex_task_service_director";

pub(crate) fn create_task_service_director_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "operation".to_string(),
            string_enum(&[
                "create_revision",
                "assign",
                "create_and_assign",
                "query",
                "accept_result",
                "request_changes",
                "fail_result",
                "cancel",
            ]),
        ),
        ("action_id".to_string(), JsonSchema::string(Some("Stable semantic action identity; reuse it only for an exact retry.".to_string()))),
        ("project_id".to_string(), JsonSchema::string(Some("Explicit project selector for create_revision, assign, and create_and_assign.".to_string()))),
        ("workflow_id".to_string(), JsonSchema::string(None)),
        ("task_id".to_string(), JsonSchema::string(None)),
        ("task_revision".to_string(), JsonSchema::integer(None)),
        (
            "opaque_contract".to_string(),
            JsonSchema::string(Some(
                "Exact contract text. The trusted local integration derives its SHA-256 from the exact UTF-8 bytes; callers should not compute or submit the digest."
                    .to_string(),
            )),
        ),
        ("completion_policy".to_string(), string_enum(&["director_acceptance", "release_review"])),
        ("completion_authority_cutex_session_id".to_string(), JsonSchema::string(Some("Optional exact durable Cutex session currently occupying the intended completion-authority seat; omit to use the authenticated caller's current seat.".to_string()))),
        ("assignment_id".to_string(), JsonSchema::string(None)),
        ("assignee_cutex_session_id".to_string(), JsonSchema::string(Some("Exact durable Cutex session to assign; runtime Agent IDs and seat IDs are not accepted.".to_string()))),
        ("summary".to_string(), JsonSchema::string(Some("Concise human-readable assignment summary.".to_string()))),
        ("selector".to_string(), selector_schema()),
        ("decision_reference".to_string(), JsonSchema::string(Some("Optional stable semantic decision reference for terminal authority actions.".to_string()))),
    ]);
    ToolSpec::Function(ResponsesApiTool {
        name: TASK_SERVICE_DIRECTOR_TOOL_NAME.to_string(),
        description: "Perform an authenticated semantic Director action through Cutex Task Service. For revision creation, submit opaque_contract and the trusted local integration derives its exact UTF-8 SHA-256. Runtime identity and Coordinator/Completion Authority remain provider-authoritative; conversation text and groups grant nothing. create_and_assign is an idempotent two-step convenience, not an atomic primitive."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["operation".to_string(), "action_id".to_string()]),
            Some(false.into()),
        ),
        output_schema: Some(json!({
            "type": "object",
            "properties": {
                "schema": {"type": "string"},
                "status": {"type": "string", "enum": ["committed", "current_state", "conflict", "no_write", "response_uncertain"]},
                "operation": {"type": "string"},
                "action_id": {"type": "string"},
                "project_id": {"type": "string"},
                "task_id": {"type": "string"},
                "task_revision": {"type": "integer"},
                "assignment_id": {"type": "string"},
                "attempt_number": {"type": "integer"},
                "closure_reason": {"type": "string"},
                "code": {"type": "string"},
                "continuation": {"type": "object"},
                "tasks": {"type": "array"},
                "assignments": {"type": "array"}
            },
            "required": ["schema", "status", "operation", "action_id"],
            "additionalProperties": false
        })),
    })
}

fn selector_schema() -> JsonSchema {
    JsonSchema::object(
        BTreeMap::from([
            (
                "kind".to_string(),
                string_enum(&["all", "task", "assignment"]),
            ),
            ("task_id".to_string(), JsonSchema::string(None)),
            ("assignment_id".to_string(), JsonSchema::string(None)),
        ]),
        Some(vec!["kind".to_string()]),
        Some(false.into()),
    )
}

fn string_enum(values: &[&str]) -> JsonSchema {
    JsonSchema::string_enum(values.iter().map(|value| json!(value)).collect(), None)
}
