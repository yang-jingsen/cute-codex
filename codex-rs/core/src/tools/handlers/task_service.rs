#[cfg(test)]
use std::ffi::OsString;
#[cfg(test)]
use std::time::Duration;

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::task_service_spec::TASK_SERVICE_TOOL_NAME;
use crate::tools::handlers::task_service_spec::create_task_service_tool;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_tools::ToolName;
use codex_tools::ToolSpec;

#[path = "task_service_receipt.rs"]
mod receipt;
use receipt::ModelReceipt;
use receipt::model_no_write;
use receipt::sanitize_provider_receipt;
use receipt::sanitize_provider_response;
use receipt::serialize_model_receipt;

#[path = "task_service_protocol.rs"]
mod protocol;
use protocol::PrepareResult;

use super::task_service_transport::TaskServiceTransport;
use super::task_service_transport::TransportError;

const ACTION_SCHEMA: &str = "cutex/task-service-action/v2";
const MAX_SEMANTIC_TEXT_BYTES: usize = 4096;
const MAX_BLOCKER_SUMMARY_BYTES: usize = 2048;

pub struct TaskServiceHandler {
    transport: TaskServiceTransport,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum WorkerOperation {
    Start,
    ReportStatus,
    Block,
    Resume,
    Submit,
    Decline,
    AbortAttempt,
}

impl WorkerOperation {
    fn attempt_required(self) -> bool {
        matches!(
            self,
            Self::ReportStatus | Self::Block | Self::Resume | Self::Submit | Self::AbortAttempt
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskServiceArgs {
    operation: WorkerOperation,
    assignment_id: String,
    action_id: String,
    summary: Option<String>,
    evidence_sha256: Option<String>,
    result_sha256: Option<String>,
    result_reference: Option<String>,
}

impl TaskServiceHandler {
    pub(crate) fn from_environment() -> Option<Self> {
        TaskServiceTransport::from_environment().map(Self::with_transport)
    }

    #[cfg(test)]
    fn new(
        bus_url: Option<OsString>,
        token: Option<OsString>,
        agent_id: Option<OsString>,
        timeout: Duration,
    ) -> Self {
        Self::with_transport(TaskServiceTransport::new(bus_url, token, agent_id, timeout))
    }

    fn with_transport(transport: TaskServiceTransport) -> Self {
        Self { transport }
    }

    async fn invoke_arguments(&self, arguments: &str) -> String {
        let args: TaskServiceArgs = match serde_json::from_str(arguments) {
            Ok(args) => args,
            Err(_) => return serialize_model_receipt(ModelReceipt::no_write("invalid_arguments")),
        };
        let action = match provider_request(&args) {
            Ok(action) => action,
            Err(code) => {
                return serialize_model_receipt(ModelReceipt::no_write(code).for_action(&args));
            }
        };
        let prepared = match self.prepare_action(&args, &action).await {
            Ok(PrepareResult::Prepared(envelope_body)) => envelope_body,
            Ok(PrepareResult::Committed(receipt)) => {
                return serialize_model_receipt(sanitize_provider_receipt(&args, receipt));
            }
            Ok(PrepareResult::NoWrite(code)) => {
                return serialize_model_receipt(model_no_write(code).for_action(&args));
            }
            Ok(PrepareResult::Invalid) => {
                return serialize_model_receipt(
                    ModelReceipt::no_write("invalid_provider_response").for_action(&args),
                );
            }
            Err(receipt) => return serialize_model_receipt(receipt),
        };

        serialize_model_receipt(self.execute_prepared(&args, &action, prepared).await)
    }

    async fn prepare_action(
        &self,
        args: &TaskServiceArgs,
        action: &Value,
    ) -> Result<PrepareResult, ModelReceipt> {
        let request_body = protocol::prepare_request_body(action)
            .map_err(|_| ModelReceipt::no_write("invalid_arguments").for_action(args))?;
        let response = self
            .transport
            .post_prepare(request_body)
            .await
            .map_err(|error| transport_error_receipt(args, error))?;
        Ok(protocol::parse_prepare_response(
            action,
            args.operation.attempt_required(),
            response,
        ))
    }

    async fn execute_prepared(
        &self,
        args: &TaskServiceArgs,
        action: &Value,
        envelope_body: Vec<u8>,
    ) -> ModelReceipt {
        match self.transport.post_action(envelope_body).await {
            Ok(response) if !protocol::is_mechanical_conflict(&args.action_id, &response) => {
                return sanitize_provider_response(args, response);
            }
            Err(error) if !matches!(error, TransportError::ResponseUncertain) => {
                return transport_error_receipt(args, error);
            }
            Ok(_) | Err(TransportError::ResponseUncertain) => {}
            Err(_) => unreachable!(),
        }

        let prepared = match self.prepare_action(args, action).await {
            Ok(prepared) => prepared,
            Err(receipt) => return receipt,
        };
        let envelope_body = match prepared {
            PrepareResult::Prepared(envelope_body) => envelope_body,
            PrepareResult::Committed(receipt) => {
                return sanitize_provider_receipt(args, receipt);
            }
            PrepareResult::NoWrite(code) => return model_no_write(code).for_action(args),
            PrepareResult::Invalid => {
                return ModelReceipt::no_write("invalid_provider_response").for_action(args);
            }
        };
        match self.transport.post_action(envelope_body).await {
            Ok(response) => sanitize_provider_response(args, response),
            Err(error) => transport_error_receipt(args, error),
        }
    }
}

fn transport_error_receipt(args: &TaskServiceArgs, error: TransportError) -> ModelReceipt {
    match error {
        TransportError::Unavailable(code) => ModelReceipt::no_write(code).for_action(args),
        TransportError::ResponseUncertain => ModelReceipt::no_write("response_uncertain")
            .with_status("response_uncertain")
            .for_action(args),
        TransportError::Rejected => ModelReceipt::no_write("integration_rejected").for_action(args),
        TransportError::InvalidResponse => {
            ModelReceipt::no_write("invalid_provider_response").for_action(args)
        }
    }
}

impl ToolExecutor<ToolInvocation> for TaskServiceHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(TASK_SERVICE_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_task_service_tool()
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(async move {
            let ToolPayload::Function { arguments } = invocation.payload else {
                return Err(FunctionCallError::RespondToModel(
                    "cutex_task_service received an unsupported payload".to_string(),
                ));
            };
            Ok(boxed_tool_output(FunctionToolOutput::from_text(
                self.invoke_arguments(&arguments).await,
                Some(true),
            )))
        })
    }
}

impl CoreToolRuntime for TaskServiceHandler {}

fn provider_request(args: &TaskServiceArgs) -> Result<Value, &'static str> {
    validate_id(&args.assignment_id)?;
    validate_id(&args.action_id)?;
    let mut body = serde_json::Map::from_iter([
        ("schema".to_string(), json!(ACTION_SCHEMA)),
        ("action_id".to_string(), json!(args.action_id)),
        ("assignment_id".to_string(), json!(args.assignment_id)),
    ]);
    match args.operation {
        WorkerOperation::ReportStatus => {
            if args.result_sha256.is_some() || args.result_reference.is_some() {
                return Err("invalid_semantic_payload");
            }
            let summary = args
                .summary
                .as_deref()
                .filter(|value| !value.trim().is_empty() && value.len() <= MAX_SEMANTIC_TEXT_BYTES)
                .ok_or("invalid_semantic_payload")?;
            body.insert("summary".to_string(), json!(summary));
            if let Some(evidence_sha256) = args.evidence_sha256.as_deref() {
                validate_sha256(evidence_sha256)?;
                body.insert("evidence_sha256".to_string(), json!(evidence_sha256));
            }
        }
        WorkerOperation::Submit => {
            if args.summary.is_some() || args.evidence_sha256.is_some() {
                return Err("invalid_semantic_payload");
            }
            let result_sha256 = args
                .result_sha256
                .as_deref()
                .ok_or("invalid_semantic_payload")?;
            validate_sha256(result_sha256)?;
            let result_reference = args
                .result_reference
                .as_deref()
                .filter(|value| !value.trim().is_empty() && value.len() <= MAX_SEMANTIC_TEXT_BYTES)
                .ok_or("invalid_semantic_payload")?;
            body.insert("result_sha256".to_string(), json!(result_sha256));
            body.insert("result_reference".to_string(), json!(result_reference));
        }
        WorkerOperation::Block => {
            if args.evidence_sha256.is_some()
                || args.result_sha256.is_some()
                || args.result_reference.is_some()
            {
                return Err("invalid_semantic_payload");
            }
            let summary = args
                .summary
                .as_deref()
                .filter(|value| {
                    !value.trim().is_empty() && value.len() <= MAX_BLOCKER_SUMMARY_BYTES
                })
                .ok_or("invalid_semantic_payload")?;
            body.insert("summary".to_string(), json!(summary));
        }
        WorkerOperation::Start
        | WorkerOperation::Resume
        | WorkerOperation::Decline
        | WorkerOperation::AbortAttempt => {
            if args.summary.is_some()
                || args.evidence_sha256.is_some()
                || args.result_sha256.is_some()
                || args.result_reference.is_some()
            {
                return Err("invalid_semantic_payload");
            }
        }
    }
    Ok(json!({ "operation": args.operation, "body": body }))
}

fn validate_id(value: &str) -> Result<(), &'static str> {
    if value.is_empty()
        || value.len() > 256
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b'@')
        })
    {
        return Err("invalid_identity");
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<(), &'static str> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("invalid_semantic_payload");
    }
    Ok(())
}

#[cfg(test)]
#[path = "task_service_tests.rs"]
mod tests;
