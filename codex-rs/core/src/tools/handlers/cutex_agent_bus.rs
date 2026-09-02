use crate::function_tool::FunctionCallError;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::cutex_agent_bus_spec::CUTEX_AGENT_LIST_TOOL_NAME;
use crate::tools::handlers::cutex_agent_bus_spec::CUTEX_AGENT_SEND_TOOL_NAME;
use crate::tools::handlers::cutex_agent_bus_spec::create_cutex_agent_list_tool;
use crate::tools::handlers::cutex_agent_bus_spec::create_cutex_agent_send_tool;
use crate::tools::handlers::parse_arguments;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_protocol::protocol::InterAgentDeliveryMode;
use codex_tools::JsonToolOutput;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use reqwest::Client;
use reqwest::redirect::Policy;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use url::Url;
use url::form_urlencoded;

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
    #[serde(default, alias = "sessionId")]
    session_id: Option<String>,
    profile: String,
    cwd: String,
    pid: u32,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default, alias = "registrationClass")]
    registration_class: Option<String>,
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
    session_id: Option<String>,
    profile: String,
    cwd: String,
    pid: u32,
    groups: Vec<String>,
    registration_class: Option<String>,
    last_seen_epoch_secs: u64,
    this: bool,
}

#[derive(Debug, Deserialize)]
struct AgentBusListArgs {
    #[serde(default, alias = "allGroups")]
    all_groups: bool,
    #[serde(default, alias = "allHosts")]
    all_hosts: bool,
}

#[derive(Debug, Deserialize)]
struct AgentBusSendArgs {
    to: String,
    message: String,
    #[serde(default, alias = "allGroups")]
    all_groups: bool,
    #[serde(default, alias = "allHosts")]
    all_hosts: bool,
    #[serde(default, alias = "deliveryMode")]
    delivery_mode: Option<InterAgentDeliveryMode>,
    #[serde(default, alias = "queueOnly")]
    queue_only: bool,
}

#[derive(Debug, Serialize)]
struct AgentBusSendRequest {
    to: String,
    #[serde(skip_serializing_if = "is_false", rename = "allGroups")]
    all_groups: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "fromAgentId")]
    from_agent_id: Option<String>,
    content: String,
    delivery_mode: InterAgentDeliveryMode,
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
    #[serde(default, alias = "deliveryMode")]
    delivery_mode: Option<InterAgentDeliveryMode>,
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
    delivery_mode: String,
    trigger_turn: bool,
    deduplicated: bool,
    summary: String,
}

pub(crate) struct CutexAgentListHandler;
pub(crate) struct CutexAgentSendHandler;

pub(crate) fn cutex_agent_bus_available() -> bool {
    std::env::var(CUTEX_AGENT_BUS_URL_ENV_VAR)
        .ok()
        .as_deref()
        .is_some_and(cutex_agent_bus_url_is_configured)
}

fn cutex_agent_bus_url_is_configured(value: &str) -> bool {
    !value.trim().is_empty()
}

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

    fn handle(&self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'_> {
        Box::pin(async move {
            let arguments = function_arguments(invocation.payload)?;
            let args: AgentBusListArgs = parse_arguments(&arguments)?;
            let config = load_cutex_agent_bus_config()?;
            let agents = fetch_agents(
                &config,
                args.all_groups,
                args.all_hosts || config.agent_id.is_some(),
            )
            .await?;
            let result = build_agent_list_result(&config, agents);
            Ok(boxed_tool_output(json_tool_output(
                &result,
                CUTEX_AGENT_LIST_TOOL_NAME,
            )))
        })
    }
}

impl CoreToolRuntime for CutexAgentListHandler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

impl ToolExecutor<ToolInvocation> for CutexAgentSendHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(CUTEX_AGENT_SEND_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_cutex_agent_send_tool()
    }

    fn handle(&self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'_> {
        Box::pin(async move {
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
            let agents = fetch_agents(
                &config,
                args.all_groups,
                args.all_hosts || config.agent_id.is_some(),
            )
            .await
            .unwrap_or_default();
            let sender = resolve_sender_name(&config, &agents);
            let delivery_mode = resolve_send_delivery_mode(&args);
            let request = AgentBusSendRequest {
                to: args.to,
                all_groups: args.all_groups,
                from: Some(sender.clone()),
                from_agent_id: config.agent_id.clone(),
                content: args.message,
                delivery_mode,
                trigger_turn: delivery_mode.trigger_turn(),
            };
            let response = post_message(&config, &request).await?;
            let result = build_send_result(&sender, response);
            Ok(boxed_tool_output(json_tool_output(
                &result,
                CUTEX_AGENT_SEND_TOOL_NAME,
            )))
        })
    }
}

