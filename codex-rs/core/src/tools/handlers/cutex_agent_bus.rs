use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::cutex_agent_bus_spec::CUTEX_AGENT_LIST_TOOL_NAME;
use crate::tools::handlers::cutex_agent_bus_spec::CUTEX_AGENT_SEND_TOOL_NAME;
use crate::tools::handlers::cutex_agent_bus_spec::create_cutex_agent_list_tool;
use crate::tools::handlers::cutex_agent_bus_spec::create_cutex_agent_send_tool;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use url::Url;

const CUTEX_AGENT_BUS_URL_ENV_VAR: &str = "CUTEX_AGENT_BUS_URL";
const CUTEX_AGENT_BUS_TOKEN_ENV_VAR: &str = "CUTEX_AGENT_BUS_TOKEN";
const CUTEX_AGENT_ID_ENV_VAR: &str = "CUTEX_AGENT_ID";
const CUTEX_AGENT_NAME_ENV_VAR: &str = "CUTEX_AGENT_NAME";

#[derive(Clone)]
struct CutexAgentBusConfig {
    base_url: String,
    token: Option<String>,
    agent_id: Option<String>,
    fallback_agent_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AgentBusAgent {
    id: String,
    name: String,
    #[serde(default)]
    base_name: Option<String>,
    #[serde(default)]
    path_key: Option<String>,
    profile: String,
    cwd: String,
    pid: u32,
    last_seen_epoch_secs: u64,
}

#[derive(Debug, Serialize)]
struct CutexAgentListResult {
    ok: bool,
    current_agent_id: String,
    agents: Vec<CutexAgentListEntry>,
    summary: String,
}

#[derive(Debug, Serialize)]
struct CutexAgentListEntry {
    id: String,
    name: String,
    base_name: Option<String>,
    path_key: Option<String>,
    profile: String,
    cwd: String,
    pid: u32,
    last_seen_epoch_secs: u64,
    this: bool,
}

#[derive(Debug, Deserialize)]
struct AgentBusSendArgs {
    to: String,
    message: String,
    #[serde(default, alias = "queueOnly")]
    queue_only: bool,
}

#[derive(Debug, Serialize)]
struct AgentBusSendRequest {
    to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    content: String,
    trigger_turn: bool,
}

#[derive(Debug, Deserialize)]
struct AgentBusSendResponse {
    id: String,
    #[serde(default)]
    from: Option<String>,
    to: String,
    #[serde(default, alias = "toName")]
    to_name: Option<String>,
    #[serde(alias = "triggerTurn")]
    trigger_turn: bool,
    queued: bool,
    #[serde(default)]
    deduplicated: bool,
}

#[derive(Debug, Serialize)]
struct CutexAgentSendResult {
    ok: bool,
    message_id: String,
    from: String,
    to: String,
    to_name: String,
    queued: bool,
    trigger_turn: bool,
    deduplicated: bool,
    summary: String,
}

pub(crate) struct CutexAgentListHandler;
pub(crate) struct CutexAgentSendHandler;

pub(crate) fn cutex_agent_bus_available() -> bool {
    std::env::var(CUTEX_AGENT_BUS_URL_ENV_VAR)
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

#[async_trait::async_trait]
impl ToolExecutor<ToolInvocation> for CutexAgentListHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(CUTEX_AGENT_LIST_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_cutex_agent_list_tool()
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        true
    }

    async fn handle(
        &self,
        invocation: ToolInvocation,
    ) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
        let _ = function_arguments(invocation.payload)?;
        let config = load_cutex_agent_bus_config()?;
        let agents = fetch_agents(&config).await?;
        let result = build_agent_list_result(&config, agents);
        Ok(boxed_tool_output(FunctionToolOutput::from_text(
            json_tool_result(&result, CUTEX_AGENT_LIST_TOOL_NAME),
            Some(true),
        )))
    }
}

impl CoreToolRuntime for CutexAgentListHandler {}

