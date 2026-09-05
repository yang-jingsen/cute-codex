//! Types used to define loaded and effective Codex configuration values.

// Note this file should generally be restricted to simple struct/enum
// definitions that do not contain business logic.

pub use crate::mcp_types::AppToolApproval;
pub use crate::mcp_types::McpServerAuth;
pub use crate::mcp_types::McpServerConfig;
pub use crate::mcp_types::McpServerDisabledReason;
pub use crate::mcp_types::McpServerEnvVar;
pub use crate::mcp_types::McpServerOAuthConfig;
pub use crate::mcp_types::McpServerToolConfig;
pub use crate::mcp_types::McpServerTransportConfig;
pub use crate::mcp_types::RawMcpServerConfig;
pub use crate::shell_environment_policy::ShellEnvironmentPolicyToml;
pub use codex_protocol::config_types::AltScreenMode;
pub use codex_protocol::config_types::ApprovalsReviewer;
pub use codex_protocol::config_types::ModeKind;
pub use codex_protocol::config_types::Personality;
pub use codex_protocol::config_types::ServiceTier;
pub use codex_protocol::config_types::WebSearchMode;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

pub use crate::tui_keymap::KeybindingSpec;
pub use crate::tui_keymap::KeybindingsSpec;
pub use crate::tui_keymap::MAX_FUNCTION_KEY;
pub use crate::tui_keymap::TuiAgentsKeymap;
pub use crate::tui_keymap::TuiApprovalKeymap;
pub use crate::tui_keymap::TuiChatKeymap;
pub use crate::tui_keymap::TuiComposerKeymap;
pub use crate::tui_keymap::TuiEditorKeymap;
pub use crate::tui_keymap::TuiGlobalKeymap;
pub use crate::tui_keymap::TuiKeymap;
pub use crate::tui_keymap::TuiListKeymap;
pub use crate::tui_keymap::TuiPagerKeymap;
pub use crate::tui_keymap::TuiVimNormalKeymap;
pub use crate::tui_keymap::TuiVimOperatorKeymap;
pub use crate::tui_keymap::TuiVimSearchKeymap;

pub const DEFAULT_OTEL_ENVIRONMENT: &str = "dev";
pub const DEFAULT_MEMORIES_MAX_ROLLOUTS_PER_STARTUP: usize = 2;
pub const DEFAULT_MEMORIES_MAX_ROLLOUT_AGE_DAYS: i64 = 10;
pub const DEFAULT_MEMORIES_MIN_ROLLOUT_IDLE_HOURS: i64 = 6;
pub const DEFAULT_MEMORIES_MIN_RATE_LIMIT_REMAINING_PERCENT: i64 = 25;
pub const DEFAULT_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION: usize = 256;
pub const DEFAULT_MEMORIES_MAX_UNUSED_DAYS: i64 = 30;
const MIN_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION: usize = 1;
const MAX_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION: usize = 4096;
const MIN_MEMORIES_MAX_ROLLOUTS_PER_STARTUP: usize = 1;
const MAX_MEMORIES_MAX_ROLLOUTS_PER_STARTUP: usize = 128;

const fn default_enabled() -> bool {
    true
}

/// Preferred layout for the resume/fork session picker.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SessionPickerViewMode {
    Comfortable,
    #[default]
    Dense,
}

impl SessionPickerViewMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Comfortable => "comfortable",
            Self::Dense => "dense",
        }
    }
}

impl fmt::Display for SessionPickerViewMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Provider filter scope for resume/fork session lookup.
#[derive(Serialize, Deserialize, Debug, Default, Copy, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SessionPickerProviderFilter {
    /// Only show sessions created with the current model provider.
    #[default]
    Current,
    /// Show sessions across all model providers that share the same CODEX_HOME.
    All,
}

impl SessionPickerProviderFilter {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::All => "all",
        }
    }
}

impl fmt::Display for SessionPickerProviderFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Working directory to use when resuming or forking a session.
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ResumeCwdMode {
    /// Use the directory where Codex was launched.
    Current,
    /// Use the latest working directory recorded in the selected session.
    Session,
}

impl ResumeCwdMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Session => "session",
        }
    }
}

/// Determine where Codex should store CLI auth credentials.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AuthCredentialsStoreMode {
    #[default]
    /// Persist credentials in CODEX_HOME/auth.json.
    File,
    /// Persist credentials in the keyring. Fail if unavailable.
    Keyring,
    /// Use keyring when available; otherwise, fall back to a file in CODEX_HOME.
    Auto,
    /// Store credentials in memory only for the current process.
    Ephemeral,
}

/// Determine where Codex should store and read MCP credentials.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum OAuthCredentialsStoreMode {
    /// Prefer `Keyring` and use `File` when keyring storage is unavailable.
    /// Once an MCP client loads credentials from one store, that client keeps the resolved store
    /// for its lifetime so refreshes cannot switch to a possibly stale credential source.
    /// Credentials stored in the keyring will only be readable by Codex unless the user explicitly grants access via OS-level keyring access.
    #[default]
    Auto,
    /// CODEX_HOME/.credentials.json
    /// This file will be readable to Codex and other applications running as the same user.
    File,
    /// Keyring when available, otherwise fail.
    Keyring,
}

/// Determine how auth credentials should use keyring-backed storage.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AuthKeyringBackendKind {
    /// Store the serialized auth payload directly in the OS keyring.
    Direct,
    /// Store auth payloads in the local encrypted secrets file, with the file key in the OS keyring.
    Secrets,
}

