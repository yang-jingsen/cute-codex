//! Status line configuration view for customizing the TUI status bar.
//!
//! This module provides an interactive picker for selecting which items appear
//! in the status line at the bottom of the terminal. Users can:
//!
//! - **Select items**: Toggle which information is displayed
//! - **Reorder items**: Use left/right arrows to change display order
//! - **Preview changes**: See a live preview of the configured status line
//!
//! # Available Status Line Items
//!
//! - Model information (name, reasoning level)
//! - Directory paths (current dir, project root)
//! - Machine hostname
//! - Git information (branch name)
//! - Permissions profile
//! - Approval mode
//! - Context usage (remaining %, used %, window size)
//! - Usage limits (primary, secondary)
//! - Session info (thread title, thread ID, tokens used)
//! - Application version

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Stylize;
use ratatui::text::Line;
use std::collections::HashSet;
use strum_macros::Display;
use strum_macros::EnumIter;
use strum_macros::EnumString;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::bottom_pane_view::BottomPaneView;
use crate::bottom_pane::multi_select_picker::MultiSelectItem;
use crate::bottom_pane::multi_select_picker::MultiSelectPicker;
use crate::bottom_pane::status_line_from_segments;
use crate::bottom_pane::status_surface_preview::StatusSurfacePreviewData;
use crate::bottom_pane::status_surface_preview::StatusSurfacePreviewItem;
use crate::keymap::ListKeymap;
use crate::render::renderable::Renderable;

const STATUS_LINE_USE_THEME_COLORS_ITEM_ID: &str = "status-line-use-theme-colors";

/// Available items that can be displayed in the status line.
///
/// Each variant represents a piece of information that can be shown at the
/// bottom of the TUI. Items are serialized to kebab-case for configuration
/// storage (e.g., `ModelWithReasoning` becomes `model-with-reasoning`).
///
/// Some items are conditionally displayed based on availability:
/// - Git-related items only show when in a git repository
/// - Context/limit items only show when data is available from the API
/// - Thread ID only shows after a session has started
#[derive(EnumIter, EnumString, Display, Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
#[strum(serialize_all = "kebab_case")]
pub(crate) enum StatusLineItem {
    /// The current model name.
    #[strum(to_string = "model", serialize = "model-name")]
    ModelName,

    /// Model name with reasoning level suffix.
    ModelWithReasoning,

    /// Current reasoning level.
    Reasoning,

    /// Current working directory path.
    CurrentDir,

    /// cutex launch profile name, when provided by the launcher.
    LaunchProfile,

    /// cutex launch runtime label, when provided by the launcher.
    LaunchRuntime,

    /// Project root directory (if detected).
    #[strum(
        to_string = "project-name",
        serialize = "project",
        serialize = "project-root"
    )]
    ProjectRoot,

    /// Hostname of the machine running Codex.
    Hostname,

    /// Current git branch name (if in a repository).
    GitBranch,

    /// Open pull request number for the current branch.
    PullRequestNumber,

    /// Committed branch diff stats relative to the default branch.
    BranchChanges,

    /// Compact runtime run-state text.
    #[strum(to_string = "run-state", serialize = "status")]
    Status,

    /// Active permission profile or sandbox summary.
    Permissions,

    /// Active command approval mode.
    #[strum(to_string = "approval-mode", serialize = "approval")]
    ApprovalMode,

    /// Percentage of context window remaining.
    ContextRemaining,

    /// Percentage of context window used.
    ///
    /// Also accepts the legacy `context-usage` config value.
    #[strum(to_string = "context-used", serialize = "context-usage")]
    ContextUsed,

    /// Remaining usage on the primary rate limit.
    FiveHourLimit,

    /// Remaining usage on the secondary rate limit.
    WeeklyLimit,

    /// Codex application version.
    CodexVersion,

    /// Total context window size in tokens.
    ContextWindowSize,

    /// Total tokens used in the current session.
    UsedTokens,

    /// Total input tokens consumed.
    TotalInputTokens,

    /// Total output tokens generated.
    TotalOutputTokens,

    /// Estimated credits attributed directly to the current enterprise thread.
    ThreadCredits,

    /// Estimated dollar cost attributed directly to the current enterprise thread.
    EstimatedThreadCost,

    /// Full thread UUID.
    #[strum(to_string = "thread-id", serialize = "session-id")]
    SessionId,

    /// Whether Fast mode is currently active.
    FastMode,

    /// Whether raw scrollback mode is currently active.
    RawOutput,

    /// Current thread title (if set by user).
    ThreadTitle,

    /// Current workspace notification headline.
    WorkspaceHeadline,

    /// Latest checklist task progress from `update_plan` (if available).
    TaskProgress,
}

