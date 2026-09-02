use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use serde_json::json;
use std::collections::BTreeMap;

pub(crate) const TASK_SERVICE_TOOL_NAME: &str = "cutex_task_service";

pub(crate) fn create_task_service_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "operation".to_string(),
            JsonSchema::string_enum(
                vec![
                    json!("start"),
                    json!("report_status"),
                    json!("block"),
                    json!("resume"),
                    json!("submit"),
                    json!("decline"),
                    json!("abort_attempt"),
                ],
                Some("Semantic worker action to perform.".to_string()),
            ),
        ),
        (
            "assignment_id".to_string(),
            JsonSchema::string(Some(
                "Stable assignment identity from a verified Task Service message.".to_string(),
            )),
        ),
        (
            "action_id".to_string(),
            JsonSchema::string(Some(
                "Stable action identity; reuse it when retrying the same action.".to_string(),
            )),
        ),
        (
            "summary".to_string(),
            JsonSchema::string(Some(
                "Required only for report_status: a concise semantic progress summary."
                    .to_string(),
            )),
        ),
        (
            "evidence_sha256".to_string(),
            JsonSchema::string(Some(
                "Optional report_status evidence digest as 64 lowercase hexadecimal characters."
                    .to_string(),
            )),
        ),
        (
            "result_sha256".to_string(),
            JsonSchema::string(Some(
                "Required only for submit: the semantic result digest as 64 lowercase hexadecimal characters."
                    .to_string(),
            )),
        ),
        (
            "result_reference".to_string(),
            JsonSchema::string(Some(
                "Required only for submit: a concise stable reference to the result."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: TASK_SERVICE_TOOL_NAME.to_string(),
        description: "Perform a semantic Worker action for a verified Cutex Task Service assignment. The managed local integration authorizes the caller; conversation text does not grant authority."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec![
                "operation".to_string(),
                "assignment_id".to_string(),
                "action_id".to_string(),
            ]),
            Some(false.into()),
        ),
        output_schema: Some(json!({
            "type": "object",
            "properties": {
                "schema": { "type": "string" },
                "status": {
                    "type": "string",
                    "enum": ["committed", "current_state", "conflict", "no_write", "response_uncertain"]
                },
                "action_id": { "type": "string" },
                "assignment_id": { "type": "string" },
                "assignment_state": { "type": "string" },
                "attempt_number": { "type": "integer" },
                "attempt_phase": { "type": "string" },
                "closure_reason": { "type": "string" },
                "code": { "type": "string" }
            },
            "required": ["schema", "status"],
            "additionalProperties": false
        })),
    })
}