impl Default for AuthKeyringBackendKind {
    fn default() -> Self {
        if cfg!(windows) {
            Self::Secrets
        } else {
            Self::Direct
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WindowsSandboxModeToml {
    Elevated,
    Unelevated,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct WindowsToml {
    pub sandbox: Option<WindowsSandboxModeToml>,
    /// Defaults to `true`. Set to `false` to launch the final sandboxed child
    /// process on `Winsta0\\Default` instead of a private desktop.
    pub sandbox_private_desktop: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, JsonSchema)]
pub enum UriBasedFileOpener {
    #[serde(rename = "vscode")]
    VsCode,

    #[serde(rename = "vscode-insiders")]
    VsCodeInsiders,

    #[serde(rename = "windsurf")]
    Windsurf,

    #[serde(rename = "cursor")]
    Cursor,

    /// Option to disable the URI-based file opener.
    #[serde(rename = "none")]
    None,
}

/// Settings that govern if and what will be written to `~/.codex/history.jsonl`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[serde(default)]
#[schemars(deny_unknown_fields)]
pub struct History {
    /// If true, history entries will not be written to disk.
    pub persistence: HistoryPersistence,

    /// If set, the maximum size of the history file in bytes. The oldest entries
    /// are dropped once the file exceeds this limit.
    pub max_bytes: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Default, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryPersistence {
    /// Save all history entries to disk.
    #[default]
    SaveAll,
    /// Do not write history to disk.
    None,
}

// ===== Analytics configuration =====

/// Analytics settings loaded from config.toml. Fields are optional so we can apply defaults.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AnalyticsConfigToml {
    /// When `false`, disables analytics across Codex product surfaces in this profile.
    pub enabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct FeedbackConfigToml {
    /// When `false`, disables the feedback flow across Codex product surfaces.
    pub enabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolSuggestDiscoverableType {
    Connector,
    Plugin,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ToolSuggestDiscoverable {
    #[serde(rename = "type")]
    pub kind: ToolSuggestDiscoverableType,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ToolSuggestDisabledTool {
    #[serde(rename = "type")]
    pub kind: ToolSuggestDiscoverableType,
    pub id: String,
}

impl ToolSuggestDisabledTool {
    pub fn plugin(id: impl Into<String>) -> Self {
        Self {
            kind: ToolSuggestDiscoverableType::Plugin,
            id: id.into(),
        }
    }

    pub fn connector(id: impl Into<String>) -> Self {
        Self {
            kind: ToolSuggestDiscoverableType::Connector,
            id: id.into(),
        }
    }

    pub fn normalized(&self) -> Option<Self> {
        let id = self.id.trim();
        (!id.is_empty()).then(|| Self {
            kind: self.kind,
            id: id.to_string(),
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ToolSuggestConfig {
    #[serde(default)]
    pub discoverables: Vec<ToolSuggestDiscoverable>,
    #[serde(default)]
    pub disabled_tools: Vec<ToolSuggestDisabledTool>,
}

/// Memories settings loaded from config.toml.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct MemoriesToml {
    /// When `true`, external context sources mark the thread `memory_mode` as `"polluted"`.
    #[serde(alias = "no_memories_if_mcp_or_web_search")]
    pub disable_on_external_context: Option<bool>,
    /// When `false`, newly created threads are stored with `memory_mode = "disabled"` in the state DB.
    pub generate_memories: Option<bool>,
    /// When `false`, skip injecting memory usage instructions into developer prompts.
    pub use_memories: Option<bool>,
    /// When `true`, expose dedicated memory tools through the extension tool surface.
    pub dedicated_tools: Option<bool>,
    /// Maximum number of recent raw memories retained for global consolidation.
    #[schemars(range(min = 1, max = 4096))]
    pub max_raw_memories_for_consolidation: Option<usize>,
    /// Maximum number of days since a memory was last used before it becomes ineligible for phase 2 selection.
    pub max_unused_days: Option<i64>,
    /// Maximum age of the threads used for memories.
    pub max_rollout_age_days: Option<i64>,
    /// Maximum number of rollout candidates processed per pass.
    #[schemars(range(min = 1, max = 128))]
    pub max_rollouts_per_startup: Option<usize>,
    /// Minimum idle time between last thread activity and memory creation (hours). > 12h recommended.
    pub min_rollout_idle_hours: Option<i64>,
    /// Minimum remaining percentage required in Codex rate-limit windows before memory startup runs.
    #[schemars(range(min = 0, max = 100))]
    pub min_rate_limit_remaining_percent: Option<i64>,
    /// Model used for thread summarisation.
    pub extract_model: Option<String>,
    /// Model used for memory consolidation.
    pub consolidation_model: Option<String>,
}

/// Effective memories settings after defaults are applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoriesConfig {
    pub disable_on_external_context: bool,
    pub generate_memories: bool,
    pub use_memories: bool,
    pub dedicated_tools: bool,
    pub max_raw_memories_for_consolidation: usize,
    pub max_unused_days: i64,
    pub max_rollout_age_days: i64,
    pub max_rollouts_per_startup: usize,
    pub min_rollout_idle_hours: i64,
    pub min_rate_limit_remaining_percent: i64,
    pub extract_model: Option<String>,
    pub consolidation_model: Option<String>,
}

impl Default for MemoriesConfig {
    fn default() -> Self {
        Self {
            disable_on_external_context: false,
            generate_memories: true,
            use_memories: true,
            dedicated_tools: false,
            max_raw_memories_for_consolidation: DEFAULT_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION,
            max_unused_days: DEFAULT_MEMORIES_MAX_UNUSED_DAYS,
            max_rollout_age_days: DEFAULT_MEMORIES_MAX_ROLLOUT_AGE_DAYS,
            max_rollouts_per_startup: DEFAULT_MEMORIES_MAX_ROLLOUTS_PER_STARTUP,
            min_rollout_idle_hours: DEFAULT_MEMORIES_MIN_ROLLOUT_IDLE_HOURS,
            min_rate_limit_remaining_percent: DEFAULT_MEMORIES_MIN_RATE_LIMIT_REMAINING_PERCENT,
            extract_model: None,
            consolidation_model: None,
        }
    }
}

impl From<MemoriesToml> for MemoriesConfig {
    fn from(toml: MemoriesToml) -> Self {
        let defaults = Self::default();
        Self {
            disable_on_external_context: toml
                .disable_on_external_context
                .unwrap_or(defaults.disable_on_external_context),
            generate_memories: toml.generate_memories.unwrap_or(defaults.generate_memories),
            use_memories: toml.use_memories.unwrap_or(defaults.use_memories),
            dedicated_tools: toml.dedicated_tools.unwrap_or(defaults.dedicated_tools),
            max_raw_memories_for_consolidation: toml
                .max_raw_memories_for_consolidation
                .unwrap_or(defaults.max_raw_memories_for_consolidation)
                .clamp(
                    MIN_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION,
                    MAX_MEMORIES_MAX_RAW_MEMORIES_FOR_CONSOLIDATION,
                ),
            max_unused_days: toml
                .max_unused_days
                .unwrap_or(defaults.max_unused_days)
                .clamp(0, 365),
            max_rollout_age_days: toml
                .max_rollout_age_days
                .unwrap_or(defaults.max_rollout_age_days)
                .clamp(0, 90),
            max_rollouts_per_startup: toml
                .max_rollouts_per_startup
                .unwrap_or(defaults.max_rollouts_per_startup)
                .clamp(
                    MIN_MEMORIES_MAX_ROLLOUTS_PER_STARTUP,
                    MAX_MEMORIES_MAX_ROLLOUTS_PER_STARTUP,
                ),
            min_rollout_idle_hours: toml
                .min_rollout_idle_hours
                .unwrap_or(defaults.min_rollout_idle_hours)
                .clamp(1, 48),
            min_rate_limit_remaining_percent: toml
                .min_rate_limit_remaining_percent
                .unwrap_or(defaults.min_rate_limit_remaining_percent)
                .clamp(0, 100),
            extract_model: toml.extract_model,
            consolidation_model: toml.consolidation_model,
        }
    }
}

/// Default settings that apply to all apps.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AppsDefaultConfig {
    /// When `false`, apps are disabled unless overridden by per-app settings.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Reviewer for approval prompts unless overridden by per-app settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approvals_reviewer: Option<ApprovalsReviewer>,

    /// Whether tools with `destructive_hint = true` are allowed by default.
    #[serde(
        default = "default_enabled",
        skip_serializing_if = "std::clone::Clone::clone"
    )]
    pub destructive_enabled: bool,

    /// Whether tools with `open_world_hint = true` are allowed by default.
    #[serde(
        default = "default_enabled",
        skip_serializing_if = "std::clone::Clone::clone"
    )]
    pub open_world_enabled: bool,

    /// Approval mode for tools unless overridden by per-app or per-tool settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tools_approval_mode: Option<AppToolApproval>,
}

/// Per-tool settings for a single app tool.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AppToolConfig {
    /// Whether this tool is enabled. `Some(true)` explicitly allows this tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Approval mode for this tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_mode: Option<AppToolApproval>,
}

/// Tool settings for a single app.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AppToolsConfig {
    /// Per-tool overrides keyed by tool name (for example `repos/list`).
    #[serde(default, flatten)]
    pub tools: HashMap<String, AppToolConfig>,
}

/// Approval settings for a connected account within an app.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AppLinkConfig {
    /// Reviewer for approval prompts from this account, overriding the app default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approvals_reviewer: Option<ApprovalsReviewer>,

    /// Approval mode for this account unless a tool override exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tools_approval_mode: Option<AppToolApproval>,
}

/// Account settings for a single app.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AppLinksConfig {
    /// Per-account approval settings keyed by link ID.
    #[serde(default, flatten)]
    pub links: HashMap<String, AppLinkConfig>,
}