#[async_trait::async_trait]
impl ToolExecutor<ToolInvocation> for CutexAgentSendHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(CUTEX_AGENT_SEND_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_cutex_agent_send_tool()
    }

    async fn handle(
        &self,
        invocation: ToolInvocation,
    ) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
        let arguments = function_arguments(invocation.payload)?;
        let args: AgentBusSendArgs = parse_arguments(&arguments)?;
        if args.to.trim().is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "target agent `to` must not be empty".to_string(),
            ));
        }
        if args.message.trim().is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "agent message must not be empty".to_string(),
            ));
        }

        let config = load_cutex_agent_bus_config()?;
        let agents = fetch_agents(&config).await.unwrap_or_default();
        let sender = resolve_sender_name(&config, &agents);
        let request = AgentBusSendRequest {
            to: args.to,
            from: Some(sender.clone()),
            content: args.message,
            trigger_turn: !args.queue_only,
        };
        let response = post_message(&config, &request).await?;
        let result = build_send_result(&sender, response);
        Ok(boxed_tool_output(FunctionToolOutput::from_text(
            json_tool_result(&result, CUTEX_AGENT_SEND_TOOL_NAME),
            Some(true),
        )))
    }
}

impl CoreToolRuntime for CutexAgentSendHandler {}

fn function_arguments(payload: ToolPayload) -> Result<String, FunctionCallError> {
    match payload {
        ToolPayload::Function { arguments } => Ok(arguments),
        _ => Err(FunctionCallError::RespondToModel(
            "cutex agent handler received unsupported payload".to_string(),
        )),
    }
}

