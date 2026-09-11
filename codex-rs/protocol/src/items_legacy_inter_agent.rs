//! Read-only historical display data from the K rollout format.
//! These types have no delivery, authority, or model-input conversion methods.
use crate::AgentPath;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase")]
pub struct LegacyInterAgentMessageItem {
    pub id: String,
    pub author: AgentPath,
    pub recipient: AgentPath,
    #[serde(default)]
    pub other_recipients: Vec<AgentPath>,
    pub content: String,
    pub delivery_mode: LegacyInterAgentDeliveryMode,
}

/// Historical label only; in particular `interrupt` never interrupts a resumed thread.
#[derive(Debug, Copy, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum LegacyInterAgentDeliveryMode {
    AfterTurn,
    Soon,
    Passive,
    Interrupt,
}

#[cfg(test)]
#[path = "items_legacy_inter_agent_tests.rs"]
mod tests;