/// Config values for a single app/connector.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AppConfig {
    /// When `false`, Codex does not surface this app.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Reviewer for approval prompts from this app, overriding the thread default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approvals_reviewer: Option<ApprovalsReviewer>,

    /// Whether tools with `destructive_hint = true` are allowed for this app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destructive_enabled: Option<bool>,

    /// Whether tools with `open_world_hint = true` are allowed for this app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_world_enabled: Option<bool>,

    /// Approval mode for tools in this app unless a tool override exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tools_approval_mode: Option<AppToolApproval>,

    /// Whether tools are enabled by default for this app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tools_enabled: Option<bool>,

    /// Per-tool settings for this app.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<AppToolsConfig>,

    /// Per-account approval settings keyed by link ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub links: Option<AppLinksConfig>,
}

/// App/connector settings loaded from `config.toml`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct AppsConfigToml {
    /// Default settings for all apps.
    #[serde(default, rename = "_default", skip_serializing_if = "Option::is_none")]
    pub default: Option<AppsDefaultConfig>,

    /// Per-app settings keyed by app ID (for example `[apps.google_drive]`).
    #[serde(default, flatten)]
    pub apps: HashMap<String, AppConfig>,
}

// ===== OTEL configuration =====

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OtelHttpProtocol {
    /// Binary payload
    Binary,
    /// JSON payload
    Json,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
#[serde(rename_all = "kebab-case")]
pub struct OtelTlsConfig {
    pub ca_certificate: Option<AbsolutePathBuf>,
    pub client_certificate: Option<AbsolutePathBuf>,
    pub client_private_key: Option<AbsolutePathBuf>,
}

/// Which OTEL exporter to use.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema)]
#[schemars(deny_unknown_fields)]
#[serde(rename_all = "kebab-case")]
pub enum OtelExporterKind {
    None,
    Statsig,
    OtlpHttp {
        endpoint: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        protocol: OtelHttpProtocol,
        #[serde(default)]
        tls: Option<OtelTlsConfig>,
    },
    OtlpGrpc {
        endpoint: String,
        #[serde(default)]
        headers: HashMap<String, String>,
        #[serde(default)]
        tls: Option<OtelTlsConfig>,
    },
}

/// OTEL settings loaded from config.toml. Fields are optional so we can apply defaults.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct OtelConfigToml {
    /// Byte limit for tool-result log output; independent of model-visible output.
    #[serde(default)]
    pub tool_result: codex_protocol::config_types::ToolResultLogConfig,
    /// Log user prompt in traces
    pub log_user_prompt: Option<bool>,

    /// Mark traces with environment (dev, staging, prod, test). Defaults to dev.
    pub environment: Option<String>,

    /// Optional log exporter
    pub exporter: Option<OtelExporterKind>,

    /// Optional trace exporter
    pub trace_exporter: Option<OtelExporterKind>,

    /// Optional metrics exporter
    pub metrics_exporter: Option<OtelExporterKind>,

    /// Attributes to add to every exported trace span.
    pub span_attributes: Option<BTreeMap<String, String>>,

    /// Semicolon-separated `key:value` fields to upsert into W3C tracestate members.
    pub tracestate: Option<BTreeMap<String, BTreeMap<String, String>>>,
}