impl StatusLineItem {
    /// User-visible description shown in the popup.
    pub(crate) fn description(self) -> &'static str {
        match self {
            StatusLineItem::ModelName => "Current model name",
            StatusLineItem::ModelWithReasoning => "Current model name with reasoning level",
            StatusLineItem::Reasoning => "Current reasoning level",
            StatusLineItem::CurrentDir => "Current working directory",
            StatusLineItem::LaunchProfile => "cutex launch profile name (omitted outside cutex)",
            StatusLineItem::LaunchRuntime => "cutex launch runtime label (omitted outside cutex)",
            StatusLineItem::ProjectRoot => "Project name (omitted when unavailable)",
            StatusLineItem::Hostname => "Current machine hostname (omitted when unavailable)",
            StatusLineItem::GitBranch => "Current Git branch (omitted when unavailable)",
            StatusLineItem::PullRequestNumber => {
                "Open pull request number for the current branch (omitted when unavailable)"
            }
            StatusLineItem::BranchChanges => {
                "Committed branch changes against the default branch (omitted when unavailable)"
            }
            StatusLineItem::Status => "Compact session run-state text (Ready, Working, Thinking)",
            StatusLineItem::Permissions => "Active permission profile or sandbox mode",
            StatusLineItem::ApprovalMode => "Active command approval mode",
            StatusLineItem::ContextRemaining => {
                "Percentage of context window remaining (omitted when unknown)"
            }
            StatusLineItem::ContextUsed => {
                "Percentage of context window used (omitted when unknown)"
            }
            StatusLineItem::FiveHourLimit => {
                "Remaining usage on the primary usage limit (omitted when unavailable)"
            }
            StatusLineItem::WeeklyLimit => {
                "Remaining usage on the secondary usage limit (omitted when unavailable)"
            }
            StatusLineItem::CodexVersion => "Codex application version",
            StatusLineItem::ContextWindowSize => {
                "Total context window size in tokens (omitted when unknown)"
            }
            StatusLineItem::UsedTokens => "Total tokens used in session (omitted when zero)",
            StatusLineItem::TotalInputTokens => "Total input tokens used in session",
            StatusLineItem::TotalOutputTokens => "Total output tokens used in session",
            StatusLineItem::ThreadCredits => {
                "Estimated current-thread credits (Enterprise workspaces only; omitted when unavailable)"
            }
            StatusLineItem::EstimatedThreadCost => {
                "Estimated current-thread cost in USD (Enterprise workspaces only; omitted when unavailable)"
            }
            StatusLineItem::SessionId => "Current thread identifier (omitted until thread starts)",
            StatusLineItem::FastMode => "Whether Fast mode is currently active",
            StatusLineItem::RawOutput => "Whether raw scrollback mode is active",
            StatusLineItem::ThreadTitle => {
                "Current thread title, or thread identifier when unnamed"
            }
            StatusLineItem::WorkspaceHeadline => {
                "Workspace notification headline (Enterprise workspaces only; omitted when unavailable)"
            }
            StatusLineItem::TaskProgress => {
                "Latest task progress from update_plan (omitted until available)"
            }
        }
    }

    pub(crate) fn preview_item(self) -> StatusSurfacePreviewItem {
        match self {
            StatusLineItem::ModelName => StatusSurfacePreviewItem::Model,
            StatusLineItem::ModelWithReasoning => StatusSurfacePreviewItem::ModelWithReasoning,
            StatusLineItem::Reasoning => StatusSurfacePreviewItem::Reasoning,
            StatusLineItem::CurrentDir => StatusSurfacePreviewItem::CurrentDir,
            StatusLineItem::LaunchProfile => StatusSurfacePreviewItem::LaunchProfile,
            StatusLineItem::LaunchRuntime => StatusSurfacePreviewItem::LaunchRuntime,
            StatusLineItem::ProjectRoot => StatusSurfacePreviewItem::ProjectRoot,
            StatusLineItem::Hostname => StatusSurfacePreviewItem::Hostname,
            StatusLineItem::GitBranch => StatusSurfacePreviewItem::GitBranch,
            StatusLineItem::PullRequestNumber => StatusSurfacePreviewItem::PullRequestNumber,
            StatusLineItem::BranchChanges => StatusSurfacePreviewItem::BranchChanges,
            StatusLineItem::Status => StatusSurfacePreviewItem::Status,
            StatusLineItem::Permissions => StatusSurfacePreviewItem::Permissions,
            StatusLineItem::ApprovalMode => StatusSurfacePreviewItem::ApprovalMode,
            StatusLineItem::ContextRemaining => StatusSurfacePreviewItem::ContextRemaining,
            StatusLineItem::ContextUsed => StatusSurfacePreviewItem::ContextUsed,
            StatusLineItem::FiveHourLimit => StatusSurfacePreviewItem::FiveHourLimit,
            StatusLineItem::WeeklyLimit => StatusSurfacePreviewItem::WeeklyLimit,
            StatusLineItem::CodexVersion => StatusSurfacePreviewItem::CodexVersion,
            StatusLineItem::ContextWindowSize => StatusSurfacePreviewItem::ContextWindowSize,
            StatusLineItem::UsedTokens => StatusSurfacePreviewItem::UsedTokens,
            StatusLineItem::TotalInputTokens => StatusSurfacePreviewItem::TotalInputTokens,
            StatusLineItem::TotalOutputTokens => StatusSurfacePreviewItem::TotalOutputTokens,
            StatusLineItem::ThreadCredits => StatusSurfacePreviewItem::ThreadCredits,
            StatusLineItem::EstimatedThreadCost => StatusSurfacePreviewItem::EstimatedThreadCost,
            StatusLineItem::SessionId => StatusSurfacePreviewItem::SessionId,
            StatusLineItem::FastMode => StatusSurfacePreviewItem::FastMode,
            StatusLineItem::RawOutput => StatusSurfacePreviewItem::RawOutput,
            StatusLineItem::ThreadTitle => StatusSurfacePreviewItem::ThreadTitle,
            StatusLineItem::WorkspaceHeadline => StatusSurfacePreviewItem::WorkspaceHeadline,
            StatusLineItem::TaskProgress => StatusSurfacePreviewItem::TaskProgress,
        }
    }
}

