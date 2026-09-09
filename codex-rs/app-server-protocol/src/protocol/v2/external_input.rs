use crate::JsonSchema;
use crate::TS;
use codex_protocol::external_input::Envelope;
use codex_protocol::external_input::Message;
use codex_protocol::external_input_record::MessageKey;
use codex_protocol::external_input_status::Status;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct ExternalInputSubmitParams {
    pub version: u32,
    pub owner_id: String,
    pub thread_id: String,
    pub runtime_generation: u64,
    pub message: Message,
    pub semantic_sha256: String,
}

impl From<ExternalInputSubmitParams> for Envelope {
    fn from(value: ExternalInputSubmitParams) -> Self {
        Self {
            version: value.version,
            owner_id: value.owner_id,
            thread_id: value.thread_id,
            runtime_generation: value.runtime_generation,
            message: value.message,
            semantic_sha256: value.semantic_sha256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct ExternalInputStatusParams {
    pub version: u32,
    pub owner_id: String,
    pub thread_id: String,
    pub runtime_generation: u64,
    pub messages: Vec<MessageKey>,
}

/// Submit returns this same shape with exactly one status; status preserves the
/// requested batch's count and order. Only context_persisted carries a receipt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct ExternalInputResponse {
    pub version: u32,
    pub owner_id: String,
    pub thread_id: String,
    pub runtime_generation: u64,
    pub statuses: Vec<Status>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct ExternalInputRetryParams {
    pub version: u32,
    pub owner_id: String,
    pub thread_id: String,
    pub runtime_generation: u64,
    pub message_id: String,
    pub semantic_sha256: String,
    #[schemars(with = "Option<String>")]
    pub expected_attempt_id: Option<Uuid>,
    pub retry_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "v2/")]
pub enum ExternalInputRetryDisposition {
    Released,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct ExternalInputRetryResponse {
    pub version: u32,
    pub owner_id: String,
    pub thread_id: String,
    pub runtime_generation: u64,
    pub message_id: String,
    pub semantic_sha256: String,
    #[schemars(with = "Option<String>")]
    pub expected_attempt_id: Option<Uuid>,
    pub retry_id: String,
    pub disposition: ExternalInputRetryDisposition,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct ExternalInputStatusChangedNotification {
    pub thread_id: String,
    pub message_id: String,
}
