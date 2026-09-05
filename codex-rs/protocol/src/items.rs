use crate::AgentPath;
use crate::ResponseItemId;
use crate::ThreadId;
use crate::dynamic_tools::DynamicToolCallOutputContentItem;
use crate::mcp::CallToolResult;
use crate::memory_citation::MemoryCitation;
use crate::models::ContentItem;
use crate::models::FunctionCallOutputBody;
use crate::models::ImageDetail;
use crate::models::MessagePhase;
use crate::models::ResponseItem;
use crate::models::WebSearchAction;
use crate::openai_models::ReasoningEffort as ReasoningEffortConfig;
use crate::parse_command::ParsedCommand;
use crate::protocol::AgentStatus;
use crate::protocol::CollabAgentRef;
use crate::protocol::ExecCommandSource;
use crate::protocol::ExecCommandStatus;
use crate::protocol::FileChange;
use crate::protocol::InterAgentDeliveryMode;
use crate::protocol::PatchApplyStatus;
use crate::protocol::ReviewOutputEvent;
use crate::protocol::ReviewTarget;
use crate::protocol::SubAgentActivityKind;
use crate::user_input::ByteRange;
use crate::user_input::TextElement;
use crate::user_input::UserInput;
use codex_extension_items::ExtensionItem;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path_uri::PathUri;
use quick_xml::de::from_str as from_xml_str;
use quick_xml::se::to_string as to_xml_string;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use ts_rs::TS;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
#[serde(tag = "type")]
#[ts(tag = "type")]
pub enum TurnItem {
    UserMessage(UserMessageItem),
    FunctionCallOutput(FunctionCallOutputItem),
    HookPrompt(HookPromptItem),
    AgentMessage(AgentMessageItem),
    InterAgentMessage(InterAgentMessageItem),
    ManagedAgentActivity(ManagedAgentActivityItem),
    OutboundInterAgentMessage(OutboundInterAgentMessageItem),
    TaskAssignmentActivity(TaskAssignmentActivityItem),
    TaskWatchdogActivity(TaskWatchdogActivityItem),
    Plan(PlanItem),
    Reasoning(ReasoningItem),
    CommandExecution(CommandExecutionItem),
    DynamicToolCall(DynamicToolCallItem),
    CollabAgentToolCall(CollabAgentToolCallItem),
    SubAgentActivity(SubAgentActivityItem),
    /// Hosted Responses API web-search item handled directly by core.
    ///
    /// Standalone web search uses Self::Extension instead because its display
    /// schema is owned by the web-search extension.
    WebSearch(WebSearchItem),
    ImageView(ImageViewItem),
    /// Item whose schema and lifecycle details are owned by an extension.
    ///
    /// Standalone image generation, sleep, and web search use this path.
    /// App-server wraps the same typed items in their public variants.
    Extension(ExtensionItem),
    /// Hosted Responses API image-generation item handled directly by core.
    ///
    /// This remains separate from [`Self::Extension`] because core still owns
    /// hosted image persistence and legacy-event fanout.
    ImageGeneration(ImageGenerationItem),
    EnteredReviewMode(EnteredReviewModeItem),
    ExitedReviewMode(ExitedReviewModeItem),
    FileChange(FileChangeItem),
    McpToolCall(McpToolCallItem),
    ContextCompaction(ContextCompactionItem),
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
pub struct UserMessageItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_id: Option<String>,
    pub content: Vec<UserInput>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
pub struct FunctionCallOutputItem {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub namespace: Option<String>,
    pub output: FunctionCallOutputBody,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
pub struct HookPromptItem {
    pub id: String,
    pub fragments: Vec<HookPromptFragment>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct HookPromptFragment {
    pub text: String,
    pub hook_run_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename = "hook_prompt")]
struct HookPromptXml {
    #[serde(rename = "@hook_run_id")]
    hook_run_id: String,
    #[serde(rename = "$text")]
    text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
#[serde(tag = "type")]
#[ts(tag = "type")]
pub enum AgentMessageContent {
    Text { text: String },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub enum AgentMessageDelivery {
    Async,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct AsyncUserInputQuestion {
    pub title: String,
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
/// Assistant-authored message payload used in turn-item streams.
///
/// `phase` is optional because not all providers/models emit it. Consumers
/// should use it when present, but retain legacy completion semantics when it
/// is `None`.
pub struct AgentMessageItem {
    pub id: String,
    pub content: Vec<AgentMessageContent>,
    /// Optional phase metadata carried through from `ResponseItem::Message`.
    ///
    /// This is currently used by TUI rendering to distinguish mid-turn
    /// commentary from a final answer and avoid status-indicator jitter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phase: Option<MessagePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub memory_citation: Option<MemoryCitation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub delivery: Option<AgentMessageDelivery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub questions: Option<Vec<AsyncUserInputQuestion>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct InterAgentMessageItem {
    pub id: String,
    pub author: AgentPath,
    pub recipient: AgentPath,
    #[serde(default)]
    pub other_recipients: Vec<AgentPath>,
    pub content: String,
    pub delivery_mode: InterAgentDeliveryMode,
    /// Authenticated Cutex presentation hints. Live-only: canonical identity and content remain
    /// authoritative, and resume is permitted to omit these hints.
    #[serde(skip)]
    #[ts(skip)]
    #[schemars(skip)]
    pub author_metadata: Option<CutexParticipantPresentation>,
    #[serde(skip)]
    #[ts(skip)]
    #[schemars(skip)]
    pub recipient_metadata: Option<CutexParticipantPresentation>,
    /// Authenticated semantic Task Service classification for live TUI presentation only.
    #[serde(skip)]
    #[ts(skip)]
    #[schemars(skip)]
    pub task_service_presentation: Option<TaskServiceMessagePresentation>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = "v2/")]
pub enum TaskServiceMessageClass {
    Assignment,
    Progress,
    Blocked,
    Resumed,
    ReviewReady,
    Retry,
    TerminalClosure,
}

/// Bounded semantic fields already authenticated at Task Service ingress. Mechanical provider
/// identifiers are deliberately absent, and this value is never serialized into rollout state.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub struct TaskServiceMessagePresentation {
    pub class: TaskServiceMessageClass,
    pub task_name: String,
    pub assignment_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub transition: Option<String>,
    pub semantic_payload: String,
}

/// Optional presentation-only attributes for a Cutex participant.
///
/// Every field is an authenticated hint supplied by Cutex. Consumers must not infer missing
/// values from Agent paths, runtime IDs, message bodies, groups, working directories, or prose.
#[derive(Debug, Clone, Default, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub struct CutexParticipantPresentation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cutex_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub runtime_backend: Option<String>,
}

/// UI-only lifecycle for a durable Cutex-managed agent.
///
/// These identifiers deliberately remain opaque strings. They are not native Codex `ThreadId`s
/// or `AgentPath`s and must never be used to establish native sub-agent ownership.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub struct ManagedAgentActivityItem {
    /// Stable action identity used to update one visible activity.
    pub id: String,
    /// Unique source-event identity used for idempotent ingestion.
    pub event_id: String,
    /// Monotonic sequence for this action.
    pub sequence: u64,
    /// Source event time, in Unix milliseconds.
    pub occurred_at_ms: i64,
    /// Authoritative Cutex project identity for frontend-local presentation correlation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_id: Option<String>,
    pub operation: ManagedAgentOperation,
    pub status: ManagedAgentActivityStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phase_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phase: Option<AgentManagementPhase>,
    pub managed_agent_id: String,
    pub managed_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub managed_agent_metadata: Option<CutexParticipantPresentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub predecessor_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub predecessor_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub predecessor_metadata: Option<CutexParticipantPresentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub successor_agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub successor_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub successor_metadata: Option<CutexParticipantPresentation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replace_policy: Option<AgentManagementReplacePolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rotation_mode: Option<AgentManagementRotationMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub authority_epoch: Option<u64>,
    pub managed_agent_role: Option<String>,
    pub initial_task_preview: Option<String>,
    pub detail: Option<String>,
    pub runtime_generation: Option<u64>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub enum ManagedAgentOperation {
    Create,
    QueryManaged,
    Online,
    Offline,
    Restart,
    Replace,
    Close,
    DirectorRotate,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub enum ManagedAgentActivityStatus {
    InProgress,
    Completed,
    Failed,
}

/// Authoritative committed Agent Management phase projected by Cutex for live presentation.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub enum AgentManagementPhase {
    Prepared,
    PrivateCwdReady,
    NativeBootstrapPending,
    NativeSessionCaptured,
    Adopted,
    Configured,
    Online,
    Ready,
    MessagePending,
    MessageQueued,
    PredecessorClosing,
    PredecessorClosed,
    AuthorityTransferPending,
    AuthorityTransferred,
    SuccessorReady,
    Complete,
    NoWrite,
    OwnerActionRequired,
    Failure,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub enum AgentManagementReplacePolicy {
    CloseBeforeCreate,
    CloseAfterReady,
    KeepOld,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub enum AgentManagementRotationMode {
    ClosePredecessorThenCreateWithMessage,
    RetainPredecessorWithMessage,
    RetainPredecessorBootstrapOnly,
}

/// UI-only record of a message sent through the Cutex Agent Bus.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub struct OutboundInterAgentMessageItem {
    /// Stable message identity used to update one visible activity.
    pub id: String,
    /// Unique source-event identity used for idempotent ingestion.
    pub event_id: String,
    /// Monotonic sequence for this message.
    pub sequence: u64,
    /// Source event time, in Unix milliseconds.
    pub occurred_at_ms: i64,
    pub sender_agent_id: String,
    pub sender_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sender_metadata: Option<CutexParticipantPresentation>,
    pub recipient_agent_id: String,
    pub recipient_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recipient_metadata: Option<CutexParticipantPresentation>,
    #[serde(default)]
    pub other_recipient_agent_ids: Vec<String>,
    pub delivery_mode: InterAgentDeliveryMode,
    pub status: OutboundInterAgentMessageStatus,
    /// Bounded display-only preview. The full function-call argument remains the model-visible
    /// source of truth and is never reconstructed from this field.
    pub content_preview: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub enum OutboundInterAgentMessageStatus {
    Sending,
    Sent,
    Failed,
}

/// Typed UI reducer input for Cutex Task Service assignment and attempt transitions.
///
/// A future task panel must still requery the authoritative Task Service snapshot; these watch
/// events are durable timeline records and sequence-bearing invalidation hints, not authority.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub struct TaskAssignmentActivityItem {
    /// Stable assignment identity used to update one visible activity.
    pub id: String,
    /// Unique source-event identity used for idempotent ingestion.
    pub event_id: String,
    /// Monotonic Task Service sequence for the assignment.
    pub sequence: u64,
    /// Source event time, in Unix milliseconds.
    pub occurred_at_ms: i64,
    pub task_id: String,
    pub task_title: Option<String>,
    pub director_agent_id: String,
    pub director_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub director_metadata: Option<CutexParticipantPresentation>,
    pub assignee_agent_id: String,
    pub assignee_agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignee_metadata: Option<CutexParticipantPresentation>,
    pub status: TaskAssignmentActivityStatus,
    pub attempt_id: Option<String>,
    pub attempt_number: Option<u32>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub enum TaskAssignmentActivityStatus {
    Committed,
    CommunicationRecorded,
    AttemptStarted,
    AttemptAcknowledged,
    AttemptProgressed,
    AttemptBlocked,
    AttemptResumed,
    RetryScheduled,
    ReviewReady,
    Completed,
    Failed,
    Closed,
    Declined,
    Aborted,
}

/// Raw presentation-only metadata for one stale-running Task Service episode.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", export_to = "v2/")]
pub struct TaskWatchdogActivityItem {
    /// Stable episode identity shared by every stage of one stale interval.
    pub id: String,
    pub event_id: String,
    pub event_key: String,
    pub sequence: u64,
    pub occurred_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub project_id: Option<String>,
    pub task_id: String,
    pub task_revision: u64,
    pub assignment_id: String,
    pub attempt_number: u64,
    pub director_agent_id: String,
    pub assignee_agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignee_metadata: Option<CutexParticipantPresentation>,
    pub activity_watermark: String,
    pub activity_kind: TaskWatchdogActivityKind,
    pub idle_duration_secs: u64,
    pub stage: TaskWatchdogStage,
    pub source_sequence: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = "v2/")]
pub enum TaskWatchdogActivityKind {
    PhaseTransition,
    StatusProgress,
    LastOutput,
    LastToolCall,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = "v2/")]
pub enum TaskWatchdogStage {
    FirstStale,
    DirectorEscalated,
}

impl TaskWatchdogStage {
    pub fn event_key(self) -> &'static str {
        match self {
            Self::FirstStale => "task_watchdog.first_stale",
            Self::DirectorEscalated => "task_watchdog.director_escalated",
        }
    }
}

/// Stable timeline identifier used only to transport live Cutex UI activity that has no native
/// turn correlation. Native turn identifiers are UUIDs, so this value cannot collide with one.
pub const CUTEX_UI_ONLY_ACTIVITY_LANE_ID: &str = "cutex-ui-only-activity-lane";

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case", export_to = "v2/")]
pub enum CutexUiActivityDeliveryClass {
    Live,
    CatchUp,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
#[ts(export_to = "v2/")]
pub enum CutexUiActivityDeliverySchema {
    #[serde(rename = "cutex/ui-activity-delivery/v1")]
    #[ts(rename = "cutex/ui-activity-delivery/v1")]
    V1,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CutexUiActivityCheckpoint {
    pub stream_id: String,
    pub cursor: String,
    pub sequence: u64,
}

/// Required transport facts for one producer-projected Cutex UI activity.
#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
#[ts(export_to = "v2/")]
pub struct CutexUiActivityDelivery {
    pub schema: CutexUiActivityDeliverySchema,
    pub class: CutexUiActivityDeliveryClass,
    pub recovered: bool,
    pub batch_id: String,
    pub batch_index: u32,
    pub batch_size: u32,
    pub source_checkpoint: CutexUiActivityCheckpoint,
    pub batch_checkpoint: CutexUiActivityCheckpoint,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type", rename_all = "camelCase", export_to = "v2/")]
pub enum CutexUiActivity {
    ManagedAgentActivity(Box<ManagedAgentActivityItem>),
    OutboundInterAgentMessage(OutboundInterAgentMessageItem),
    TaskAssignmentActivity(TaskAssignmentActivityItem),
    TaskWatchdogActivity(TaskWatchdogActivityItem),
}

impl CutexUiActivity {
    pub fn event_id(&self) -> &str {
        match self {
            Self::ManagedAgentActivity(item) => &item.event_id,
            Self::OutboundInterAgentMessage(item) => &item.event_id,
            Self::TaskAssignmentActivity(item) => &item.event_id,
            Self::TaskWatchdogActivity(item) => &item.event_id,
        }
    }