impl CoreToolRuntime for CutexAgentSendHandler {
    fn matches_kind(&self, payload: &ToolPayload) -> bool {
        matches!(payload, ToolPayload::Function { .. })
    }
}

fn resolve_send_delivery_mode(args: &AgentBusSendArgs) -> InterAgentDeliveryMode {
    if let Some(delivery_mode) = args.delivery_mode {
        return delivery_mode;
    }
    if args.queue_only {
        InterAgentDeliveryMode::Passive
    } else {
        InterAgentDeliveryMode::AfterTurn
    }
}

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
    if !matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
        return Err(FunctionCallError::RespondToModel(
            "cutex agent bus URL must point to localhost".to_string(),
        ));
    }
    Ok(())
}

async fn fetch_agents(
    config: &CutexAgentBusConfig,
    all_groups: bool,
    all_hosts: bool,
) -> Result<Vec<AgentBusAgent>, FunctionCallError> {
    let path = build_agents_path(config, all_groups, all_hosts);
    let value = http_get_json(config, &path).await?;
    serde_json::from_value::<Vec<AgentBusAgent>>(value).map_err(|err| {
        FunctionCallError::RespondToModel(format!("failed to parse cutex agent list: {err}"))
    })
}

fn build_agents_path(config: &CutexAgentBusConfig, all_groups: bool, all_hosts: bool) -> String {
    let mut query = form_urlencoded::Serializer::new(String::new());
    if let Some(agent_id) = config
        .agent_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        query.append_pair("agent_id", agent_id);
        if all_hosts {
            query.append_pair("allHosts", "true");
        }
    }
    if all_groups {
        query.append_pair("allGroups", "true");
    }
    let query = query.finish();
    if query.is_empty() {
        "/api/agents".to_string()
    } else {
        format!("/api/agents?{query}")
    }
}

async fn post_message(
    config: &CutexAgentBusConfig,
    request: &AgentBusSendRequest,
) -> Result<AgentBusSendResponse, FunctionCallError> {
    let client = build_client()?;
    let mut builder = client
        .post(format!(
            "{}{path}",
            config.base_url,
            path = "/api/messages/send"
        ))
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
    let value = serde_json::from_str::<Value>(&text).map_err(|err| {
        FunctionCallError::RespondToModel(format!(
            "failed to parse cutex agent send JSON response: {err}"
        ))
    })?;
    parse_agent_bus_send_response(value).map_err(|err| {
        FunctionCallError::RespondToModel(format!(
            "failed to parse cutex agent send response: {err}"
        ))
    })
}

fn parse_agent_bus_send_response(value: Value) -> Result<AgentBusSendResponse, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "expected JSON object".to_string())?;
    Ok(AgentBusSendResponse {
        id: required_string_field(obj, "id")?,
        from: optional_string_field(obj, "from"),
        to: required_string_field(obj, "to")?,
        to_name: optional_string_field(obj, "to_name")
            .or_else(|| optional_string_field(obj, "toName")),
        delivery_mode: optional_string_field(obj, "delivery_mode")
            .or_else(|| optional_string_field(obj, "deliveryMode"))
            .and_then(|mode| serde_json::from_value(Value::String(mode)).ok()),
        trigger_turn: optional_bool_field(obj, "trigger_turn")
            .or_else(|| optional_bool_field(obj, "triggerTurn"))
            .ok_or_else(|| "missing boolean field trigger_turn".to_string())?,
        queued: optional_bool_field(obj, "queued")
            .ok_or_else(|| "missing boolean field queued".to_string())?,
        deduplicated: optional_bool_field(obj, "deduplicated").unwrap_or(false),
    })
}

