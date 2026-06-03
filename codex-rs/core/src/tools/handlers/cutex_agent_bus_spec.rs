use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use std::collections::BTreeMap;

pub(crate) const CUTEX_AGENT_LIST_TOOL_NAME: &str = "cutex_agent_list";
pub(crate) const CUTEX_AGENT_SEND_TOOL_NAME: &str = "cutex_agent_send";

pub(crate) fn create_cutex_agent_list_tool() -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: CUTEX_AGENT_LIST_TOOL_NAME.to_string(),
        description: "List peer agents currently registered with the local cutex agent bus. Use this before sending an inter-agent message when the target is not already clear."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(BTreeMap::new(), Some(Vec::new()), Some(false.into())),
        output_schema: None,
    })
}

pub(crate) fn create_cutex_agent_send_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "to".to_string(),
            JsonSchema::string(Some(
                "Target agent id, display name, or unique thread name from cutex_agent_list."
                    .to_string(),
            )),
        ),
        (
            "message".to_string(),
            JsonSchema::string(Some("Message text to deliver to the target agent.".to_string())),
        ),
        (
            "queue_only".to_string(),
            JsonSchema::boolean(Some(
                "Set true only when the user explicitly asks to queue a low-priority message without waking the recipient. Omit for normal replies and task/status messages so the recipient can respond promptly."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: CUTEX_AGENT_SEND_TOOL_NAME.to_string(),
        description: "Send a structured message to another cute-codex session registered with cutex. Use this for agent-to-agent communication instead of shelling out to `cutex agent send`; cutex labels the sender automatically."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            Some(vec!["to".to_string(), "message".to_string()]),
            Some(false.into()),
        ),
        output_schema: None,
    })
}
