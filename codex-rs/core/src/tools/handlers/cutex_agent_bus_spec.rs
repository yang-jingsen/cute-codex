use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use std::collections::BTreeMap;

pub(crate) const CUTEX_AGENT_LIST_TOOL_NAME: &str = "cutex_agent_list";
pub(crate) const CUTEX_AGENT_SEND_TOOL_NAME: &str = "cutex_agent_send";

pub(crate) fn create_cutex_agent_list_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "all_groups".to_string(),
            JsonSchema::boolean(Some(
                "Set true to list every registered cutex agent across collaboration groups. Default false lists only agents visible to this session's group scope."
                    .to_string(),
            )),
        ),
        (
            "all_hosts".to_string(),
            JsonSchema::boolean(Some(
                "Set true to list agents across Bridgeboard-connected hosts. Agent sessions default this to true."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: CUTEX_AGENT_LIST_TOOL_NAME.to_string(),
        description: "List peer agents currently registered with the local cutex agent bus. Use this before sending an inter-agent message when the target is not already clear."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, Some(Vec::new()), Some(false.into())),
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
            "all_groups".to_string(),
            JsonSchema::boolean(Some(
                "Set true to resolve the target across every registered cutex agent group. Default false only searches agents visible to this session's collaboration groups; exact full runtime ids are accepted by the bus."
                    .to_string(),
            )),
        ),
        (
            "all_hosts".to_string(),
            JsonSchema::boolean(Some(
                "Set true to resolve the target across Bridgeboard-connected hosts. Agent sessions default this to true."
                    .to_string(),
            )),
        ),
        (
            "delivery_mode".to_string(),
            JsonSchema::string(Some(
                "How the recipient should process the message. Use after_turn by default so the recipient handles it after its current turn. Use soon only for urgent follow-up. Use passive for FYI/no-action messages."
                    .to_string(),
            )),
        ),
        (
            "queue_only".to_string(),
            JsonSchema::boolean(Some(
                "Deprecated compatibility flag. Set true only for passive FYI/no-action messages; prefer delivery_mode=\"passive\"."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: CUTEX_AGENT_SEND_TOOL_NAME.to_string(),
        description: "Send a structured message to another cute-codex session registered with cutex. Use this for agent-to-agent communication instead of shelling out to `cutex agent send`; cutex labels the sender automatically. Default delivery is after_turn."
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cutex_agent_list_schema_keeps_scope_flags_optional() {
        let ToolSpec::Function(tool) = create_cutex_agent_list_tool() else {
            panic!("cutex_agent_list must be a function tool");
        };
        let value = serde_json::to_value(tool).expect("list tool should serialize");

        assert_eq!(value["name"], CUTEX_AGENT_LIST_TOOL_NAME);
        assert_eq!(value["parameters"]["required"], json!([]));
        assert_eq!(value["parameters"]["additionalProperties"], false);
        assert_eq!(
            value["parameters"]["properties"]["all_groups"]["type"],
            "boolean"
        );
        assert_eq!(
            value["parameters"]["properties"]["all_hosts"]["type"],
            "boolean"
        );
    }

    #[test]
    fn cutex_agent_send_schema_requires_target_and_message() {
        let ToolSpec::Function(tool) = create_cutex_agent_send_tool() else {
            panic!("cutex_agent_send must be a function tool");
        };
        let value = serde_json::to_value(tool).expect("send tool should serialize");

        assert_eq!(value["name"], CUTEX_AGENT_SEND_TOOL_NAME);
        assert_eq!(value["parameters"]["required"], json!(["to", "message"]));
        assert_eq!(value["parameters"]["additionalProperties"], false);
        assert_eq!(
            value["parameters"]["properties"]["delivery_mode"]["type"],
            "string"
        );
        assert_eq!(
            value["parameters"]["properties"]["queue_only"]["type"],
            "boolean"
        );
    }
}