fn load_cutex_agent_bus_config() -> Result<CutexAgentBusConfig, FunctionCallError> {
    let base_url = std::env::var(CUTEX_AGENT_BUS_URL_ENV_VAR)
        .map_err(|_| {
            FunctionCallError::RespondToModel(
                "cutex agent bus is unavailable in this session".to_string(),
            )
        })?
        .trim_end_matches('/')
        .to_string();
    if base_url.trim().is_empty() {
        return Err(FunctionCallError::RespondToModel(
            "cutex agent bus URL is empty".to_string(),
        ));
    }
    validate_local_http_base_url(&base_url)?;

    let token = std::env::var(CUTEX_AGENT_BUS_TOKEN_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let agent_id = std::env::var(CUTEX_AGENT_ID_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let fallback_agent_name = std::env::var(CUTEX_AGENT_NAME_ENV_VAR)
        .ok()
        .filter(|value| !value.trim().is_empty());
    Ok(CutexAgentBusConfig {
        base_url,
        token,
        agent_id,
        fallback_agent_name,
    })
}

fn validate_local_http_base_url(base_url: &str) -> Result<(), FunctionCallError> {
    let url = Url::parse(base_url).map_err(|err| {
        FunctionCallError::RespondToModel(format!("invalid cutex agent bus URL: {err}"))
    })?;
    if url.scheme() != "http" {
        return Err(FunctionCallError::RespondToModel(
            "cutex agent bus only supports http:// URLs".to_string(),
        ));
    }
    let host = url.host_str().unwrap_or_default();
    if !matches!(host, "127.0.0.1" | "localhost" | "::1") {
        return Err(FunctionCallError::RespondToModel(
            "cutex agent bus URL must point to localhost".to_string(),
        ));
    }
    Ok(())
}

async fn fetch_agents(
    config: &CutexAgentBusConfig,
) -> Result<Vec<AgentBusAgent>, FunctionCallError> {
    let value = http_get_json(config, "/api/agents").await?;
    serde_json::from_value::<Vec<AgentBusAgent>>(value).map_err(|err| {
        FunctionCallError::RespondToModel(format!("failed to parse cutex agent list: {err}"))
    })
}

async fn post_message(
    config: &CutexAgentBusConfig,
    request: &AgentBusSendRequest,
) -> Result<AgentBusSendResponse, FunctionCallError> {
    let client = build_client()?;
    let mut builder = client
        .post(format!("{}{}", config.base_url, "/api/messages/send"))
        .json(request);
    if let Some(token) = &config.token {
        builder = builder.bearer_auth(token);
    }
    let response = builder.send().await.map_err(|err| {
        FunctionCallError::RespondToModel(format!("cutex agent send failed: {err}"))
    })?;
    let status = response.status();
    let text = response.text().await.map_err(|err| {
        FunctionCallError::RespondToModel(format!("cutex agent send response failed: {err}"))
    })?;
    if !status.is_success() {
        return Err(FunctionCallError::RespondToModel(format!(
            "cutex agent bus returned {status}: {text}"
        )));
    }
    serde_json::from_str::<AgentBusSendResponse>(&text).map_err(|err| {
        FunctionCallError::RespondToModel(format!(
            "failed to parse cutex agent send response: {err}"
        ))
    })
}

async fn http_get_json(
    config: &CutexAgentBusConfig,
    path: &str,
) -> Result<Value, FunctionCallError> {
    let client = build_client()?;
    let mut builder = client.get(format!("{}{}", config.base_url, path));
    if let Some(token) = &config.token {
        builder = builder.bearer_auth(token);
    }
    let response = builder.send().await.map_err(|err| {
        FunctionCallError::RespondToModel(format!("cutex agent bus request failed: {err}"))
    })?;
    let status = response.status();
    let text = response.text().await.map_err(|err| {
        FunctionCallError::RespondToModel(format!("cutex agent bus response failed: {err}"))
    })?;
    if !status.is_success() {
        return Err(FunctionCallError::RespondToModel(format!(
            "cutex agent bus returned {status}: {text}"
        )));
    }
    serde_json::from_str::<Value>(&text).map_err(|err| {
        FunctionCallError::RespondToModel(format!("failed to parse cutex agent bus JSON: {err}"))
    })
}

fn build_client() -> Result<Client, FunctionCallError> {
    Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|err| {
            FunctionCallError::RespondToModel(format!(
                "failed to build cutex agent HTTP client: {err}"
            ))
        })
}

fn resolve_sender_name(config: &CutexAgentBusConfig, agents: &[AgentBusAgent]) -> String {
    if let Some(agent_id) = config.agent_id.as_deref()
        && let Some(agent) = agents.iter().find(|agent| agent.id == agent_id)
    {
        return agent
            .base_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(agent.name.as_str())
            .to_string();
    }
    config
        .fallback_agent_name
        .clone()
        .or_else(|| config.agent_id.clone())
        .unwrap_or_else(|| "cutex".to_string())
}

fn build_agent_list_result(
    config: &CutexAgentBusConfig,
    agents: Vec<AgentBusAgent>,
) -> CutexAgentListResult {
    let current_agent_id = config.agent_id.clone().unwrap_or_else(|| "-".to_string());
    let agent_count = agents.len();
    let agents = agents
        .into_iter()
        .map(|agent| {
            let is_current = Some(agent.id.as_str()) == config.agent_id.as_deref();
            CutexAgentListEntry {
                id: agent.id,
                name: agent.name,
                base_name: agent.base_name,
                path_key: agent.path_key,
                profile: agent.profile,
                cwd: agent.cwd,
                pid: agent.pid,
                last_seen_epoch_secs: agent.last_seen_epoch_secs,
                this: is_current,
            }
        })
        .collect::<Vec<_>>();
    CutexAgentListResult {
        ok: true,
        current_agent_id,
        agents,
        summary: format!("Listed {agent_count} cutex agent(s)."),
    }
}

fn build_send_result(sender: &str, response: AgentBusSendResponse) -> CutexAgentSendResult {
    let target = response
        .to_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(response.to.as_str())
        .to_string();
    let actual_sender = response
        .from
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(sender)
        .to_string();
    let summary = format!(
        "Sent message {} from {} to {} ({}) queued={} trigger_turn={} deduplicated={}",
        response.id,
        actual_sender,
        target,
        response.to,
        response.queued,
        response.trigger_turn,
        response.deduplicated
    );
    CutexAgentSendResult {
        ok: true,
        message_id: response.id,
        from: actual_sender,
        to: response.to,
        to_name: target,
        queued: response.queued,
        trigger_turn: response.trigger_turn,
        deduplicated: response.deduplicated,
        summary,
    }
}

fn json_tool_result<T: Serialize>(value: &T, tool_name: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|err| {
        serde_json::json!({
            "ok": false,
            "summary": format!("failed to serialize {tool_name} result: {err}")
        })
        .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, name: &str, base_name: Option<&str>) -> AgentBusAgent {
        AgentBusAgent {
            id: id.to_string(),
            name: name.to_string(),
            base_name: base_name.map(str::to_string),
            path_key: Some("abc1234".to_string()),
            profile: "aemeath".to_string(),
            cwd: "/tmp/project".to_string(),
            pid: 123,
            last_seen_epoch_secs: 1,
        }
    }

    #[test]
    fn sender_name_prefers_live_base_name() {
        let config = CutexAgentBusConfig {
            base_url: "http://127.0.0.1:24260".to_string(),
            token: None,
            agent_id: Some("agent-1".to_string()),
            fallback_agent_name: Some("stale".to_string()),
        };
        let agents = vec![agent("agent-1", "msgbot-1.abc1234", Some("msgbot-1"))];

        assert_eq!(resolve_sender_name(&config, &agents), "msgbot-1");
    }

    #[test]
    fn sender_name_falls_back_to_env_name() {
        let config = CutexAgentBusConfig {
            base_url: "http://127.0.0.1:24260".to_string(),
            token: None,
            agent_id: Some("agent-1".to_string()),
            fallback_agent_name: Some("aemeath".to_string()),
        };

        assert_eq!(resolve_sender_name(&config, &[]), "aemeath");
    }

    #[test]
    fn validates_local_agent_bus_url() {
        assert!(validate_local_http_base_url("http://127.0.0.1:24260").is_ok());
        assert!(validate_local_http_base_url("https://127.0.0.1:24260").is_err());
        assert!(validate_local_http_base_url("http://example.com:24260").is_err());
    }

    #[test]
    fn send_result_is_structured_json() {
        let result = build_send_result(
            "msgbot-1",
            AgentBusSendResponse {
                id: "message-1".to_string(),
                from: Some("msgbot-1".to_string()),
                to: "agent-2".to_string(),
                to_name: Some("msgbot-2.abc1234".to_string()),
                trigger_turn: true,
                queued: true,
                deduplicated: false,
            },
        );

        let value: serde_json::Value =
            serde_json::from_str(&json_tool_result(&result, CUTEX_AGENT_SEND_TOOL_NAME)).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["from"], "msgbot-1");
        assert_eq!(value["to_name"], "msgbot-2.abc1234");
        assert_eq!(value["queued"], true);
        assert_eq!(value["deduplicated"], false);
    }

    #[test]
    fn send_args_default_to_trigger_turn() {
        let args: AgentBusSendArgs = serde_json::from_value(serde_json::json!({
            "to": "worker",
            "message": "please report"
        }))
        .expect("send args should parse");

        assert!(!args.queue_only);
    }

    #[test]
    fn send_args_accept_queue_only() {
        let args: AgentBusSendArgs = serde_json::from_value(serde_json::json!({
            "to": "worker",
            "message": "low-priority note",
            "queue_only": true
        }))
        .expect("send args should parse");

        assert!(args.queue_only);
    }

    #[test]
    fn send_args_ignore_legacy_trigger_turn_field() {
        let args: AgentBusSendArgs = serde_json::from_value(serde_json::json!({
            "to": "worker",
            "message": "status that should still wake",
            "trigger_turn": false
        }))
        .expect("send args should parse");

        assert!(!args.queue_only);
    }

    #[test]
    fn send_request_uses_snake_case_bus_wire_format() {
        let request = AgentBusSendRequest {
            to: "worker".to_string(),
            from: Some("leader".to_string()),
            content: "please report".to_string(),
            trigger_turn: false,
        };

        let value = serde_json::to_value(&request).expect("request should encode");
        assert_eq!(value["trigger_turn"], false);
        assert!(value.get("triggerTurn").is_none());
    }

    #[test]
    fn send_response_accepts_snake_case_bus_wire_format() {
        let response: AgentBusSendResponse = serde_json::from_value(serde_json::json!({
            "id": "message-1",
            "from": "leader",
            "to": "agent-2",
            "to_name": "worker.abc1234",
            "trigger_turn": false,
            "queued": true,
            "deduplicated": true
        }))
        .expect("snake_case cutex bus response should parse");

        assert_eq!(response.to_name.as_deref(), Some("worker.abc1234"));
        assert!(!response.trigger_turn);
        assert!(response.deduplicated);
    }

    #[test]
    fn send_response_accepts_camel_case_for_compatibility() {
        let response: AgentBusSendResponse = serde_json::from_value(serde_json::json!({
            "id": "message-1",
            "from": "leader",
            "to": "agent-2",
            "toName": "worker.abc1234",
            "triggerTurn": true,
            "queued": true,
            "deduplicated": false
        }))
        .expect("camelCase response should parse for compatibility");

        assert_eq!(response.to_name.as_deref(), Some("worker.abc1234"));
        assert!(response.trigger_turn);
        assert!(!response.deduplicated);
    }
}