#[derive(Clone, Debug)]
enum StatusLineChoicePreview {
    Builtin(StatusLineItem),
    Custom(Option<Line<'static>>),
}

/// One selectable status-line item, backed by either a native item or an
/// externally supplied catalog entry.
#[derive(Clone, Debug)]
pub(crate) struct StatusLineChoice {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    preview: StatusLineChoicePreview,
}

impl StatusLineChoice {
    pub(crate) fn builtin(item: StatusLineItem, preview_data: &StatusSurfacePreviewData) -> Self {
        let default_name = item.to_string();
        let default_description = item.description();
        let (name, description) = match item {
            StatusLineItem::FiveHourLimit | StatusLineItem::WeeklyLimit => (
                preview_data.rate_limit_item_name(item.preview_item(), &default_name),
                preview_data.rate_limit_item_description(item.preview_item(), default_description),
            ),
            _ => (default_name, default_description.to_string()),
        };
        Self {
            id: item.to_string(),
            name,
            description: Some(description),
            preview: StatusLineChoicePreview::Builtin(item),
        }
    }

    pub(crate) fn custom(
        id: String,
        name: String,
        description: Option<String>,
        preview: Option<Line<'static>>,
    ) -> Self {
        Self {
            id,
            name,
            description,
            preview: StatusLineChoicePreview::Custom(preview),
        }
    }