/// Effective OTEL settings after defaults are applied.
#[derive(Debug, Clone, PartialEq)]
pub struct OtelConfig {
    pub tool_result: codex_protocol::config_types::ToolResultLogConfig,
    pub log_user_prompt: bool,
    pub environment: String,
    pub exporter: OtelExporterKind,
    pub trace_exporter: OtelExporterKind,
    pub metrics_exporter: OtelExporterKind,
    pub span_attributes: BTreeMap<String, String>,
    pub tracestate: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for OtelConfig {
    fn default() -> Self {
        OtelConfig {
            tool_result: Default::default(),
            log_user_prompt: false,
            environment: DEFAULT_OTEL_ENVIRONMENT.to_owned(),
            exporter: OtelExporterKind::None,
            trace_exporter: OtelExporterKind::None,
            metrics_exporter: OtelExporterKind::Statsig,
            span_attributes: BTreeMap::new(),
            tracestate: BTreeMap::new(),
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Notifications {
    Enabled(bool),
    Custom(Vec<String>),
}

impl Default for Notifications {
    fn default() -> Self {
        Self::Enabled(true)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum NotificationMethod {
    #[default]
    Auto,
    Osc9,
    Bel,
}

impl fmt::Display for NotificationMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotificationMethod::Auto => write!(f, "auto"),
            NotificationMethod::Osc9 => write!(f, "osc9"),
            NotificationMethod::Bel => write!(f, "bel"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "lowercase")]
pub enum NotificationCondition {
    /// Emit TUI notifications only while the terminal is unfocused.
    #[default]
    Unfocused,
    /// Emit TUI notifications regardless of terminal focus.
    Always,
}

impl fmt::Display for NotificationCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotificationCondition::Unfocused => write!(f, "unfocused"),
            NotificationCondition::Always => write!(f, "always"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TuiPetAnchor {
    /// Anchor the pet to the bottom of the current TUI composer viewport.
    #[default]
    Composer,
    /// Anchor the pet to the physical bottom of the terminal screen.
    ScreenBottom,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct TuiNotificationSettings {
    /// Enable desktop notifications from the TUI.
    /// Defaults to `true`.
    #[serde(default, rename = "notifications")]
    pub notifications: Notifications,

    /// Notification method to use for terminal notifications.
    /// Defaults to `auto`.
    #[serde(default, rename = "notification_method")]
    pub method: NotificationMethod,

    /// Controls whether TUI notifications are delivered only when the terminal is unfocused or
    /// regardless of focus. Defaults to `unfocused`.
    #[serde(default, rename = "notification_condition")]
    pub condition: NotificationCondition,
}

/// Lifecycle events that may be sent to the optional external HTTP notifier.
///
/// This service is intentionally separate from the terminal notification
/// settings above: it carries structured session metadata to a configured
/// endpoint and is disabled by default unless a URL is supplied.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotifyServiceEvent {
    TaskCompleted,
    ThinkingTooLong,
    WaitingApproval,
    ConnectionError,
    SessionExit,
    SessionStarted,
    SessionStartupIdle,
    UserMessageSent,
    UserMessageDispatched,
    TurnStarted,
    TurnCompleted,
    TurnInterrupted,
    TurnFailed,
    ApprovalRequested,
    ApprovalResolved,
    ThreadClosed,
    ContextCompacted,
    RateLimitWarning,
    RateLimitPromptShown,
    CommandExecutionStarted,
    CommandExecutionCompleted,
    PatchApplyStarted,
    PatchApplyCompleted,
    McpToolCallStarted,
    McpToolCallCompleted,
    WebSearchStarted,
    WebSearchCompleted,
    ImageGenerationStarted,
    ImageGenerationCompleted,
    ReviewStarted,
    ReviewCompleted,
    HookStarted,
    HookCompleted,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotifyServiceUserMessageContent {
    #[default]
    None,
    Preview,
    Full,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
pub struct NotifyServiceSettings {
    /// HTTP endpoint for structured lifecycle notifications. Empty or unset disables it.
    #[serde(default)]
    pub notify_service_url: Option<String>,

    /// Optional bearer token for the notification endpoint.
    #[serde(default)]
    pub notify_service_token: Option<String>,

    /// Seconds of idle after a completed turn before `task_completed` fires.
    #[serde(default = "default_idle_timeout_secs")]
    pub notify_service_idle_timeout_secs: u64,

    /// Seconds of unchanged composer draft before `thinking_too_long` fires.
    #[serde(default = "default_composer_idle_timeout_secs")]
    pub notify_service_composer_idle_timeout_secs: u64,

    /// Seconds to wait before notifying about an unresolved approval.
    #[serde(default = "default_approval_timeout_secs")]
    pub notify_service_approval_timeout_secs: u64,

    /// Seconds to wait after a new session is configured without a first message.
    #[serde(default = "default_startup_idle_timeout_secs")]
    pub notify_service_startup_idle_timeout_secs: u64,

    /// Agent name included in payloads.
    #[serde(default = "default_notify_agent_name")]
    pub notify_service_agent_name: String,

    /// Minimum total tokens required before `session_exit` is emitted.
    #[serde(default = "default_notify_min_tokens")]
    pub notify_service_min_tokens: i64,

    /// Allowlist of lifecycle events. Defaults preserve the original idle/exit behavior.
    #[serde(default = "default_notify_events")]
    pub notify_service_events: Vec<NotifyServiceEvent>,

    /// Amount of user message text included in `user_message_sent` payloads.
    #[serde(default)]
    pub notify_service_user_message_content: NotifyServiceUserMessageContent,

    /// Maximum Unicode scalar count for a `preview` user message payload.
    #[serde(default = "default_notify_user_message_preview_chars")]
    pub notify_service_user_message_preview_chars: usize,
}

fn default_idle_timeout_secs() -> u64 {
    60
}

fn default_composer_idle_timeout_secs() -> u64 {
    600
}

fn default_approval_timeout_secs() -> u64 {
    30
}

fn default_startup_idle_timeout_secs() -> u64 {
    180
}

fn default_notify_agent_name() -> String {
    "codex".to_string()
}

fn default_notify_min_tokens() -> i64 {
    100
}

fn default_notify_events() -> Vec<NotifyServiceEvent> {
    vec![
        NotifyServiceEvent::TaskCompleted,
        NotifyServiceEvent::ThinkingTooLong,
        NotifyServiceEvent::WaitingApproval,
        NotifyServiceEvent::SessionExit,
    ]
}

fn default_notify_user_message_preview_chars() -> usize {
    200
}

impl Default for NotifyServiceSettings {
    fn default() -> Self {
        Self {
            notify_service_url: None,
            notify_service_token: None,
            notify_service_idle_timeout_secs: default_idle_timeout_secs(),
            notify_service_composer_idle_timeout_secs: default_composer_idle_timeout_secs(),
            notify_service_approval_timeout_secs: default_approval_timeout_secs(),
            notify_service_startup_idle_timeout_secs: default_startup_idle_timeout_secs(),
            notify_service_agent_name: default_notify_agent_name(),
            notify_service_min_tokens: default_notify_min_tokens(),
            notify_service_events: default_notify_events(),
            notify_service_user_message_content: NotifyServiceUserMessageContent::default(),
            notify_service_user_message_preview_chars: default_notify_user_message_preview_chars(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelAvailabilityNuxConfig {
    /// Number of times a startup availability NUX has been shown per model slug.
    #[serde(default, flatten)]
    pub shown_count: HashMap<String, u32>,
}

/// Fallback resize-reflow row cap when Codex cannot identify a terminal-specific scrollback size.
pub const DEFAULT_TERMINAL_RESIZE_REFLOW_FALLBACK_MAX_ROWS: usize = 1_000;

/// Presentation-only labels for managed-Agent activity in the TUI.
///
/// Values are bounded, allowlisted templates at render time. Invalid values fall back to the
/// built-in English wording. Supported placeholders are `{agent}`, `{operation}`, and `{status}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct TuiManagedAgentActivityLabels {
    #[serde(default)]
    pub in_progress: Option<String>,
    #[serde(default)]
    pub completed: Option<String>,
    #[serde(default)]
    pub failed: Option<String>,
}

/// Presentation-only labels for outbound Agent Bus activity in the TUI.
/// Supported placeholders are `{sender}`, `{recipient}`, and `{status}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct TuiOutboundMessageActivityLabels {
    #[serde(default)]
    pub sending: Option<String>,
    #[serde(default)]
    pub sent: Option<String>,
    #[serde(default)]
    pub failed: Option<String>,
}

/// Presentation for one authoritative Task assignment status.
///
/// Supported placeholders are `{director}`, `{assignee}`, `{task}`, `{assignment}`, `{status}`,
/// `{sequence}`, `{title}`, and `{detail}`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiTaskAssignmentStatusPresentation {
    #[serde(default)]
    pub show: Option<bool>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub rich_template: Option<Vec<TuiCutexRichSpan>>,
    #[serde(default)]
    pub style: TuiCutexTextStyle,
    #[serde(default)]
    pub show_detail: Option<bool>,
    #[serde(default)]
    pub content_indent: Option<u8>,
}

/// Per-status presentation for authoritative Task assignment activity.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiTaskAssignmentActivitySettings {
    #[serde(default)]
    pub committed: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub communication_recorded: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub attempt_started: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub attempt_acknowledged: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub attempt_progressed: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub attempt_blocked: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub attempt_resumed: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub retry_scheduled: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub review_ready: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub completed: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub failed: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub closed: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub declined: Box<TuiTaskAssignmentStatusPresentation>,
    #[serde(default)]
    pub aborted: Box<TuiTaskAssignmentStatusPresentation>,
}

/// Foreground colors accepted by Cutex presentation configuration.
///
/// Named values retain their existing wire spelling. RGB serializes as strict #RRGGBB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiCutexForegroundColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
    White,
    Rgb(u8, u8, u8),
}

impl TuiCutexForegroundColor {
    fn parse(value: &str) -> Result<Self, String> {
        let named = match value {
            "black" => Some(Self::Black),
            "red" => Some(Self::Red),
            "green" => Some(Self::Green),
            "yellow" => Some(Self::Yellow),
            "blue" => Some(Self::Blue),
            "magenta" => Some(Self::Magenta),
            "cyan" => Some(Self::Cyan),
            "gray" => Some(Self::Gray),
            "white" => Some(Self::White),
            _ => None,
        };
        if let Some(named) = named {
            return Ok(named);
        }
        if value.len() != 7
            || !value.starts_with('#')
            || !value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err("expected a named color or strict #RRGGBB truecolor".to_string());
        }
        let component = |range: std::ops::Range<usize>| {
            u8::from_str_radix(&value[range], 16)
                .map_err(|_| "expected a named color or strict #RRGGBB truecolor".to_string())
        };
        Ok(Self::Rgb(
            component(1..3)?,
            component(3..5)?,
            component(5..7)?,
        ))
    }
}

impl Serialize for TuiCutexForegroundColor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Black => serializer.serialize_str("black"),
            Self::Red => serializer.serialize_str("red"),
            Self::Green => serializer.serialize_str("green"),
            Self::Yellow => serializer.serialize_str("yellow"),
            Self::Blue => serializer.serialize_str("blue"),
            Self::Magenta => serializer.serialize_str("magenta"),
            Self::Cyan => serializer.serialize_str("cyan"),
            Self::Gray => serializer.serialize_str("gray"),
            Self::White => serializer.serialize_str("white"),
            Self::Rgb(red, green, blue) => {
                serializer.serialize_str(&format!("#{red:02X}{green:02X}{blue:02X}"))
            }
        }
    }
}

impl<'de> Deserialize<'de> for TuiCutexForegroundColor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for TuiCutexForegroundColor {
    fn schema_name() -> String {
        "TuiCutexForegroundColor".to_string()
    }

    fn json_schema(_generator: &mut schemars::r#gen::SchemaGenerator) -> schemars::schema::Schema {
        use schemars::schema::InstanceType;
        use schemars::schema::Schema;
        use schemars::schema::SchemaObject;
        use schemars::schema::StringValidation;
        use schemars::schema::SubschemaValidation;

        let named = Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            enum_values: Some(
                [
                    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "gray", "white",
                ]
                .into_iter()
                .map(|value| serde_json::Value::String(value.to_string()))
                .collect(),
            ),
            ..Default::default()
        });
        let rgb = Schema::Object(SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            string: Some(Box::new(StringValidation {
                max_length: Some(7),
                min_length: Some(7),
                pattern: Some("^#[0-9A-Fa-f]{6}$".to_string()),
            })),
            ..Default::default()
        });
        Schema::Object(SchemaObject {
            subschemas: Some(Box::new(SubschemaValidation {
                one_of: Some(vec![named, rgb]),
                ..Default::default()
            })),
            ..Default::default()
        })
    }
}

