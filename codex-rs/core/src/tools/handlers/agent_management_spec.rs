use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use serde_json::json;
use std::collections::BTreeMap;

pub(crate) const AGENT_MANAGEMENT_TOOL_NAME: &str = "cutex_agent_management";

pub(crate) fn create_agent_management_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "operation".to_string(),
            string_enum(&[
                "create",
                "query_managed",
                "online",
                "offline",
                "restart",
                "close",
                "replace",
                "director_rotate",
            ]),
        ),
        ("action_id".to_string(), JsonSchema::string(None)),
        ("project_id".to_string(), JsonSchema::string(None)),
        ("spec".to_string(), managed_agent_spec()),
        ("cutex_session_id".to_string(), JsonSchema::string(None)),
        (
            "predecessor_cutex_session_id".to_string(),
            JsonSchema::string(None),
        ),
        (
            "policy".to_string(),
            string_enum(&["close_before_create", "close_after_ready", "keep_old"]),
        ),
        ("successor".to_string(), managed_agent_spec()),
        (
            "start_mode".to_string(),
            string_enum(&["bootstrap_only", "custom_message"]),
        ),
        ("frozen_message".to_string(), JsonSchema::string(None)),
        (
            "expected_predecessor_cutex_session".to_string(),
            JsonSchema::string(None),
        ),
        (
            "expected_authority_epoch".to_string(),
            JsonSchema::integer(None),
        ),
        (
            "mode".to_string(),
            string_enum(&[
                "close_predecessor_then_create_with_message",
                "retain_predecessor_with_message",
                "retain_predecessor_bootstrap_only",
            ]),
        ),
    ]);
    ToolSpec::Function(ResponsesApiTool {
        name: AGENT_MANAGEMENT_TOOL_NAME.to_string(),
        description: "Perform one typed project-scoped Cutex Agent Management v1 action. Caller identity and authority come only from the authenticated durable Cutex runtime. Optional project_id only selects among projects already authorized by the provider and never grants authority. Reuse action_id only for an exact replay."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["operation".to_string(), "action_id".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

fn managed_agent_spec() -> JsonSchema {
    let required = [
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
    ];
    let mut properties: BTreeMap<String, JsonSchema> = required
        .iter()
        .map(|field| (field.to_string(), JsonSchema::string(None)))
        .collect();
    properties.insert(
        "groups".to_string(),
        JsonSchema::array(JsonSchema::string(None), None),
    );
    properties.insert("expose_to_im".to_string(), JsonSchema::boolean(None));
    properties.insert("pin".to_string(), JsonSchema::boolean(None));
    JsonSchema::object(
        properties,
        Some(required.into_iter().map(ToString::to_string).collect()),
        Some(false.into()),
    )
}

fn string_enum(values: &[&str]) -> JsonSchema {
    JsonSchema::string_enum(values.iter().map(|value| json!(value)).collect(), None)
}