    fn into_select_item(self, enabled: bool) -> MultiSelectItem {
        MultiSelectItem {
            id: self.id,
            name: self.name,
            description: self.description,
            enabled,
            orderable: true,
            section_break_after: false,
        }
    }

    fn preview_line(
        &self,
        preview_data: &StatusSurfacePreviewData,
        use_theme_colors: bool,
    ) -> Option<Line<'static>> {
        match &self.preview {
            StatusLineChoicePreview::Builtin(item) => preview_data
                .value_for(item.preview_item())
                .and_then(|value| {
                    status_line_from_segments([(*item, value.to_string())], use_theme_colors)
                }),
            StatusLineChoicePreview::Custom(line) => line.clone(),
        }
    }
}

fn preview_line_for_choices(
    items: &[MultiSelectItem],
    choices: &[StatusLineChoice],
    preview_data: &StatusSurfacePreviewData,
    use_theme_colors: bool,
) -> Option<Line<'static>> {
    let mut line: Option<Line<'static>> = None;
    for item in items
        .iter()
        .filter(|item| item.enabled && item.id != STATUS_LINE_USE_THEME_COLORS_ITEM_ID)
    {
        let Some(choice) = choices.iter().find(|choice| choice.id == item.id) else {
            continue;
        };
        let Some(preview) = choice.preview_line(preview_data, use_theme_colors) else {
            continue;
        };
        if let Some(existing) = line.as_mut() {
            existing.spans.push(" · ".dim());
            existing.spans.extend(preview.spans);
        } else {
            line = Some(preview);
        }
    }
    line
}

/// Interactive view for configuring which items appear in the status line.
///
/// Wraps a [`MultiSelectPicker`] with status-line-specific behavior:
/// - Pre-populates items from current configuration
/// - Shows a live preview of the configured status line
/// - Emits [`AppEvent::StatusLineSetup`] on confirmation
/// - Emits [`AppEvent::StatusLineSetupCancelled`] on cancellation
pub(crate) struct StatusLineSetupView {
    /// The underlying multi-select picker widget.
    picker: MultiSelectPicker,
}