/// Presentation-only style overrides. `None` retains the renderer's safe status default.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiCutexTextStyle {
    #[serde(default)]
    pub foreground: Option<TuiCutexForegroundColor>,
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub dim: Option<bool>,
    #[serde(default)]
    pub italic: Option<bool>,
}

/// One ordered span in a bounded mixed-style Cutex presentation template.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiCutexRichSpan {
    pub text: String,
    #[serde(default)]
    pub foreground: Option<TuiCutexForegroundColor>,
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub dim: Option<bool>,
    #[serde(default)]
    pub italic: Option<bool>,
}

/// Whether committed Agent Management phases advance one card or append live-only lines.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TuiAgentManagementPhaseDisplay {
    #[default]
    Coalesce,
    Append,
}

/// Deterministic frontend-local selection for an ordered template candidate list.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TuiCutexTemplateSelection {
    StableRandom,
}

/// One named candidate pool used to keep correlated live presentation in the same group.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiCutexGroupedTemplates {
    pub group: String,
    pub templates: Vec<String>,
}

/// Presentation-only settings for one authoritative Agent Management phase.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiAgentManagementPhasePresentation {
    #[serde(default)]
    pub show: Option<bool>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub templates: Option<Vec<String>>,
    #[serde(default)]
    pub grouped_templates: Option<Vec<TuiCutexGroupedTemplates>>,
    #[serde(default)]
    pub rich_template: Option<Vec<TuiCutexRichSpan>>,
    #[serde(default)]
    pub selection: Option<TuiCutexTemplateSelection>,
    #[serde(default)]
    pub foreground: Option<TuiCutexForegroundColor>,
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub dim: Option<bool>,
    #[serde(default)]
    pub italic: Option<bool>,
    #[serde(default)]
    pub indent: Option<u8>,
    #[serde(default)]
    pub wrap_width: Option<u16>,
}

