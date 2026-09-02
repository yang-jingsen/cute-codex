use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use sha2::Sha256;
use tokio::process::Command;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::agent_management_spec::AGENT_MANAGEMENT_TOOL_NAME;
use crate::tools::handlers::agent_management_spec::create_agent_management_tool;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_tools::ToolName;
use codex_tools::ToolSpec;

const CONTRACT: &str = "cutex/agent-management/v1";
const RECEIPT_SCHEMA: &str = "cutex/agent-management-receipt/v1";
const FAILURE_SCHEMA: &str = "cutex/agent-management-failure/v1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

pub struct AgentManagementHandler {
    integration: Result<Integration, &'static str>,
}

#[derive(Debug)]
struct Integration {
    executable: PathBuf,
    runtime_agent_id: String,
    timeout: Duration,
}

#[derive(Debug, Deserialize, Serialize)]
struct ToolInput {
    action_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(flatten)]
    operation: Operation,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ManagedAgentSpec {
    name: String,
    cwd: String,
    profile: String,
    runtime_backend: String,
    model: String,
    reasoning: String,
    permissions: String,
    approval_policy: String,
    sandbox_mode: String,
    groups: Vec<String>,
    #[serde(default)]
    expose_to_im: bool,
    #[serde(default)]
    pin: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    Create {
        spec: ManagedAgentSpec,
        start_mode: String,
        #[serde(default)]
        frozen_message: Option<String>,
    },
    QueryManaged,
    Online {
        cutex_session_id: String,
    },
    Offline {
        cutex_session_id: String,
    },
    Restart {
        cutex_session_id: String,
    },
    Close {
        cutex_session_id: String,
    },
    Replace {
        predecessor_cutex_session_id: String,
        policy: String,
        successor: ManagedAgentSpec,
        start_mode: String,
        #[serde(default)]
        frozen_message: Option<String>,
    },
    DirectorRotate {
        expected_predecessor_cutex_session: String,
        expected_authority_epoch: u64,
        mode: String,
        successor: ManagedAgentSpec,
        #[serde(default)]
        frozen_message: Option<String>,
    },
}

impl Operation {
    fn name(&self) -> &'static str {
        match self {
            Self::Create { .. } => "create",
            Self::QueryManaged => "query_managed",
            Self::Online { .. } => "online",
            Self::Offline { .. } => "offline",
            Self::Restart { .. } => "restart",
            Self::Close { .. } => "close",
            Self::Replace { .. } => "replace",
            Self::DirectorRotate { .. } => "director_rotate",
        }
    }

    fn command(&self) -> &'static str {
        match self {
            Self::QueryManaged => "query-managed",
            Self::DirectorRotate { .. } => "director-rotate",
            operation => operation.name(),
        }
    }
}

#[derive(Serialize)]
struct ProviderRequest<'a> {
    schema: &'static str,
    action_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<&'a str>,
    #[serde(flatten)]
    operation: &'a Operation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderResponse {
    schema: String,
    action_id: String,
    outcome: ProviderOutcome,
}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
enum ProviderOutcome {
    Complete { receipt: Value },
    NoWrite { code: String, detail: String },
    OwnerActionRequired { failure: Value },
}

impl AgentManagementHandler {
    pub(crate) fn from_environment() -> Option<Self> {
        let runtime = Some(std::env::var("CUTEX_AGENT_ID").ok()?);
        Some(Self {
            integration: Integration::resolve(OsString::from("cutex"), runtime, REQUEST_TIMEOUT),
        })
    }

    async fn invoke_arguments(&self, arguments: &str) -> String {
        let input = match parse_input(arguments) {
            Ok(input) => input,
            Err(_) => return serialize(no_write("invalid-body", "invalid_arguments")),
        };
        if !valid_identity(&input.action_id) {
            return serialize(no_write("invalid-body", "invalid_arguments"));
        }
        if input
            .project_id
            .as_deref()
            .is_some_and(|project_id| !valid_project(project_id))
        {
            return serialize(no_write(&input.action_id, "invalid_arguments"));
        }
        let integration = match &self.integration {
            Ok(integration) => integration,
            Err(code) => {
                return serialize(no_write(&input.action_id, code));
            }
        };
        let (bytes, digest) = match request_bytes(&input) {
            Ok(prepared) => prepared,
            Err(_) => {
                return serialize(no_write(&input.action_id, "invalid_arguments"));
            }
        };
        let response = match integration.execute(input.operation.command(), &bytes).await {
            Ok(response) => response,
            Err(code) => {
                return serialize(no_write(&input.action_id, code));
            }
        };
        serialize(sanitize_response(&input, &digest, response))
    }
}