impl StatusLineSetupView {
    /// Creates a new status line setup view.
    ///
    /// # Arguments
    ///
    /// * `status_line_items` - Currently configured item IDs (in display order),
    ///   or `None` to start with all items disabled
    /// * `use_theme_colors` - Whether the preview and saved status line use colors from
    ///   the active theme
    /// * `choices` - Native and external catalog items available to select
    /// * `app_event_tx` - Event sender for dispatching configuration changes
    ///
    /// Items from `status_line_items` are shown first (in order) and marked as
    /// enabled. Remaining items are appended and marked as disabled.
    pub(crate) fn new(
        status_line_items: Option<&[String]>,
        use_theme_colors: bool,
        choices: &[StatusLineChoice],
        preview_data: StatusSurfacePreviewData,
        app_event_tx: AppEventSender,
        list_keymap: ListKeymap,
    ) -> Self {
        let mut used_ids = HashSet::new();
        let mut items = vec![MultiSelectItem {
            id: STATUS_LINE_USE_THEME_COLORS_ITEM_ID.to_string(),
            name: "Use theme colors".to_string(),
            description: Some("Apply colors from the active /theme".to_string()),
            enabled: use_theme_colors,
            orderable: false,
            section_break_after: true,
        }];

        if let Some(selected_items) = status_line_items.as_ref() {
            for id in *selected_items {
                let choice_id = id
                    .parse::<StatusLineItem>()
                    .map(|item| item.to_string())
                    .unwrap_or_else(|_| id.clone());
                if !used_ids.insert(choice_id.clone()) {
                    continue;
                }
                if let Some(choice) = choices.iter().find(|choice| choice.id == choice_id) {
                    items.push(choice.clone().into_select_item(/*enabled*/ true));
                }
            }
        }

        for choice in choices {
            if used_ids.contains(&choice.id) {
                continue;
            }
            items.push(choice.clone().into_select_item(/*enabled*/ false));
        }

        let preview_choices = choices.to_vec();

        Self {
            picker: MultiSelectPicker::builder(
                "Configure Status Line".to_string(),
                Some("Select which items to display in the status line.".to_string()),
                app_event_tx,
            )
            .list_keymap(list_keymap)
            .items(items)
            .enable_ordering()
            .on_preview(move |items| {
                let use_theme_colors = items
                    .iter()
                    .find(|item| item.id == STATUS_LINE_USE_THEME_COLORS_ITEM_ID)
                    .map(|item| item.enabled)
                    .unwrap_or(true);
                preview_line_for_choices(items, &preview_choices, &preview_data, use_theme_colors)
            })
            .on_confirm(|ids, app_event| {
                let use_theme_colors = ids
                    .iter()
                    .any(|id| id == STATUS_LINE_USE_THEME_COLORS_ITEM_ID);
                let ids = ids
                    .iter()
                    .filter(|id| *id != STATUS_LINE_USE_THEME_COLORS_ITEM_ID)
                    .cloned()
                    .collect::<Vec<_>>();
                app_event.send(AppEvent::StatusLineSetup {
                    ids,
                    use_theme_colors,
                });
            })
            .on_cancel(|app_event| {
                app_event.send(AppEvent::StatusLineSetupCancelled);
            })
            .build(),
        }
    }
}

impl BottomPaneView for StatusLineSetupView {
    fn keymap_contexts(&self) -> crate::keymap::KeymapContextSet {
        crate::keymap::KeymapContextSet::new(crate::keymap::KeymapContext::List)
    }

    fn handle_key_event(&mut self, key_event: crossterm::event::KeyEvent) {
        self.picker.handle_key_event(key_event);
    }

    fn is_complete(&self) -> bool {
        self.picker.complete
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.picker.close();
        CancellationEvent::Handled
    }
}