fn required_string_field(
    obj: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, String> {
    optional_string_field(obj, field).ok_or_else(|| format!("missing string field {field}"))
}

fn optional_string_field(obj: &serde_json::Map<String, Value>, field: &str) -> Option<String> {
    obj.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn optional_bool_field(obj: &serde_json::Map<String, Value>, field: &str) -> Option<bool> {
    obj.get(field).and_then(Value::as_bool)
}

async fn http_get_json(
    config: &CutexAgentBusConfig,
    path: &str,
) -> Result<Value, FunctionCallError> {
    let client = build_client()?;
    let mut builder = client.get(format!("{}{path}", config.base_url));
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

#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_false(value: &bool) -> bool {
    !*value
}

fn build_client() -> Result<Client, FunctionCallError> {
    Client::builder()
        .no_proxy()
        .redirect(Policy::none())
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
                session_id: agent.session_id,
                profile: agent.profile,
                cwd: agent.cwd,
                pid: agent.pid,
                groups: agent.groups,
                registration_class: agent.registration_class,
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
    let delivery_mode = response
        .delivery_mode
        .unwrap_or_else(|| InterAgentDeliveryMode::from_legacy_trigger_turn(response.trigger_turn));
    let mode = delivery_mode_label(delivery_mode);
    let summary = format!(
        "Sent message {} from {} to {} ({}) queued={} trigger_turn={} deduplicated={}",
        response.id,
        actual_sender,
        target,
        mode,
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
        delivery_mode: mode.to_string(),
        trigger_turn: response.trigger_turn,
        deduplicated: response.deduplicated,
        summary,
    }
}

fn delivery_mode_label(mode: InterAgentDeliveryMode) -> &'static str {
    match mode {
        InterAgentDeliveryMode::AfterTurn => "after-turn",
        InterAgentDeliveryMode::Soon => "soon",
        InterAgentDeliveryMode::Passive => "passive",
        InterAgentDeliveryMode::Interrupt => "interrupt",
    }
}

fn json_tool_output<T: Serialize>(value: &T, tool_name: &str) -> JsonToolOutput {
    let value = serde_json::to_value(value).unwrap_or_else(|err| {
        serde_json::json!({
            "ok": false,
            "summary": format!("failed to serialize {tool_name} result: {err}")
        })
    });
    JsonToolOutput::new(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_tools::ToolOutput;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::body_json;
    use wiremock::matchers::header;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use wiremock::matchers::query_param;

    fn agent(id: &str, name: &str, base_name: Option<&str>) -> AgentBusAgent {
        AgentBusAgent {
            id: id.to_string(),
            name: name.to_string(),
            base_name: base_name.map(str::to_string),
            path_key: Some("abc1234".to_string()),
            session_id: Some("019e-agent".to_string()),
            profile: "aemeath".to_string(),
            cwd: "/tmp/project".to_string(),
            pid: 123,
            groups: vec!["project:abc1234".to_string()],
            registration_class: Some("local_only".to_string()),
            last_seen_epoch_secs: 1,
        }
    }

    fn config(base_url: String) -> CutexAgentBusConfig {
        CutexAgentBusConfig {
            base_url,
            token: Some("secret-token".to_string()),
            agent_id: Some("cutex.aemeath.worker.123".to_string()),
            fallback_agent_name: Some("aemeath".to_string()),
        }
    }

    fn send_response(mode: Option<InterAgentDeliveryMode>) -> AgentBusSendResponse {
        AgentBusSendResponse {
            id: "message-1".to_string(),
            from: Some("msgbot-1".to_string()),
            to: "agent-2".to_string(),
            to_name: Some("msgbot-2.abc1234".to_string()),
            delivery_mode: mode,
            trigger_turn: true,
            queued: true,
            deduplicated: false,
        }
    }

    #[test]
    fn cutex_agent_sender_name_prefers_live_base_name() {
        let config = config("http://127.0.0.1:24260".to_string());
        let agents = vec![agent(
            "cutex.aemeath.worker.123",
            "msgbot-1.abc1234",
            Some("msgbot-1"),
        )];

        assert_eq!(resolve_sender_name(&config, &agents), "msgbot-1");
    }

    #[test]
    fn cutex_agent_sender_name_falls_back_to_env_name() {
        let config = config("http://127.0.0.1:24260".to_string());

        assert_eq!(resolve_sender_name(&config, &[]), "aemeath");
    }

    #[test]
    fn cutex_agent_bus_availability_requires_a_non_empty_value() {
        assert!(!cutex_agent_bus_url_is_configured(""));
        assert!(!cutex_agent_bus_url_is_configured("  \t"));
        assert!(cutex_agent_bus_url_is_configured("http://127.0.0.1:24260"));
    }

    #[test]
    fn cutex_agent_bus_url_must_be_loopback_http() {
        assert!(validate_local_http_base_url("http://127.0.0.1:24260").is_ok());
        assert!(validate_local_http_base_url("http://localhost:24260").is_ok());
        assert!(validate_local_http_base_url("http://[::1]:24260").is_ok());
        assert!(validate_local_http_base_url("https://127.0.0.1:24260").is_err());
        assert!(validate_local_http_base_url("http://example.com:24260").is_err());
    }

    #[test]
    fn cutex_agent_list_path_keeps_and_encodes_requester_scope() {
        let mut config = config("http://127.0.0.1:24260".to_string());
        config.agent_id = Some("cutex agent&owner".to_string());

        assert_eq!(
            build_agents_path(&config, true, true),
            "/api/agents?agent_id=cutex+agent%26owner&allHosts=true&allGroups=true"
        );
    }

    #[test]
    fn cutex_agent_send_result_is_a_structured_receipt() {
        let result = build_send_result(
            "msgbot-1",
            send_response(Some(InterAgentDeliveryMode::AfterTurn)),
        );
        let output = json_tool_output(&result, CUTEX_AGENT_SEND_TOOL_NAME);
        let payload = ToolPayload::Function {
            arguments: "{}".to_string(),
        };
        let hook_value = output
            .post_tool_use_response("call-1", &payload)
            .expect("structured hook receipt should be present");

        assert_eq!(hook_value["ok"], true);
        assert_eq!(hook_value["message_id"], "message-1");
        assert_eq!(hook_value["from"], "msgbot-1");
        assert_eq!(hook_value["to_name"], "msgbot-2.abc1234");
        assert_eq!(hook_value["queued"], true);
        assert_eq!(hook_value["delivery_mode"], "after-turn");
        assert_eq!(hook_value["deduplicated"], false);
        assert_eq!(output.code_mode_result(&payload), hook_value);
    }

    #[test]
    fn cutex_agent_send_args_default_to_after_turn() {
        let args: AgentBusSendArgs = serde_json::from_value(serde_json::json!({
            "to": "worker",
            "message": "please report"
        }))
        .expect("send args should parse");

        assert_eq!(
            resolve_send_delivery_mode(&args),
            InterAgentDeliveryMode::AfterTurn
        );
    }

    #[test]
    fn cutex_agent_send_args_keep_queue_only_compatibility() {
        let args: AgentBusSendArgs = serde_json::from_value(serde_json::json!({
            "to": "worker",
            "message": "low-priority note",
            "queue_only": true
        }))
        .expect("send args should parse");

        assert_eq!(
            resolve_send_delivery_mode(&args),
            InterAgentDeliveryMode::Passive
        );
    }

    #[test]
    fn cutex_agent_send_args_ignore_legacy_trigger_turn() {
        let args: AgentBusSendArgs = serde_json::from_value(serde_json::json!({
            "to": "worker",
            "message": "status that should still wake",
            "trigger_turn": false
        }))
        .expect("send args should parse");

        assert_eq!(
            resolve_send_delivery_mode(&args),
            InterAgentDeliveryMode::AfterTurn
        );
    }

    #[test]
    fn cutex_agent_send_args_accept_all_explicit_delivery_modes() {
        for (value, expected) in [
            ("after_turn", InterAgentDeliveryMode::AfterTurn),
            ("soon", InterAgentDeliveryMode::Soon),
            ("passive", InterAgentDeliveryMode::Passive),
            ("interrupt", InterAgentDeliveryMode::Interrupt),
        ] {
            let args: AgentBusSendArgs = serde_json::from_value(serde_json::json!({
                "to": "worker",
                "message": "mode-specific status",
                "delivery_mode": value
            }))
            .expect("send args should parse");

            assert_eq!(resolve_send_delivery_mode(&args), expected);
            assert_eq!(expected.trigger_turn(), value != "passive");
        }
    }

    #[test]
    fn cutex_agent_send_request_uses_the_bus_wire_format() {
        let request = AgentBusSendRequest {
            to: "worker".to_string(),
            all_groups: true,
            from: Some("leader".to_string()),
            from_agent_id: Some("agent-leader".to_string()),
            content: "please report".to_string(),
            delivery_mode: InterAgentDeliveryMode::AfterTurn,
            trigger_turn: true,
        };

        let value = serde_json::to_value(&request).expect("request should encode");
        assert_eq!(value["allGroups"], true);
        assert_eq!(value["fromAgentId"], "agent-leader");
        assert_eq!(value["delivery_mode"], "after_turn");
        assert_eq!(value["trigger_turn"], true);
        assert!(value.get("triggerTurn").is_none());
    }

    #[test]
    fn cutex_agent_send_response_accepts_camel_case_compatibility() {
        let response: AgentBusSendResponse = serde_json::from_value(serde_json::json!({
            "id": "message-1",
            "from": "leader",
            "to": "agent-2",
            "toName": "worker.abc1234",
            "deliveryMode": "soon",
            "triggerTurn": true,
            "queued": true,
            "deduplicated": false
        }))
        .expect("camelCase response should parse");

        assert_eq!(response.to_name.as_deref(), Some("worker.abc1234"));
        assert_eq!(response.delivery_mode, Some(InterAgentDeliveryMode::Soon));
        assert!(response.trigger_turn);
    }

    #[test]
    fn cutex_agent_send_parser_tolerates_duplicate_legacy_case_fields() {
        let response = parse_agent_bus_send_response(serde_json::json!({
            "id": "message-1",
            "from": "leader",
            "to": "agent-2",
            "to_name": "worker.abc1234",
            "toName": "worker.abc1234",
            "delivery_mode": "passive",
            "deliveryMode": "passive",
            "trigger_turn": false,
            "triggerTurn": false,
            "queued": true,
            "deduplicated": true
        }))
        .expect("mixed-case response should parse");

        assert_eq!(response.to_name.as_deref(), Some("worker.abc1234"));
        assert_eq!(
            response.delivery_mode,
            Some(InterAgentDeliveryMode::Passive)
        );
        assert!(!response.trigger_turn);
        assert!(response.queued);
        assert!(response.deduplicated);
    }

    #[tokio::test]
    async fn cutex_agent_list_http_preserves_scope_and_bearer_auth() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/agents"))
            .and(query_param("agent_id", "cutex.aemeath.worker.123"))
            .and(query_param("allHosts", "true"))
            .and(query_param("allGroups", "true"))
            .and(header("authorization", "Bearer secret-token"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": "cutex.aemeath.worker.123",
                    "name": "msgbot-1.abc1234",
                    "base_name": "msgbot-1",
                    "path_key": "abc1234",
                    "session_id": "019e-agent",
                    "profile": "aemeath",
                    "cwd": "/tmp/project",
                    "pid": 123,
                    "groups": ["project:abc1234"],
                    "registration_class": "local_only",
                    "last_seen_epoch_secs": 1
                }])),
            )
            .expect(1)
            .mount(&server)
            .await;
        let config = config(server.uri());

        let agents = fetch_agents(&config, true, true)
            .await
            .expect("agent list request should succeed");
        let result = build_agent_list_result(&config, agents);
        let value = serde_json::to_value(result).expect("list result should serialize");

        assert_eq!(value["current_agent_id"], "cutex.aemeath.worker.123");
        assert_eq!(value["agents"][0]["base_name"], "msgbot-1");
        assert_eq!(value["agents"][0]["this"], true);
    }

    #[tokio::test]
    async fn cutex_agent_send_http_preserves_request_and_receipt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/messages/send"))
            .and(header("authorization", "Bearer secret-token"))
            .and(body_json(serde_json::json!({
                "to": "worker",
                "allGroups": true,
                "from": "leader",
                "fromAgentId": "cutex.aemeath.worker.123",
                "content": "please report",
                "delivery_mode": "after_turn",
                "trigger_turn": true
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "message-http-1",
                "from": "leader",
                "to": "cutex.aemeath.worker.456",
                "toName": "worker.abc1234",
                "deliveryMode": "after_turn",
                "triggerTurn": true,
                "queued": true,
                "deduplicated": true
            })))
            .expect(1)
            .mount(&server)
            .await;
        let config = config(server.uri());
        let request = AgentBusSendRequest {
            to: "worker".to_string(),
            all_groups: true,
            from: Some("leader".to_string()),
            from_agent_id: config.agent_id.clone(),
            content: "please report".to_string(),
            delivery_mode: InterAgentDeliveryMode::AfterTurn,
            trigger_turn: true,
        };

        let response = post_message(&config, &request)
            .await
            .expect("agent send request should succeed");
        let result = build_send_result("leader", response);
        let value = serde_json::to_value(result).expect("receipt should serialize");

        assert_eq!(value["message_id"], "message-http-1");
        assert_eq!(value["to"], "cutex.aemeath.worker.456");
        assert_eq!(value["to_name"], "worker.abc1234");
        assert_eq!(value["delivery_mode"], "after-turn");
        assert_eq!(value["trigger_turn"], true);
        assert_eq!(value["queued"], true);
        assert_eq!(value["deduplicated"], true);
    }

    #[tokio::test]
    async fn cutex_agent_http_client_does_not_follow_redirects() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/messages/send"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", "http://example.com/leak"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let config = config(server.uri());
        let request = AgentBusSendRequest {
            to: "worker".to_string(),
            all_groups: false,
            from: Some("leader".to_string()),
            from_agent_id: config.agent_id.clone(),
            content: "please report".to_string(),
            delivery_mode: InterAgentDeliveryMode::AfterTurn,
            trigger_turn: true,
        };

        let err = post_message(&config, &request)
            .await
            .expect_err("redirect must remain an HTTP error");
        assert!(format!("{err:?}").contains("302 Found"));
    }
}