fn parse_input(arguments: &str) -> Result<ToolInput, ()> {
    let value: Value = serde_json::from_str(arguments).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    let operation = object.get("operation").and_then(Value::as_str).ok_or(())?;
    let operation_fields: &[&str] = match operation {
        "create" => &["spec", "start_mode", "frozen_message"],
        "query_managed" => &[],
        "online" | "offline" | "restart" | "close" => &["cutex_session_id"],
        "replace" => &[
            "predecessor_cutex_session_id",
            "policy",
            "successor",
            "start_mode",
            "frozen_message",
        ],
        "director_rotate" => &[
            "expected_predecessor_cutex_session",
            "expected_authority_epoch",
            "mode",
            "successor",
            "frozen_message",
        ],
        _ => return Err(()),
    };
    if object.keys().any(|field| {
        !matches!(field.as_str(), "operation" | "action_id" | "project_id")
            && !operation_fields.contains(&field.as_str())
    }) {
        return Err(());
    }
    serde_json::from_value(value).map_err(|_| ())
}

fn request_bytes(input: &ToolInput) -> Result<(Vec<u8>, String), serde_json::Error> {
    let request = ProviderRequest {
        schema: CONTRACT,
        action_id: &input.action_id,
        project_id: input.project_id.as_deref(),
        operation: &input.operation,
    };
    let bytes = serde_json::to_vec(&request)?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    Ok((bytes, digest))
}

impl Integration {
    fn resolve(
        executable: OsString,
        runtime_agent_id: Option<String>,
        timeout: Duration,
    ) -> Result<Self, &'static str> {
        let runtime_agent_id = runtime_agent_id
            .filter(|value| valid_identity(value))
            .ok_or("missing_ambient_identity")?;
        Ok(Self {
            executable: executable.into(),
            runtime_agent_id,
            timeout,
        })
    }

    async fn execute(&self, operation: &str, bytes: &[u8]) -> Result<Value, &'static str> {
        let mut request = private_request_file()?;
        request
            .write_all(bytes)
            .and_then(|_| request.flush())
            .map_err(|_| "provider_unavailable")?;
        let mut command = Command::new(&self.executable);
        command
            .args(["agent", "manage", operation, "--request-file"])
            .arg(request.path())
            .env("CUTEX_AGENT_ID", &self.runtime_agent_id)
            // Groups are collaboration labels and must not cross the provider authority boundary.
            .env_remove("CUTEX_AGENT_GROUPS")
            .kill_on_drop(true);
        let output = tokio::time::timeout(self.timeout, command.output())
            .await
            .map_err(|_| "provider_timeout")?
            .map_err(|_| "provider_unavailable")?;
        if !output.status.success() {
            return Err("provider_rejected");
        }
        if output.stdout.len() > MAX_OUTPUT_BYTES {
            return Err("invalid_provider_response");
        }
        serde_json::from_slice(&output.stdout).map_err(|_| "invalid_provider_response")
    }
}

fn private_request_file() -> Result<tempfile::NamedTempFile, &'static str> {
    let request = tempfile::Builder::new()
        .prefix("cutex-agent-management-")
        .tempfile()
        .map_err(|_| "provider_unavailable")?;
    let metadata = request
        .as_file()
        .metadata()
        .map_err(|_| "provider_unavailable")?;
    if !metadata.file_type().is_file() {
        return Err("provider_unavailable");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        request
            .as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "provider_unavailable")?;
        let mode = request
            .as_file()
            .metadata()
            .map_err(|_| "provider_unavailable")?
            .permissions()
            .mode();
        if mode & 0o777 != 0o600 {
            return Err("provider_unavailable");
        }
    }
    Ok(request)
}

fn sanitize_response(input: &ToolInput, digest: &str, value: Value) -> Value {
    let response: ProviderResponse = match serde_json::from_value::<ProviderResponse>(value.clone())
    {
        Ok(response) if response.schema == CONTRACT && response.action_id == input.action_id => {
            response
        }
        _ => return no_write(&input.action_id, "invalid_provider_response"),
    };
    let valid = match response.outcome {
        ProviderOutcome::Complete { receipt }
            if typed_payload_matches(&receipt, RECEIPT_SCHEMA, input, Some(digest)) =>
        {
            true
        }
        ProviderOutcome::NoWrite { code, detail }
            if bounded_text(&code, 128) && bounded_text(&detail, 4096) =>
        {
            true
        }
        ProviderOutcome::OwnerActionRequired { failure }
            if typed_payload_matches(&failure, FAILURE_SCHEMA, input, None) =>
        {
            true
        }
        _ => false,
    };
    if valid {
        value
    } else {
        no_write(&input.action_id, "invalid_provider_response")
    }
}

fn typed_payload_matches(
    payload: &Value,
    schema: &str,
    input: &ToolInput,
    digest: Option<&str>,
) -> bool {
    let Some(resolved_project) = payload.get("project_id").and_then(Value::as_str) else {
        return false;
    };
    let schema_shape_matches = match schema {
        RECEIPT_SCHEMA => receipt_shape_matches(payload, input),
        FAILURE_SCHEMA => failure_shape_matches(payload),
        _ => false,
    };
    schema_shape_matches
        && payload.get("schema").and_then(Value::as_str) == Some(schema)
        && payload.get("action_id").and_then(Value::as_str) == Some(input.action_id.as_str())
        && valid_project(resolved_project)
        && input
            .project_id
            .as_deref()
            .is_none_or(|selector| selector == resolved_project)
        && project_ids_are_consistent(payload, resolved_project)
        && payload.get("operation").and_then(Value::as_str) == Some(input.operation.name())
        && digest.is_none_or(|digest| {
            payload.get("request_sha256").and_then(Value::as_str) == Some(digest)
        })
}