impl Renderable for StatusLineSetupView {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.picker.render(area, buf)
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.picker.desired_height(width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_event_sender::AppEventSender;
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::text::Line;
    use strum::IntoEnumIterator;
    use tokio::sync::mpsc::unbounded_channel;

    use crate::app_event::AppEvent;

    #[test]
    fn context_used_accepts_context_usage_legacy_id() {
        assert_eq!(StatusLineItem::ContextUsed.to_string(), "context-used");
        assert_eq!(
            "context-used".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ContextUsed)
        );
        assert_eq!(
            "context-usage".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ContextUsed)
        );
    }

    #[test]
    fn context_remaining_is_selectable_id() {
        assert_eq!(
            "context-remaining".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ContextRemaining)
        );
        assert_eq!(
            StatusLineItem::ContextRemaining.to_string(),
            "context-remaining"
        );
    }

    #[test]
    fn thread_usage_items_are_independently_selectable() {
        assert_eq!(
            "thread-credits".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ThreadCredits)
        );
        assert_eq!(
            "estimated-thread-cost".parse::<StatusLineItem>(),
            Ok(StatusLineItem::EstimatedThreadCost)
        );
    }

    #[test]
    fn project_name_is_canonical_and_accepts_legacy_ids() {
        assert_eq!(StatusLineItem::ProjectRoot.to_string(), "project-name");
        assert_eq!(
            "project-name".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ProjectRoot)
        );
        assert_eq!(
            "project".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ProjectRoot)
        );
        assert_eq!(
            "project-root".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ProjectRoot)
        );
    }

    #[test]
    fn model_is_canonical_and_accepts_model_name_legacy_id() {
        assert_eq!(StatusLineItem::ModelName.to_string(), "model");
        assert_eq!(
            "model".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ModelName)
        );
        assert_eq!(
            "model-name".parse::<StatusLineItem>(),
            Ok(StatusLineItem::ModelName)
        );
    }

    #[test]
    fn reasoning_is_selectable_id() {
        assert_eq!(StatusLineItem::Reasoning.to_string(), "reasoning");
        assert_eq!(
            "reasoning".parse::<StatusLineItem>(),
            Ok(StatusLineItem::Reasoning)
        );
    }

    #[test]
    fn run_state_is_canonical_and_accepts_status_legacy_id() {
        assert_eq!(StatusLineItem::Status.to_string(), "run-state");
        assert_eq!(
            "run-state".parse::<StatusLineItem>(),
            Ok(StatusLineItem::Status)
        );
        assert_eq!(
            "status".parse::<StatusLineItem>(),
            Ok(StatusLineItem::Status)
        );
    }

    #[test]
    fn git_summary_items_are_selectable_ids() {
        assert_eq!(
            "pull-request-number".parse::<StatusLineItem>(),
            Ok(StatusLineItem::PullRequestNumber)
        );
        assert_eq!(
            "branch-changes".parse::<StatusLineItem>(),
            Ok(StatusLineItem::BranchChanges)
        );
    }

    #[test]
    fn parse_status_line_items_accepts_title_only_variants() {
        let items = ["run-state", "task-progress"]
            .into_iter()
            .map(str::parse::<StatusLineItem>)
            .collect::<Result<Vec<_>, _>>();
        assert_eq!(
            items,
            Ok(vec![StatusLineItem::Status, StatusLineItem::TaskProgress,])
        );
    }

    #[test]
    fn preview_uses_runtime_values() {
        let preview_data = StatusSurfacePreviewData::from_iter([
            (
                StatusLineItem::ModelName.preview_item(),
                "gpt-5".to_string(),
            ),
            (
                StatusLineItem::CurrentDir.preview_item(),
                "/repo".to_string(),
            ),
        ]);
        let items = [
            MultiSelectItem {
                id: StatusLineItem::ModelName.to_string(),
                name: String::new(),
                description: None,
                enabled: true,
                orderable: true,
                section_break_after: false,
            },
            MultiSelectItem {
                id: StatusLineItem::CurrentDir.to_string(),
                name: String::new(),
                description: None,
                enabled: true,
                orderable: true,
                section_break_after: false,
            },
        ];

        assert_eq!(
            line_text(
                preview_data.status_line_for_items(
                    items
                        .iter()
                        .filter_map(|item| item.id.parse::<StatusLineItem>().ok()),
                    /*use_theme_colors*/ true,
                )
            ),
            Some("gpt-5 · /repo".to_string())
        );
    }

    #[test]
    fn preview_uses_placeholders_when_runtime_values_are_missing() {
        let preview_data = StatusSurfacePreviewData::from_iter([(
            StatusSurfacePreviewItem::Model,
            "gpt-5".to_string(),
        )]);
        let items = [
            MultiSelectItem {
                id: StatusLineItem::ModelName.to_string(),
                name: String::new(),
                description: None,
                enabled: true,
                orderable: true,
                section_break_after: false,
            },
            MultiSelectItem {
                id: StatusLineItem::GitBranch.to_string(),
                name: String::new(),
                description: None,
                enabled: true,
                orderable: true,
                section_break_after: false,
            },
        ];

        assert_eq!(
            line_text(
                preview_data.status_line_for_items(
                    items
                        .iter()
                        .filter_map(|item| item.id.parse::<StatusLineItem>().ok()),
                    /*use_theme_colors*/ true,
                )
            ),
            Some("gpt-5 · feat/awesome-feature".to_string())
        );
    }

    #[test]
    fn preview_includes_thread_title() {
        let preview_data = StatusSurfacePreviewData::from_iter([
            (
                StatusLineItem::ModelName.preview_item(),
                "gpt-5".to_string(),
            ),
            (
                StatusLineItem::ThreadTitle.preview_item(),
                "Roadmap cleanup".to_string(),
            ),
        ]);
        let items = [
            MultiSelectItem {
                id: StatusLineItem::ModelName.to_string(),
                name: String::new(),
                description: None,
                enabled: true,
                orderable: true,
                section_break_after: false,
            },
            MultiSelectItem {
                id: StatusLineItem::ThreadTitle.to_string(),
                name: String::new(),
                description: None,
                enabled: true,
                orderable: true,
                section_break_after: false,
            },
        ];

        assert_eq!(
            line_text(
                preview_data.status_line_for_items(
                    items
                        .iter()
                        .filter_map(|item| item.id.parse::<StatusLineItem>().ok()),
                    /*use_theme_colors*/ true,
                )
            ),
            Some("gpt-5 · Roadmap cleanup".to_string())
        );
    }

    #[test]
    fn setup_view_snapshot_uses_runtime_preview_values() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let preview_data = StatusSurfacePreviewData::from_iter([
            (
                StatusLineItem::ModelName.preview_item(),
                "gpt-5-codex".to_string(),
            ),
            (
                StatusLineItem::CurrentDir.preview_item(),
                "~/codex-rs".to_string(),
            ),
            (
                StatusLineItem::GitBranch.preview_item(),
                "jif/statusline-preview".to_string(),
            ),
            (
                StatusLineItem::WeeklyLimit.preview_item(),
                "weekly 82% left".to_string(),
            ),
        ]);
        let choices = StatusLineItem::iter()
            .map(|item| StatusLineChoice::builtin(item, &preview_data))
            .collect::<Vec<_>>();
        let view = StatusLineSetupView::new(
            Some(&[
                StatusLineItem::ModelName.to_string(),
                StatusLineItem::CurrentDir.to_string(),
                StatusLineItem::GitBranch.to_string(),
            ]),
            /*use_theme_colors*/ true,
            &choices,
            preview_data,
            AppEventSender::new(tx_raw),
            crate::keymap::RuntimeKeymap::defaults().list,
        );

        assert_snapshot!(render_lines(&view, /*width*/ 72));
    }

    #[test]
    fn mixed_preview_preserves_custom_style_and_native_theme_toggle() {
        let preview_data = StatusSurfacePreviewData::from_iter([(
            StatusLineItem::ModelName.preview_item(),
            "gpt-5".to_string(),
        )]);
        let choices = [
            StatusLineChoice::builtin(StatusLineItem::ModelName, &preview_data),
            StatusLineChoice::custom(
                "custom:red".to_string(),
                "Red".to_string(),
                /*description*/ None,
                Some(Line::from(ratatui::text::Span::styled(
                    "custom",
                    ratatui::style::Style::default().red(),
                ))),
            ),
        ];
        let items = [
            choices[0].clone().into_select_item(/*enabled*/ true),
            choices[1].clone().into_select_item(/*enabled*/ true),
        ];

        let colored = preview_line_for_choices(
            &items,
            &choices,
            &preview_data,
            /*use_theme_colors*/ true,
        )
        .expect("mixed preview should render");
        let uncolored = preview_line_for_choices(
            &items,
            &choices,
            &preview_data,
            /*use_theme_colors*/ false,
        )
        .expect("mixed preview should render");

        assert_eq!(
            line_text(Some(colored.clone())).as_deref(),
            Some("gpt-5 · custom")
        );
        assert_ne!(colored.spans[0].style.fg, None);
        assert!(
            uncolored.spans[0]
                .style
                .add_modifier
                .contains(ratatui::style::Modifier::DIM)
        );
        assert_eq!(colored.spans[2].style.fg, Some(ratatui::style::Color::Red));
        assert_eq!(
            uncolored.spans[2].style.fg,
            Some(ratatui::style::Color::Red)
        );
    }

    #[test]
    fn setup_confirmation_preserves_external_string_ids() {
        let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
        let preview_data = StatusSurfacePreviewData::default();
        let choices = [
            StatusLineChoice::builtin(StatusLineItem::ModelName, &preview_data),
            StatusLineChoice::custom(
                "custom:profile".to_string(),
                "Profile".to_string(),
                Some("Launcher profile".to_string()),
                Some(Line::from("Profile sample")),
            ),
        ];
        let mut view = StatusLineSetupView::new(
            Some(&["model-name".to_string(), "custom:profile".to_string()]),
            /*use_theme_colors*/ true,
            &choices,
            preview_data,
            AppEventSender::new(tx_raw),
            crate::keymap::RuntimeKeymap::defaults().list,
        );

        view.handle_key_event(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));

        match rx.try_recv().expect("confirmation event should be sent") {
            AppEvent::StatusLineSetup {
                ids,
                use_theme_colors,
            } => {
                assert_eq!(ids, vec!["model".to_string(), "custom:profile".to_string()]);
                assert!(use_theme_colors);
            }
            event => panic!("unexpected event: {event:?}"),
        }
    }

    #[test]
    fn setup_view_snapshot_includes_thread_usage_items() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let preview_data = StatusSurfacePreviewData::from_iter([
            (
                StatusLineItem::ThreadCredits.preview_item(),
                "5.2 credits".to_string(),
            ),
            (
                StatusLineItem::EstimatedThreadCost.preview_item(),
                "~$1.82".to_string(),
            ),
        ]);
        let choices = StatusLineItem::iter()
            .map(|item| StatusLineChoice::builtin(item, &preview_data))
            .collect::<Vec<_>>();
        let view = StatusLineSetupView::new(
            Some(&[
                StatusLineItem::ThreadCredits.to_string(),
                StatusLineItem::EstimatedThreadCost.to_string(),
            ]),
            /*use_theme_colors*/ true,
            &choices,
            preview_data,
            AppEventSender::new(tx_raw),
            crate::keymap::RuntimeKeymap::defaults().list,
        );

        assert_snapshot!(render_lines(&view, /*width*/ 100));
    }

    #[test]
    fn setup_view_snapshot_includes_hostname() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let preview_data = StatusSurfacePreviewData::from_iter([
            (
                StatusLineItem::Hostname.preview_item(),
                "ssh-build-01.example.com".to_string(),
            ),
            (
                StatusLineItem::CurrentDir.preview_item(),
                "~/codex-rs".to_string(),
            ),
        ]);
        let choices = StatusLineItem::iter()
            .map(|item| StatusLineChoice::builtin(item, &preview_data))
            .collect::<Vec<_>>();
        let view = StatusLineSetupView::new(
            Some(&[
                StatusLineItem::Hostname.to_string(),
                StatusLineItem::CurrentDir.to_string(),
            ]),
            /*use_theme_colors*/ true,
            &choices,
            preview_data,
            AppEventSender::new(tx_raw),
            crate::keymap::RuntimeKeymap::defaults().list,
        );

        assert_snapshot!(
            render_lines(&view, /*width*/ 100)
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    fn render_lines(view: &StatusLineSetupView, width: u16) -> String {
        let height = view.desired_height(width);
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        (0..area.height)
            .map(|row| {
                let mut line = String::new();
                for col in 0..area.width {
                    let symbol = buf[(area.x + col, area.y + row)].symbol();
                    if symbol.is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(symbol);
                    }
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn line_text(line: Option<Line<'static>>) -> Option<String> {
        line.map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
    }
}