/// Per-operation overrides for every committed phase in the frozen Cutex wire contract.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiAgentManagementOperationPresentation {
    #[serde(default)]
    pub phase_display: Option<TuiAgentManagementPhaseDisplay>,
    #[serde(default)]
    pub prepared: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub private_cwd_ready: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub native_bootstrap_pending: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub native_session_captured: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub adopted: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub configured: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub online: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub ready: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub message_pending: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub message_queued: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub predecessor_closing: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub predecessor_closed: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub authority_transfer_pending: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub authority_transferred: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub successor_ready: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub complete: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub no_write: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub owner_action_required: Option<TuiAgentManagementPhasePresentation>,
    #[serde(default)]
    pub failure: Option<TuiAgentManagementPhasePresentation>,
}

/// Tagged Cutex inbound-message presentation. Native parent/subagent messages never consult it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiCutexInboundMessageSettings {
    /// Bounded template placeholders: `{author}`, `{recipient}`, `{mode}`, `{profile}`, `{model}`,
    /// `{reasoning}`, `{role}`, and `{runtime_backend}`. Newlines deliberately separate label
    /// lines; other control characters are rejected by the renderer.
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_cutex_inbound_style")]
    pub style: TuiCutexTextStyle,
    #[serde(default = "default_true")]
    pub show_metadata: bool,
    #[serde(default = "default_true")]
    pub show_ids: bool,
    #[serde(default = "default_cutex_content_indent")]
    pub content_indent: u8,
}

/// Presentation for one authenticated Task Service semantic message class.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiTaskServiceMessagePresentation {
    #[serde(default)]
    pub show: Option<bool>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub templates: Option<Vec<String>>,
    #[serde(default)]
    pub grouped_templates: Option<Vec<TuiCutexGroupedTemplates>>,
    #[serde(default)]
    pub rich_template: Option<Vec<TuiCutexRichSpan>>,
    #[serde(default)]
    pub selection: Option<TuiCutexTemplateSelection>,
    #[serde(default)]
    pub style: TuiCutexTextStyle,
    #[serde(default)]
    pub show_payload: Option<bool>,
    #[serde(default)]
    pub content_indent: Option<u8>,
}

/// Presentation for one raw Task watchdog stage. It never changes Task Service state.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiTaskWatchdogStagePresentation {
    #[serde(default)]
    pub show: Option<bool>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub templates: Option<Vec<String>>,
    #[serde(default)]
    pub grouped_templates: Option<Vec<TuiCutexGroupedTemplates>>,
    #[serde(default)]
    pub rich_template: Option<Vec<TuiCutexRichSpan>>,
    #[serde(default)]
    pub selection: Option<TuiCutexTemplateSelection>,
    #[serde(default)]
    pub style: TuiCutexTextStyle,
    #[serde(default)]
    pub content_indent: Option<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiTaskWatchdogSettings {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub first_stale: TuiTaskWatchdogStagePresentation,
    #[serde(default)]
    pub director_escalated: TuiTaskWatchdogStagePresentation,
}

impl Default for TuiTaskWatchdogSettings {
    fn default() -> Self {
        Self {
            visible: true,
            first_stale: TuiTaskWatchdogStagePresentation::default(),
            director_escalated: TuiTaskWatchdogStagePresentation::default(),
        }
    }
}

/// Dedicated live presentation for authenticated Task Service system messages.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiTaskServiceMessageSettings {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub assignment: TuiTaskServiceMessagePresentation,
    #[serde(default)]
    pub progress: TuiTaskServiceMessagePresentation,
    #[serde(default)]
    pub blocked: TuiTaskServiceMessagePresentation,
    #[serde(default)]
    pub resumed: TuiTaskServiceMessagePresentation,
    #[serde(default)]
    pub review_ready: TuiTaskServiceMessagePresentation,
    #[serde(default)]
    pub retry: TuiTaskServiceMessagePresentation,
    #[serde(default = "default_hidden_task_service_terminal_closure")]
    pub terminal_closure: TuiTaskServiceMessagePresentation,
}

fn default_hidden_task_service_terminal_closure() -> TuiTaskServiceMessagePresentation {
    TuiTaskServiceMessagePresentation {
        show: Some(false),
        ..Default::default()
    }
}

impl Default for TuiTaskServiceMessageSettings {
    fn default() -> Self {
        Self {
            visible: true,
            assignment: TuiTaskServiceMessagePresentation::default(),
            progress: TuiTaskServiceMessagePresentation::default(),
            blocked: TuiTaskServiceMessagePresentation::default(),
            resumed: TuiTaskServiceMessagePresentation::default(),
            review_ready: TuiTaskServiceMessagePresentation::default(),
            retry: TuiTaskServiceMessagePresentation::default(),
            terminal_closure: default_hidden_task_service_terminal_closure(),
        }
    }
}