fn receipt_shape_matches(payload: &Value, input: &ToolInput) -> bool {
    const FIELDS: &[&str] = &[
        "schema",
        "action_id",
        "request_sha256",
        "operation",
        "project_id",
        "completed_at",
        "result",
    ];
    let Some(object) = payload.as_object() else {
        return false;
    };
    if object.len() != FIELDS.len() || !FIELDS.iter().all(|field| object.contains_key(*field)) {
        return false;
    }
    let expected_result = match &input.operation {
        Operation::Create { .. } => "created",
        Operation::QueryManaged => "query_managed",
        Operation::Online { .. }
        | Operation::Offline { .. }
        | Operation::Restart { .. }
        | Operation::Close { .. } => "lifecycle",
        Operation::Replace { .. } => "replaced",
        Operation::DirectorRotate { .. } => "director_rotated",
    };
    object
        .get("completed_at")
        .and_then(Value::as_str)
        .is_some_and(|value| bounded_text(value, 128))
        && object
            .get("result")
            .and_then(Value::as_object)
            .and_then(|result| result.get("kind"))
            .and_then(Value::as_str)
            == Some(expected_result)
}

fn failure_shape_matches(payload: &Value) -> bool {
    const REQUIRED_FIELDS: &[&str] = &[
        "schema",
        "event_id",
        "action_id",
        "project_id",
        "operation",
        "code",
        "detail",
        "routing_status",
        "created_at",
    ];
    const OPTIONAL_FIELDS: &[&str] = &["route_to_director_session", "target_cutex_session_id"];
    let Some(object) = payload.as_object() else {
        return false;
    };
    if !(REQUIRED_FIELDS.len()..=REQUIRED_FIELDS.len() + OPTIONAL_FIELDS.len())
        .contains(&object.len())
        || !REQUIRED_FIELDS
            .iter()
            .all(|field| object.contains_key(*field))
        || object.keys().any(|field| {
            !REQUIRED_FIELDS.contains(&field.as_str()) && !OPTIONAL_FIELDS.contains(&field.as_str())
        })
    {
        return false;
    }
    object
        .get("event_id")
        .and_then(Value::as_str)
        .is_some_and(valid_identity)
        && object
            .get("code")
            .and_then(Value::as_str)
            .is_some_and(|value| bounded_text(value, 128))
        && object
            .get("detail")
            .and_then(Value::as_str)
            .is_some_and(|value| value.len() <= 4096)
        && matches!(
            object.get("routing_status").and_then(Value::as_str),
            Some("routable" | "unrouted")
        )
        && object
            .get("created_at")
            .and_then(Value::as_str)
            .is_some_and(|value| bounded_text(value, 128))
        && OPTIONAL_FIELDS.iter().all(|field| {
            object.get(*field).is_none_or(|value| {
                value.is_null()
                    || value
                        .as_str()
                        .is_some_and(|value| valid_identity(value) && value.len() <= 256)
            })
        })
}

fn project_ids_are_consistent(value: &Value, expected: &str) -> bool {
    match value {
        Value::Object(object) => object.iter().all(|(key, value)| {
            if key == "project_id" {
                value.as_str() == Some(expected)
            } else {
                project_ids_are_consistent(value, expected)
            }
        }),
        Value::Array(values) => values
            .iter()
            .all(|value| project_ids_are_consistent(value, expected)),
        _ => true,
    }
}

fn valid_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b'@')
        })
}

fn valid_project(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b':'))
}

fn bounded_text(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max
}

fn no_write(action_id: &str, code: &str) -> Value {
    serde_json::json!({
        "schema": CONTRACT,
        "action_id": action_id,
        "outcome": {
            "status": "no_write",
            "code": code,
            "detail": "native Agent Management provider did not commit an action"
        }
    })
}

fn serialize(receipt: Value) -> String {
    serde_json::to_string(&receipt).unwrap_or_else(|_| {
        format!(
            r#"{{"schema":"{CONTRACT}","action_id":"invalid-body","outcome":{{"status":"no_write","code":"serialization_failed","detail":"native Agent Management provider did not commit an action"}}}}"#
        )
    })
}

impl ToolExecutor<ToolInvocation> for AgentManagementHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(AGENT_MANAGEMENT_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_agent_management_tool()
    }

    fn handle(&self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'_> {
        Box::pin(async move {
            let ToolPayload::Function { arguments } = invocation.payload else {
                return Err(FunctionCallError::RespondToModel(
                    "cutex_agent_management received an unsupported payload".to_string(),
                ));
            };
            Ok(boxed_tool_output(FunctionToolOutput::from_text(
                self.invoke_arguments(&arguments).await,
                Some(true),
            )))
        })
    }
}

impl CoreToolRuntime for AgentManagementHandler {}

#[cfg(test)]
#[path = "agent_management_tests.rs"]
mod tests;