    pub fn item_id(&self) -> &str {
        match self {
            Self::ManagedAgentActivity(item) => &item.id,
            Self::OutboundInterAgentMessage(item) => &item.id,
            Self::TaskAssignmentActivity(item) => &item.id,
            Self::TaskWatchdogActivity(item) => &item.id,
        }
    }

    pub fn sequence(&self) -> u64 {
        match self {
            Self::ManagedAgentActivity(item) => item.sequence,
            Self::OutboundInterAgentMessage(item) => item.sequence,
            Self::TaskAssignmentActivity(item) => item.sequence,
            Self::TaskWatchdogActivity(item) => item.sequence,
        }
    }

    pub fn occurred_at_ms(&self) -> i64 {
        match self {
            Self::ManagedAgentActivity(item) => item.occurred_at_ms,
            Self::OutboundInterAgentMessage(item) => item.occurred_at_ms,
            Self::TaskAssignmentActivity(item) => item.occurred_at_ms,
            Self::TaskWatchdogActivity(item) => item.occurred_at_ms,
        }
    }

    pub fn is_in_progress(&self) -> bool {
        match self {
            Self::ManagedAgentActivity(item) => {
                matches!(item.status, ManagedAgentActivityStatus::InProgress)
            }
            Self::OutboundInterAgentMessage(item) => {
                matches!(item.status, OutboundInterAgentMessageStatus::Sending)
            }
            Self::TaskAssignmentActivity(item) => matches!(
                item.status,
                TaskAssignmentActivityStatus::Committed
                    | TaskAssignmentActivityStatus::CommunicationRecorded
                    | TaskAssignmentActivityStatus::AttemptStarted
                    | TaskAssignmentActivityStatus::AttemptAcknowledged
                    | TaskAssignmentActivityStatus::AttemptProgressed
                    | TaskAssignmentActivityStatus::AttemptBlocked
                    | TaskAssignmentActivityStatus::AttemptResumed
                    | TaskAssignmentActivityStatus::RetryScheduled
            ),
            Self::TaskWatchdogActivity(_) => true,
        }
    }