impl Default for TuiCutexInboundMessageSettings {
    fn default() -> Self {
        Self {
            label: None,
            style: default_cutex_inbound_style(),
            show_metadata: true,
            show_ids: true,
            content_indent: default_cutex_content_indent(),
        }
    }
}

fn default_cutex_inbound_style() -> TuiCutexTextStyle {
    TuiCutexTextStyle {
        foreground: Some(TuiCutexForegroundColor::Cyan),
        bold: Some(true),
        dim: Some(false),
        italic: Some(false),
    }
}

fn default_cutex_content_indent() -> u8 {
    2
}

/// Frontend-owned Cutex activity presentation settings.
///
/// These settings only affect the current TUI's rendering. They do not alter typed activity,
/// rollout items, model context, Task Service status, or Agent authority.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(deny_unknown_fields)]
pub struct TuiCutexActivitySettings {
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default = "default_true")]
    pub managed_agent_visible: bool,
    #[serde(default = "default_true")]
    pub outbound_message_visible: bool,
    #[serde(default = "default_true")]
    pub task_assignment_visible: bool,
    #[serde(default)]
    pub managed_agent: TuiManagedAgentActivityLabels,
    #[serde(default)]
    pub outbound_message: TuiOutboundMessageActivityLabels,
    #[serde(default)]
    pub task_assignment: TuiTaskAssignmentActivitySettings,
    #[serde(default)]
    pub inbound_message: TuiCutexInboundMessageSettings,
    #[serde(default)]
    pub task_service_message: TuiTaskServiceMessageSettings,
    #[serde(default)]
    pub task_watchdog: TuiTaskWatchdogSettings,
    #[serde(default)]
    pub phase_display: TuiAgentManagementPhaseDisplay,
    #[serde(default)]
    pub create: TuiAgentManagementOperationPresentation,
    #[serde(default)]
    pub query_managed: TuiAgentManagementOperationPresentation,
    #[serde(default)]
    pub online: TuiAgentManagementOperationPresentation,
    #[serde(default)]
    pub offline: TuiAgentManagementOperationPresentation,
    #[serde(default)]
    pub restart: TuiAgentManagementOperationPresentation,
    #[serde(default)]
    pub close: TuiAgentManagementOperationPresentation,
    #[serde(default)]
    pub replace: TuiAgentManagementOperationPresentation,
    #[serde(default)]
    pub director_rotate: TuiAgentManagementOperationPresentation,
}

impl Default for TuiCutexActivitySettings {
    fn default() -> Self {
        Self {
            visible: true,
            managed_agent_visible: true,
            outbound_message_visible: true,
            task_assignment_visible: true,
            managed_agent: TuiManagedAgentActivityLabels::default(),
            outbound_message: TuiOutboundMessageActivityLabels::default(),
            task_assignment: TuiTaskAssignmentActivitySettings::default(),
            inbound_message: TuiCutexInboundMessageSettings::default(),
            task_service_message: TuiTaskServiceMessageSettings::default(),
            task_watchdog: TuiTaskWatchdogSettings::default(),
            phase_display: TuiAgentManagementPhaseDisplay::default(),
            create: TuiAgentManagementOperationPresentation::default(),
            query_managed: TuiAgentManagementOperationPresentation::default(),
            online: TuiAgentManagementOperationPresentation::default(),
            offline: TuiAgentManagementOperationPresentation::default(),
            restart: TuiAgentManagementOperationPresentation::default(),
            close: TuiAgentManagementOperationPresentation::default(),
            replace: TuiAgentManagementOperationPresentation::default(),
            director_rotate: TuiAgentManagementOperationPresentation::default(),
        }
    }
}

/// Collection of settings that are specific to the TUI.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Tui {
    #[serde(default, flatten)]
    pub notification_settings: TuiNotificationSettings,

    #[serde(default, flatten)]
    pub notify_service_settings: NotifyServiceSettings,

    /// Enable animations (welcome screen, shimmer effects, spinners).
    /// Defaults to `true`.
    #[serde(default = "default_true")]
    pub animations: bool,

    /// Show startup tooltips in the TUI welcome screen.
    /// Defaults to `true`.
    #[serde(default = "default_true")]
    pub show_tooltips: bool,

    /// Generate automatic conversation recaps when the terminal is unfocused.
    /// Defaults to `true`. Disabling this leaves `/recap` available on demand.
    #[serde(default = "default_true")]
    pub auto_recap: bool,

    /// When true, disables burst-paste detection for typed input entirely.
    /// All characters are inserted as they are received, and no buffering
    /// or placeholder replacement will occur for fast keypress bursts.
    /// Overrides the legacy top-level `disable_paste_burst` setting. Defaults to `false`.
    pub disable_paste_burst: Option<bool>,

    /// Start the composer in Vim mode (`Normal`) by default.
    /// Defaults to `false`.
    #[serde(default)]
    pub vim_mode_default: bool,

    /// Start the TUI in raw scrollback mode for copy-friendly transcript output.
    /// Defaults to `false`.
    #[serde(default)]
    pub raw_output_mode: bool,

    /// Controls whether the TUI uses the terminal's alternate screen buffer.
    ///
    /// - `auto` (default): Use alternate screen.
    /// - `always`: Always use alternate screen.
    /// - `never`: Never use alternate screen (inline mode only, preserves scrollback).
    #[serde(default)]
    pub alternate_screen: AltScreenMode,

    /// Ordered list of status line item identifiers.
    ///
    /// When set, the TUI renders the selected items as the status line.
    /// When unset, the TUI defaults to: `model-with-reasoning` and `current-dir`.
    #[serde(default)]
    pub status_line: Option<Vec<String>>,

    /// Color status line items with colors derived from the active syntax theme.
    /// Defaults to `true`.
    #[serde(default = "default_true")]
    pub status_line_use_colors: bool,

    /// Ordered list of terminal title item identifiers.
    ///
    /// When set, the TUI renders the selected items into the terminal window/tab title.
    /// When unset, the TUI defaults to: `activity` and `project`.
    /// The `activity` item spins while working and shows an action-required
    /// message when blocked on the user.
    #[serde(default)]
    pub terminal_title: Option<Vec<String>>,

    /// Syntax highlighting theme name (kebab-case).
    ///
    /// When set, overrides automatic light/dark theme detection.
    /// Use `/theme` in the TUI or see `$CODEX_HOME/themes` for custom themes.
    #[serde(default)]
    pub theme: Option<String>,

    /// Pet id to preselect in the terminal pet picker.
    ///
    /// Custom pet ids resolve against CODEX_HOME/pets/<pet-id>/pet.json.
    #[serde(default)]
    pub pet: Option<String>,

    /// Where the terminal pet should anchor vertically.
    ///
    /// Defaults to `composer`, which follows the current TUI composer viewport.
    #[serde(default)]
    pub pet_anchor: TuiPetAnchor,

    /// Preferred layout for resume/fork session picker results.
    #[serde(default)]
    pub session_picker_view: Option<SessionPickerViewMode>,

    /// Provider filter scope for resume/fork session lookup.
    #[serde(default)]
    pub session_picker_provider_filter: Option<SessionPickerProviderFilter>,

    /// Working directory to use when resuming or forking a session.
    /// When unset, prompt if the current and session directories differ.
    #[serde(default)]
    pub resume_cwd: Option<ResumeCwdMode>,

    /// Keybinding overrides for the TUI.
    ///
    /// This supports rebinding selected actions globally and by context.
    /// Context bindings take precedence over `global` bindings.
    #[serde(default)]
    pub keymap: TuiKeymap,

    /// Startup tooltip availability NUX state persisted by the TUI.
    #[serde(default)]
    pub model_availability_nux: ModelAvailabilityNuxConfig,

    /// Trim terminal resize-reflow replay to the most recent rendered terminal rows when the
    /// transcript exceeds this cap. Omit to use Codex's terminal-specific default. Set to `0` to
    /// keep all rendered rows.
    #[serde(default)]
    #[schemars(range(min = 0))]
    pub terminal_resize_reflow_max_rows: Option<usize>,
}

const fn default_true() -> bool {
    true
}

/// Settings for notices we display to users via the tui and app-server clients
/// (primarily the Codex IDE extension). NOTE: these are different from
/// notifications - notices are warnings, NUX screens, acknowledgements, etc.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ExternalConfigMigrationPrompts {
    /// Tracks whether home-level external config migration prompts are hidden.
    pub home: Option<bool>,
    /// Tracks the last time the home-level external config migration prompt was shown.
    pub home_last_prompted_at: Option<i64>,
    /// Tracks which project paths have opted out of external config migration prompts.
    #[serde(default)]
    pub projects: BTreeMap<String, bool>,
    /// Tracks the last time a project-level external config migration prompt was shown.
    #[serde(default)]
    pub project_last_prompted_at: BTreeMap<String, i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Notice {
    /// Tracks whether the user has acknowledged the full access warning prompt.
    pub hide_full_access_warning: Option<bool>,
    /// Tracks whether the user has acknowledged the Windows world-writable directories warning.
    pub hide_world_writable_warning: Option<bool>,
    /// Tracks whether the user opted out of Codex-managed fast defaults.
    pub fast_default_opt_out: Option<bool>,
    /// Tracks whether the user opted out of the rate limit model switch reminder.
    pub hide_rate_limit_model_nudge: Option<bool>,
    /// Tracks whether the user has seen the model migration prompt
    pub hide_gpt5_1_migration_prompt: Option<bool>,
    /// Tracks whether the user has seen the gpt-5.1-codex-max migration prompt
    #[serde(rename = "hide_gpt-5.1-codex-max_migration_prompt")]
    pub hide_gpt_5_1_codex_max_migration_prompt: Option<bool>,
    /// Tracks acknowledged model migrations as old->new model slug mappings.
    #[serde(default)]
    pub model_migrations: BTreeMap<String, String>,
    /// Tracks scopes where external config migration prompts should be suppressed.
    #[serde(default)]
    pub external_config_migration_prompts: ExternalConfigMigrationPrompts,
}

pub use crate::skills_config::BundledSkillsConfig;
pub use crate::skills_config::SkillConfig;
pub use crate::skills_config::SkillsConfig;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct PluginConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Per-MCP-server policy overlays for MCP servers contributed by this plugin.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub mcp_servers: HashMap<String, PluginMcpServerConfig>,
}

/// Policy settings for a plugin-provided MCP server.
///
/// This intentionally excludes transport settings: plugin manifests own how the
/// MCP server is launched, while user config owns enablement and tool policy.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct PluginMcpServerConfig {
    /// When `false`, Codex skips initializing this plugin MCP server.
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Approval mode for tools in this server unless a tool override exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tools_approval_mode: Option<AppToolApproval>,

    /// Explicit allow-list of tools exposed from this server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled_tools: Option<Vec<String>>,

    /// Explicit deny-list of tools. These tools are removed after applying `enabled_tools`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled_tools: Option<Vec<String>>,

    /// Per-tool policy settings keyed by tool name.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tools: HashMap<String, McpServerToolConfig>,
}

impl Default for PluginMcpServerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_tools_approval_mode: None,
            enabled_tools: None,
            disabled_tools: None,
            tools: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct MarketplaceConfig {
    /// Last time Codex successfully added or refreshed this marketplace.
    #[serde(default)]
    pub last_updated: Option<String>,
    /// Git revision Codex last successfully activated for this marketplace.
    #[serde(default)]
    pub last_revision: Option<String>,
    /// Source kind used to install this marketplace.
    #[serde(default)]
    pub source_type: Option<MarketplaceSourceType>,
    /// Source location used when the marketplace was added.
    #[serde(default)]
    pub source: Option<String>,
    /// Git ref to check out when `source_type` is `git`.
    #[serde(default, rename = "ref")]
    pub ref_name: Option<String>,
    /// Sparse checkout paths used when `source_type` is `git`.
    #[serde(default)]
    pub sparse_paths: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarketplaceSourceType {
    Git,
    Local,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct SandboxWorkspaceWrite {
    #[serde(default)]
    pub writable_roots: Vec<AbsolutePathBuf>,
    #[serde(default)]
    pub network_access: bool,
    #[serde(default)]
    pub exclude_tmpdir_env_var: bool,
    #[serde(default)]
    pub exclude_slash_tmp: bool,
}

#[cfg(test)]
#[path = "types_tests.rs"]
mod tests;