    pub fn into_turn_item(self) -> TurnItem {
        match self {
            Self::ManagedAgentActivity(item) => TurnItem::ManagedAgentActivity(*item),
            Self::OutboundInterAgentMessage(item) => TurnItem::OutboundInterAgentMessage(item),
            Self::TaskAssignmentActivity(item) => TurnItem::TaskAssignmentActivity(item),
            Self::TaskWatchdogActivity(item) => TurnItem::TaskWatchdogActivity(item),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
pub struct EnteredReviewModeItem {
    pub id: String,
    pub target: ReviewTarget,
    pub user_facing_hint: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
pub struct ExitedReviewModeItem {
    pub id: String,
    pub review_output: Option<ReviewOutputEvent>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
pub struct PlanItem {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
pub struct ReasoningItem {
    pub id: String,
    pub summary_text: Vec<String>,
    #[serde(default)]
    pub raw_content: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandExecutionStatus {
    InProgress,
    Completed,
    Failed,
    Declined,
}

impl From<ExecCommandStatus> for CommandExecutionStatus {
    fn from(value: ExecCommandStatus) -> Self {
        match value {
            ExecCommandStatus::Completed => Self::Completed,
            ExecCommandStatus::Failed => Self::Failed,
            ExecCommandStatus::Declined => Self::Declined,
        }
    }
}

/// Returns whether a path is safe to serialize as a trusted plugin-relative path.
///
/// This validates the cross-platform wire shape only. The trusted plugin resolver
/// remains responsible for establishing that the path actually came from a plugin root.
pub fn is_safe_plugin_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path.split('/').all(|component| {
            !component.is_empty()
                && !matches!(component, "." | "..")
                && !matches!(
                    component.as_bytes(),
                    [drive, b':', ..] if drive.is_ascii_alphabetic()
                )
        })
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct CommandExecutionItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub script_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub process_id: Option<String>,
    pub command: Vec<String>,
    pub cwd: PathUri,
    pub parsed_cmd: Vec<ParsedCommand>,
    pub source: ExecCommandSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub interaction_input: Option<String>,
    pub status: CommandExecutionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stderr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub aggregated_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "string", optional)]
    pub duration: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub formatted_output: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DynamicToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct DynamicToolCallItem {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub namespace: Option<String>,
    pub tool: String,
    pub arguments: serde_json::Value,
    pub status: DynamicToolCallStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content_items: Option<Vec<DynamicToolCallOutputContentItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub success: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "string", optional)]
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CollabAgentTool {
    SpawnAgent,
    SendInput,
    ResumeAgent,
    Wait,
    CloseAgent,
    SendMessage,
    FollowupTask,
    InterruptAgent,
    ListAgents,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CollabAgentToolCallStatus {
    InProgress,
    Completed,
    Failed,
    Interrupted,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct CollabAgentToolCallItem {
    pub id: String,
    pub tool: CollabAgentTool,
    pub status: CollabAgentToolCallStatus,
    pub sender_thread_id: ThreadId,
    #[serde(default)]
    pub receiver_thread_ids: Vec<ThreadId>,
    #[serde(default)]
    pub receiver_agents: Vec<CollabAgentRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reasoning_effort: Option<ReasoningEffortConfig>,
    #[serde(default)]
    pub agents_states: HashMap<ThreadId, AgentStatus>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
pub struct SubAgentActivityItem {
    pub id: String,
    pub kind: SubAgentActivityKind,
    pub agent_thread_id: ThreadId,
    pub agent_path: AgentPath,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct WebSearchItem {
    pub id: String,
    pub query: String,
    pub action: WebSearchAction,
    /// Structured search results returned out-of-band by standalone web search.
    ///
    /// These stay as opaque JSON at the Codex transport boundary so new result
    /// fields and result types can pass through without changing model-visible
    /// context or requiring a Codex release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub results: Option<Vec<JsonValue>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct ImageViewItem {
    pub id: String,
    /// Path resolved within the selected execution environment.
    ///
    /// This core protocol type is not exposed directly in the app-server API.
    /// App-server converts the path to `LegacyAppPathString` at its boundary.
    pub path: PathUri,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct ImageGenerationItem {
    pub id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub revised_prompt: Option<String>,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub saved_path: Option<AbsolutePathBuf>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
pub struct FileChangeItem {
    pub id: String,
    pub changes: HashMap<PathBuf, FileChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<PatchApplyStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub auto_approved: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct McpToolCallItem {
    pub id: String,
    pub server: String,
    pub tool: String,
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub connector_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub mcp_app_resource_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub link_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub action_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub plugin_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub read_only_hint: Option<bool>,
    pub status: McpToolCallStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub result: Option<CallToolResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<McpToolCallError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(type = "string", optional)]
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum McpToolCallStatus {
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct McpToolCallError {
    pub message: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, TS, JsonSchema)]
pub struct ContextCompactionItem {
    pub id: String,
}

fn new_item_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

impl ContextCompactionItem {
    pub fn new() -> Self {
        Self { id: new_item_id() }
    }
}

impl Default for ContextCompactionItem {
    fn default() -> Self {
        Self::new()
    }
}

impl UserMessageItem {
    pub fn new(content: &[UserInput]) -> Self {
        Self {
            id: new_item_id(),
            client_id: None,
            content: content.to_vec(),
        }
    }

    pub fn message(&self) -> String {
        self.content
            .iter()
            .map(|c| match c {
                UserInput::Text { text, .. } => text.clone(),
                _ => String::new(),
            })
            .collect::<Vec<String>>()
            .join("")
    }

    pub fn text_elements(&self) -> Vec<TextElement> {
        let mut out = Vec::new();
        let mut offset = 0usize;
        for input in &self.content {
            if let UserInput::Text {
                text,
                text_elements,
                ..
            } = input
            {
                // Text element ranges are relative to each text chunk; offset them so they align
                // with the concatenated message returned by `message()`.
                for elem in text_elements {
                    let byte_range = ByteRange {
                        start: offset + elem.byte_range.start,
                        end: offset + elem.byte_range.end,
                    };
                    out.push(TextElement::new(
                        byte_range,
                        elem.placeholder(text).map(str::to_string),
                    ));
                }
                offset += text.len();
            }
        }
        out
    }

    pub fn image_urls(&self) -> Vec<String> {
        self.content
            .iter()
            .filter_map(|c| match c {
                UserInput::Image { image_url, .. } => Some(image_url.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn image_details(&self) -> Vec<Option<ImageDetail>> {
        trim_trailing_default_image_details(
            self.content
                .iter()
                .filter_map(|c| match c {
                    UserInput::Image { detail, .. } => Some(*detail),
                    _ => None,
                })
                .collect(),
        )
    }

    pub fn local_image_paths(&self) -> Vec<std::path::PathBuf> {
        self.content
            .iter()
            .filter_map(|c| match c {
                UserInput::LocalImage { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn local_image_details(&self) -> Vec<Option<ImageDetail>> {
        trim_trailing_default_image_details(
            self.content
                .iter()
                .filter_map(|c| match c {
                    UserInput::LocalImage { detail, .. } => Some(*detail),
                    _ => None,
                })
                .collect(),
        )
    }

    pub fn audio_urls(&self) -> Vec<String> {
        self.content
            .iter()
            .filter_map(|c| match c {
                UserInput::Audio { audio_url } => Some(audio_url.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn local_audio_paths(&self) -> Vec<std::path::PathBuf> {
        self.content
            .iter()
            .filter_map(|c| match c {
                UserInput::LocalAudio { path } => Some(path.clone()),
                _ => None,
            })
            .collect()
    }
}

fn trim_trailing_default_image_details(
    mut details: Vec<Option<ImageDetail>>,
) -> Vec<Option<ImageDetail>> {
    while matches!(details.last(), Some(None)) {
        details.pop();
    }
    details
}

impl HookPromptItem {
    pub fn from_fragments(id: Option<&str>, fragments: Vec<HookPromptFragment>) -> Self {
        Self {
            id: id.map(str::to_string).unwrap_or_else(new_item_id),
            fragments,
        }
    }
}

impl HookPromptFragment {
    pub fn from_single_hook(text: impl Into<String>, hook_run_id: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            hook_run_id: hook_run_id.into(),
        }
    }
}

pub fn build_hook_prompt_message(fragments: &[HookPromptFragment]) -> Option<ResponseItem> {
    let content = fragments
        .iter()
        .filter(|fragment| !fragment.hook_run_id.trim().is_empty())
        .filter_map(|fragment| {
            serialize_hook_prompt_fragment(&fragment.text, &fragment.hook_run_id)
                .map(|text| ContentItem::InputText { text })
        })
        .collect::<Vec<_>>();

    if content.is_empty() {
        return None;
    }

    Some(ResponseItem::Message {
        id: Some(ResponseItemId::new("msg")),
        role: "user".to_string(),
        content,
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    })
}

pub fn parse_hook_prompt_message(
    id: Option<&str>,
    content: &[ContentItem],
) -> Option<HookPromptItem> {
    let fragments = content
        .iter()
        .map(|content_item| {
            let ContentItem::InputText { text } = content_item else {
                return None;
            };
            parse_hook_prompt_fragment(text)
        })
        .collect::<Option<Vec<_>>>()?;

    if fragments.is_empty() {
        return None;
    }

    Some(HookPromptItem::from_fragments(id, fragments))
}

pub fn parse_hook_prompt_fragment(text: &str) -> Option<HookPromptFragment> {
    let trimmed = text.trim();
    let HookPromptXml { text, hook_run_id } = from_xml_str::<HookPromptXml>(trimmed).ok()?;
    if hook_run_id.trim().is_empty() {
        return None;
    }

    Some(HookPromptFragment { text, hook_run_id })
}

fn serialize_hook_prompt_fragment(text: &str, hook_run_id: &str) -> Option<String> {
    if hook_run_id.trim().is_empty() {
        return None;
    }
    to_xml_string(&HookPromptXml {
        text: text.to_string(),
        hook_run_id: hook_run_id.to_string(),
    })
    .ok()
}

impl TurnItem {
    pub fn id(&self) -> String {
        match self {
            TurnItem::UserMessage(item) => item.id.clone(),
            TurnItem::FunctionCallOutput(item) => item.id.clone(),
            TurnItem::HookPrompt(item) => item.id.clone(),
            TurnItem::AgentMessage(item) => item.id.clone(),
            TurnItem::InterAgentMessage(item) => item.id.clone(),
            TurnItem::ManagedAgentActivity(item) => item.id.clone(),
            TurnItem::OutboundInterAgentMessage(item) => item.id.clone(),
            TurnItem::TaskAssignmentActivity(item) => item.id.clone(),
            TurnItem::TaskWatchdogActivity(item) => item.id.clone(),
            TurnItem::Plan(item) => item.id.clone(),
            TurnItem::Reasoning(item) => item.id.clone(),
            TurnItem::CommandExecution(item) => item.id.clone(),
            TurnItem::DynamicToolCall(item) => item.id.clone(),
            TurnItem::CollabAgentToolCall(item) => item.id.clone(),
            TurnItem::SubAgentActivity(item) => item.id.clone(),
            TurnItem::WebSearch(item) => item.id.clone(),
            TurnItem::ImageView(item) => item.id.clone(),
            TurnItem::Extension(item) => item.id().to_string(),
            TurnItem::ImageGeneration(item) => item.id.clone(),
            TurnItem::EnteredReviewMode(item) => item.id.clone(),
            TurnItem::ExitedReviewMode(item) => item.id.clone(),
            TurnItem::FileChange(item) => item.id.clone(),
            TurnItem::McpToolCall(item) => item.id.clone(),
            TurnItem::ContextCompaction(item) => item.id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_extension_items::sleep::SleepItem;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn sleep_extension_item_preserves_type_and_kind() {
        let item = TurnItem::Extension(ExtensionItem::Sleep(SleepItem {
            id: "sleep-1".to_string(),
            duration_ms: 1_000,
        }));

        assert_eq!(
            serde_json::to_value(item).expect("serialize sleep extension item"),
            json!({
                "type": "Extension",
                "kind": "clock.sleep",
                "id": "sleep-1",
                "durationMs": 1_000,
            })
        );
    }

    #[test]
    fn user_message_item_extracts_audio_attachments() {
        let item = UserMessageItem::new(&[
            UserInput::Text {
                text: "transcribe these".to_string(),
                text_elements: Vec::new(),
            },
            UserInput::Audio {
                audio_url: "https://example.com/remote.mp3".to_string(),
            },
            UserInput::LocalAudio {
                path: std::path::PathBuf::from("local.wav"),
            },
        ]);

        assert_eq!(
            (item.audio_urls(), item.local_audio_paths()),
            (
                vec!["https://example.com/remote.mp3".to_string()],
                vec![std::path::PathBuf::from("local.wav")],
            )
        );
    }

    #[test]
    fn plugin_relative_paths_use_safe_wire_shape() {
        assert!(is_safe_plugin_relative_path("scripts/run.py"));

        for path in [
            "",
            "/home/user/.codex/plugins/cache/sample/scripts/run.py",
            "C:/Users/user/.codex/plugins/cache/sample/scripts/run.py",
            "scripts/C:/run.py",
            r"\\server\share\sample\scripts\run.py",
            r"scripts\run.py",
            "scripts//run.py",
            "scripts/./run.py",
            "scripts/../run.py",
        ] {
            assert!(
                !is_safe_plugin_relative_path(path),
                "unsafe plugin-relative path should be rejected: {path:?}"
            );
        }
    }

    #[test]
    fn hook_prompt_roundtrips_multiple_fragments() {
        let original = vec![
            HookPromptFragment::from_single_hook("Retry with care & joy.", "hook-run-1"),
            HookPromptFragment::from_single_hook("Then summarize cleanly.", "hook-run-2"),
        ];
        let message = build_hook_prompt_message(&original).expect("hook prompt");

        let ResponseItem::Message { id, content, .. } = message else {
            panic!("expected hook prompt message");
        };
        assert!(id.is_some_and(|id| id.starts_with("msg_")));

        let parsed = parse_hook_prompt_message(/*id*/ None, &content).expect("parsed hook prompt");
        assert_eq!(parsed.fragments, original);
    }

    #[test]
    fn hook_prompt_parses_legacy_single_hook_run_id() {
        let parsed = parse_hook_prompt_fragment(
            r#"<hook_prompt hook_run_id="hook-run-1">Retry with tests.</hook_prompt>"#,
        )
        .expect("legacy hook prompt");

        assert_eq!(
            parsed,
            HookPromptFragment {
                text: "Retry with tests.".to_string(),
                hook_run_id: "hook-run-1".to_string(),
            }
        );
    }
}
