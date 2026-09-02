//! Render-only Cutex Director timeline activities.
//!
//! These cells intentionally operate on opaque Cutex identifiers. They do not participate in the
//! native agent tree, liveness reducer, or model-visible conversation history.

use std::sync::Arc;
use std::sync::RwLock;

use chrono::SecondsFormat;
use codex_app_server_protocol::CutexUiActivityDelivery;
use codex_app_server_protocol::CutexUiActivityDeliveryClass;
use codex_app_server_protocol::ManagedAgentActivityStatus;
use codex_app_server_protocol::ManagedAgentOperation;
use codex_app_server_protocol::OutboundInterAgentMessageStatus;
use codex_app_server_protocol::TaskAssignmentActivityItem;
use codex_app_server_protocol::TaskAssignmentActivityStatus;
use codex_app_server_protocol::TaskWatchdogActivityItem;
use codex_app_server_protocol::TaskWatchdogStage;
use codex_app_server_protocol::ThreadItem;
use codex_config::types::TuiAgentManagementPhaseDisplay;
use codex_config::types::TuiCutexActivitySettings;
#[cfg(test)]
use codex_config::types::TuiCutexForegroundColor;
#[cfg(test)]
use codex_config::types::TuiCutexRichSpan;
use codex_config::types::TuiCutexTextStyle;
use codex_config::types::TuiTaskAssignmentStatusPresentation;
use codex_config::types::TuiTaskWatchdogStagePresentation;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;

use crate::cutex_rich_text;
use crate::cutex_template_groups::TemplateDecision;
use crate::history_cell::HistoryCell;
use crate::history_cell::plain_lines;

#[derive(Debug)]
struct CutexActivityState {
    item: ThreadItem,
    template: Option<TemplateDecision>,
}

#[derive(Clone, Debug)]
pub(crate) struct CutexActivityHandle {
    state: Arc<RwLock<CutexActivityState>>,
}

impl CutexActivityHandle {
    pub(crate) fn update(&self, item: ThreadItem, template: Option<TemplateDecision>) {
        #[expect(clippy::expect_used)]
        let mut current = self.state.write().expect("Cutex activity state poisoned");
        *current = CutexActivityState { item, template };
    }
}

#[derive(Debug)]
pub(crate) struct CutexActivityHistoryCell {
    state: Arc<RwLock<CutexActivityState>>,
    presentation: TuiCutexActivitySettings,
}

#[derive(Debug)]
struct RecoveredCutexActivityEntry {
    key: String,
    delivery: CutexUiActivityDelivery,
    item: ThreadItem,
    template: Option<TemplateDecision>,
}

#[derive(Debug)]
struct RecoveredCutexActivityState {
    entries: Vec<RecoveredCutexActivityEntry>,
    received: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct RecoveredCutexActivityHandle {
    state: Arc<RwLock<RecoveredCutexActivityState>>,
}

impl RecoveredCutexActivityHandle {
    pub(crate) fn update(
        &self,
        key: String,
        delivery: CutexUiActivityDelivery,
        item: ThreadItem,
        template: Option<TemplateDecision>,
    ) {
        #[expect(clippy::expect_used)]
        let mut state = self
            .state
            .write()
            .expect("recovered Cutex activity state poisoned");
        state.received = state.received.saturating_add(1);
        let entry = RecoveredCutexActivityEntry {
            key: key.clone(),
            delivery,
            item,
            template,
        };
        if let Some(existing) = state.entries.iter_mut().find(|entry| entry.key == key) {
            *existing = entry;
        } else {
            state.entries.push(entry);
        }
        state.entries.sort_by(|left, right| {
            left.delivery
                .source_checkpoint
                .sequence
                .cmp(&right.delivery.source_checkpoint.sequence)
                .then_with(|| {
                    left.delivery
                        .source_checkpoint
                        .cursor
                        .cmp(&right.delivery.source_checkpoint.cursor)
                })
        });
    }
}

#[derive(Debug)]
pub(crate) struct RecoveredCutexActivityHistoryCell {
    state: Arc<RwLock<RecoveredCutexActivityState>>,
    presentation: TuiCutexActivitySettings,
}

pub(crate) fn new_recovered_activity_cell(
    key: String,
    delivery: CutexUiActivityDelivery,
    item: ThreadItem,
    template: Option<TemplateDecision>,
    presentation: TuiCutexActivitySettings,
) -> (
    RecoveredCutexActivityHistoryCell,
    RecoveredCutexActivityHandle,
) {
    let state = Arc::new(RwLock::new(RecoveredCutexActivityState {
        entries: vec![RecoveredCutexActivityEntry {
            key,
            delivery,
            item,
            template,
        }],
        received: 1,
    }));
    (
        RecoveredCutexActivityHistoryCell {
            state: Arc::clone(&state),
            presentation,
        },
        RecoveredCutexActivityHandle { state },
    )
}

pub(crate) fn new_activity_cell(
    item: ThreadItem,
    template: Option<TemplateDecision>,
    presentation: TuiCutexActivitySettings,
) -> (CutexActivityHistoryCell, CutexActivityHandle) {
    let state = Arc::new(RwLock::new(CutexActivityState { item, template }));
    (
        CutexActivityHistoryCell {
            state: Arc::clone(&state),
            presentation,
        },
        CutexActivityHandle { state },
    )
}

pub(crate) fn is_visible(item: &ThreadItem, presentation: &TuiCutexActivitySettings) -> bool {
    presentation.visible
        && match item {
            ThreadItem::ManagedAgentActivity { .. } => presentation.managed_agent_visible,
            ThreadItem::OutboundInterAgentMessage { .. } => presentation.outbound_message_visible,
            ThreadItem::TaskAssignmentActivity { .. } => presentation.task_assignment_visible,
            ThreadItem::TaskWatchdogActivity { .. } => presentation.task_watchdog.visible,
            _ => false,
        }
}

pub(crate) fn has_visible_content(
    item: &ThreadItem,
    presentation: &TuiCutexActivitySettings,
) -> bool {
    match item {
        ThreadItem::ManagedAgentActivity { activity } if activity.phase.is_some() => {
            crate::cutex_management_phases::is_shown(activity, presentation)
        }
        ThreadItem::TaskWatchdogActivity { activity } => {
            watchdog_stage_settings(activity.stage, presentation).show != Some(false)
        }
        ThreadItem::TaskAssignmentActivity { activity } => {
            task_status_settings(activity.status, presentation).show != Some(false)
        }
        _ => true,
    }
}

pub(crate) fn activity_key(
    item: &ThreadItem,
    presentation: &TuiCutexActivitySettings,
) -> Option<String> {
    match item {
        ThreadItem::ManagedAgentActivity { activity } if activity.phase.is_some() => {
            let action_id = activity.action_id.as_deref().unwrap_or(&activity.id);
            Some(
                match crate::cutex_management_phases::display_mode(activity, presentation) {
                    TuiAgentManagementPhaseDisplay::Coalesce => {
                        format!("managed-phase:{action_id}")
                    }
                    TuiAgentManagementPhaseDisplay::Append => format!(
                        "managed-phase:{action_id}:{}",
                        activity
                            .phase_event_id
                            .as_deref()
                            .unwrap_or(&activity.event_id)
                    ),
                },
            )
        }
        ThreadItem::ManagedAgentActivity { activity } => Some(format!("managed:{}", activity.id)),
        ThreadItem::OutboundInterAgentMessage { activity } => {
            Some(format!("outbound:{}", activity.id))
        }
        ThreadItem::TaskAssignmentActivity { activity } => Some(format!("task:{}", activity.id)),
        ThreadItem::TaskWatchdogActivity { activity } => Some(format!("watchdog:{}", activity.id)),
        _ => None,
    }
}

pub(crate) fn occurred_at_ms(item: &ThreadItem) -> Option<i64> {
    match item {
        ThreadItem::ManagedAgentActivity { activity } => Some(activity.occurred_at_ms),
        ThreadItem::OutboundInterAgentMessage { activity } => Some(activity.occurred_at_ms),
        ThreadItem::TaskAssignmentActivity { activity } => Some(activity.occurred_at_ms),
        ThreadItem::TaskWatchdogActivity { activity } => Some(activity.occurred_at_ms),
        _ => None,
    }
}

pub(crate) fn is_terminal(item: &ThreadItem, presentation: &TuiCutexActivitySettings) -> bool {
    match item {
        ThreadItem::ManagedAgentActivity { activity }
            if activity.phase.is_some()
                && matches!(
                    crate::cutex_management_phases::display_mode(activity, presentation),
                    TuiAgentManagementPhaseDisplay::Append
                ) =>
        {
            true
        }
        ThreadItem::ManagedAgentActivity { activity } => {
            !matches!(activity.status, ManagedAgentActivityStatus::InProgress)
        }
        ThreadItem::OutboundInterAgentMessage { activity } => {
            !matches!(activity.status, OutboundInterAgentMessageStatus::Sending)
        }
        ThreadItem::TaskAssignmentActivity { activity } => matches!(
            activity.status,
            TaskAssignmentActivityStatus::ReviewReady
                | TaskAssignmentActivityStatus::Completed
                | TaskAssignmentActivityStatus::Failed
                | TaskAssignmentActivityStatus::Closed
                | TaskAssignmentActivityStatus::Declined
                | TaskAssignmentActivityStatus::Aborted
        ),
        ThreadItem::TaskWatchdogActivity { .. } => false,
        _ => true,
    }
}

pub(crate) fn summary_lines(
    item: &ThreadItem,
    presentation: &TuiCutexActivitySettings,
) -> Vec<Line<'static>> {
    summary_lines_at_width(item, presentation, u16::MAX)
}

fn summary_lines_at_width(
    item: &ThreadItem,
    presentation: &TuiCutexActivitySettings,
    width: u16,
) -> Vec<Line<'static>> {
    summary_lines_at_width_with_decision(item, presentation, width, None)
}

fn summary_lines_at_width_with_decision(
    item: &ThreadItem,
    presentation: &TuiCutexActivitySettings,
    width: u16,
    template: Option<&TemplateDecision>,
) -> Vec<Line<'static>> {
    if !is_visible(item, presentation) {
        return Vec::new();
    }
    match item {
        ThreadItem::ManagedAgentActivity { activity } if activity.phase.is_some() => {
            crate::cutex_management_phases::summary_lines_with_decision(
                activity,
                presentation,
                width,
                template,
            )
        }
        ThreadItem::ManagedAgentActivity { activity } => {
            let target = activity
                .managed_agent_metadata
                .as_ref()
                .and_then(|metadata| metadata.display_name.as_deref())
                .or(activity.managed_agent_name.as_deref())
                .unwrap_or(&activity.managed_agent_id);
            let default = format!(
                "{}{target}",
                managed_default_phrase(activity.operation, activity.status)
            );
            let template = match activity.status {
                ManagedAgentActivityStatus::InProgress => {
                    presentation.managed_agent.in_progress.as_deref()
                }
                ManagedAgentActivityStatus::Completed => {
                    presentation.managed_agent.completed.as_deref()
                }
                ManagedAgentActivityStatus::Failed => presentation.managed_agent.failed.as_deref(),
            };
            let operation = format!("{:?}", activity.operation);
            let status = format!("{:?}", activity.status);
            let action = render_template(
                template,
                default,
                &[
                    ("agent", target),
                    ("operation", &operation),
                    ("status", &status),
                ],
            );
            let action = match activity.status {
                ManagedAgentActivityStatus::InProgress => action.cyan(),
                ManagedAgentActivityStatus::Completed => action.green(),
                ManagedAgentActivityStatus::Failed => action.red(),
            };
            let mut lines = vec![vec!["• ".dim(), action].into()];
            lines.push(
                format!(
                    "  managed agent · id={} · status={:?} · sequence={}",
                    activity.managed_agent_id, activity.status, activity.sequence
                )
                .dim()
                .into(),
            );
            if let Some(role) = activity.managed_agent_role.as_deref() {
                lines.push(format!("  role: {role}").dim().into());
            }
            if let Some(preview) = activity.initial_task_preview.as_deref() {
                lines.push(
                    format!("  initial task: {}", bounded_preview(preview))
                        .dim()
                        .into(),
                );
            }
            if let Some(detail) = activity.detail.as_deref() {
                lines.push(format!("  {detail}").dim().into());
            }
            lines
        }
        ThreadItem::OutboundInterAgentMessage { activity } => {
            let target = activity
                .recipient_metadata
                .as_ref()
                .and_then(|metadata| metadata.display_name.as_deref())
                .or(activity.recipient_agent_name.as_deref())
                .unwrap_or(&activity.recipient_agent_id);
            let (template, default) = match activity.status {
                OutboundInterAgentMessageStatus::Sending => (
                    presentation.outbound_message.sending.as_deref(),
                    format!("Sending message → {target}"),
                ),
                OutboundInterAgentMessageStatus::Sent => (
                    presentation.outbound_message.sent.as_deref(),
                    format!("Sent message → {target}"),
                ),
                OutboundInterAgentMessageStatus::Failed => (
                    presentation.outbound_message.failed.as_deref(),
                    format!("Failed to send message → {target}"),
                ),
            };
            let sender = activity
                .sender_metadata
                .as_ref()
                .and_then(|metadata| metadata.display_name.as_deref())
                .or(activity.sender_agent_name.as_deref())
                .unwrap_or(&activity.sender_agent_id);
            let status = format!("{:?}", activity.status);
            let verb = render_template(
                template,
                default,
                &[
                    ("sender", sender),
                    ("recipient", target),
                    ("status", &status),
                ],
            );
            let verb = match activity.status {
                OutboundInterAgentMessageStatus::Sending => verb.cyan(),
                OutboundInterAgentMessageStatus::Sent => verb.green(),
                OutboundInterAgentMessageStatus::Failed => verb.red(),
            };
            let mut lines = vec![vec!["• ".dim(), verb].into()];
            lines.push(
                format!(
                    "  outgoing · mode={:?} · status={:?} · sequence={}",
                    activity.delivery_mode, activity.status, activity.sequence
                )
                .dim()
                .into(),
            );
            if let Some(preview) = activity.content_preview.as_deref() {
                lines.push(
                    format!("  preview: {}", bounded_preview(preview))
                        .dim()
                        .into(),
                );
            }
            if let Some(detail) = activity.detail.as_deref() {
                lines.push(format!("  {detail}").dim().into());
            }
            lines
        }
        ThreadItem::TaskAssignmentActivity { activity } => {
            task_assignment_summary_lines(activity, presentation)
        }
        ThreadItem::TaskWatchdogActivity { activity } => {
            watchdog_summary_lines(activity, presentation, template)
        }
        _ => Vec::new(),
    }
}

fn watchdog_summary_lines(
    activity: &TaskWatchdogActivityItem,
    presentation: &TuiCutexActivitySettings,
    decision: Option<&TemplateDecision>,
) -> Vec<Line<'static>> {
    let settings = watchdog_stage_settings(activity.stage, presentation);
    if settings.show == Some(false) {
        return Vec::new();
    }
    let metadata = activity.assignee_metadata.as_ref();
    let assignee = metadata
        .and_then(|metadata| metadata.display_name.as_deref())
        .unwrap_or(&activity.assignee_agent_id);
    let profile = metadata.and_then(|metadata| metadata.profile.as_deref());
    let model = metadata.and_then(|metadata| metadata.model.as_deref());
    let reasoning = metadata.and_then(|metadata| metadata.reasoning.as_deref());
    let role = metadata.and_then(|metadata| metadata.role.as_deref());
    let attempt = activity.attempt_number.to_string();
    let idle = format_idle(activity.idle_duration_secs);
    let timestamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(activity.occurred_at_ms)
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true));
    let fallback = match activity.stage {
        TaskWatchdogStage::FirstStale => format!("{assignee} appears idle for {idle}"),
        TaskWatchdogStage::DirectorEscalated => {
            format!("{assignee} is still idle after {idle}; Director escalated")
        }
    };
    let selected = match decision {
        Some(TemplateDecision::Selected(template)) => Some(template.as_str()),
        Some(TemplateDecision::SafeDefault) => None,
        None => selected_watchdog_template(settings, activity),
    };
    let mut variables = vec![
        ("assignee", assignee),
        ("assignee_id", activity.assignee_agent_id.as_str()),
        ("task", activity.task_id.as_str()),
        ("assignment", activity.assignment_id.as_str()),
        ("attempt", attempt.as_str()),
        ("idle", idle.as_str()),
        ("stage", activity.stage.event_key()),
    ];
    for (name, value) in [
        ("profile", profile),
        ("model", model),
        ("reasoning", reasoning),
        ("role", role),
        ("timestamp", timestamp.as_deref()),
    ] {
        if let Some(value) = value {
            variables.push((name, value));
        }
    }
    let rendered = render_watchdog_template(selected, fallback, &variables);
    let default_style = match activity.stage {
        TaskWatchdogStage::FirstStale => Style::default().fg(Color::Yellow),
        TaskWatchdogStage::DirectorEscalated => {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        }
    };
    let style = apply_text_style(default_style, &settings.style);
    let indent = settings.content_indent.unwrap_or(0).min(8);
    let indentation = " ".repeat(usize::from(indent));
    let rich = cutex_rich_text::render(settings.rich_template.as_deref(), &variables, style);
    let rendered_lines = rich.unwrap_or_else(|| {
        rendered
            .lines()
            .map(|text| Line::from(Span::styled(text.to_string(), style)))
            .collect()
    });
    let mut lines = rendered_lines
        .into_iter()
        .enumerate()
        .map(|(index, mut line)| {
            let prefix = if index == 0 { "• " } else { "  " };
            let mut spans = vec![prefix.dim(), Span::styled(indentation.clone(), style)];
            spans.append(&mut line.spans);
            Line::from(spans)
        })
        .collect::<Vec<_>>();
    lines.push(
        format!(
            "  task={} · assignment={} · attempt={} · activity={:?} · source_sequence={}",
            activity.task_id,
            activity.assignment_id,
            activity.attempt_number,
            activity.activity_kind,
            activity.source_sequence
        )
        .dim()
        .into(),
    );
    lines
}

fn render_watchdog_template(
    template: Option<&str>,
    fallback: String,
    variables: &[(&str, &str)],
) -> String {
    let Some(template) = template else {
        return fallback;
    };
    if template.trim().is_empty()
        || template.chars().count() > 320
        || template.lines().count() > 4
        || template
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return fallback;
    }
    let chars = template.chars().collect::<Vec<_>>();
    let mut rendered = String::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '{' if chars.get(index + 1) == Some(&'{') => {
                rendered.push('{');
                index += 2;
            }
            '}' if chars.get(index + 1) == Some(&'}') => {
                rendered.push('}');
                index += 2;
            }
            '{' => {
                let Some(relative_end) = chars[index + 1..].iter().position(|value| *value == '}')
                else {
                    return fallback;
                };
                let end = index + 1 + relative_end;
                let key = chars[index + 1..end].iter().collect::<String>();
                let Some((_, value)) = variables.iter().find(|(name, _)| *name == key) else {
                    return fallback;
                };
                rendered.push_str(value);
                index = end + 1;
            }
            '}' => return fallback,
            value => {
                rendered.push(value);
                index += 1;
            }
        }
    }
    bound_label(rendered)
}

fn watchdog_stage_settings(
    stage: TaskWatchdogStage,
    presentation: &TuiCutexActivitySettings,
) -> &TuiTaskWatchdogStagePresentation {
    match stage {
        TaskWatchdogStage::FirstStale => &presentation.task_watchdog.first_stale,
        TaskWatchdogStage::DirectorEscalated => &presentation.task_watchdog.director_escalated,
    }
}

fn selected_watchdog_template<'a>(
    settings: &'a TuiTaskWatchdogStagePresentation,
    activity: &TaskWatchdogActivityItem,
) -> Option<&'a str> {
    if let Some(template) = settings.template.as_deref() {
        return Some(template);
    }
    let candidates = if let Some(candidates) = settings.templates.as_deref() {
        candidates
    } else {
        let groups = settings.grouped_templates.as_deref()?;
        if groups.is_empty() || groups.len() > 8 || settings.selection.is_none() {
            return None;
        }
        let group = stable_watchdog_index(&activity.id, "group", groups.len());
        groups.get(group)?.templates.as_slice()
    };
    if candidates.is_empty() || candidates.len() > 8 || settings.selection.is_none() {
        return None;
    }
    candidates
        .get(stable_watchdog_index(
            &activity.id,
            activity.stage.event_key(),
            candidates.len(),
        ))
        .map(String::as_str)
}

fn stable_watchdog_index(episode_id: &str, stage: &str, len: usize) -> usize {
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    for component in [episode_id, stage] {
        hash.update(component.len().to_be_bytes());
        hash.update(component.as_bytes());
    }
    let digest = hash.finalize();
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&digest[..8]);
    u64::from_be_bytes(prefix) as usize % len
}

fn format_idle(seconds: u64) -> String {
    if seconds >= 3_600 && seconds.is_multiple_of(3_600) {
        format!("{}h", seconds / 3_600)
    } else if seconds >= 60 && seconds.is_multiple_of(60) {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn managed_default_phrase(
    operation: ManagedAgentOperation,
    status: ManagedAgentActivityStatus,
) -> &'static str {
    match (operation, status) {
        (ManagedAgentOperation::Create, ManagedAgentActivityStatus::InProgress) => {
            "Starting managed agent "
        }
        (ManagedAgentOperation::Create, ManagedAgentActivityStatus::Completed) => {
            "Started managed agent "
        }
        (ManagedAgentOperation::Create, ManagedAgentActivityStatus::Failed) => {
            "Failed to start managed agent "
        }
        (ManagedAgentOperation::QueryManaged, ManagedAgentActivityStatus::InProgress) => {
            "Querying managed agents for "
        }
        (ManagedAgentOperation::QueryManaged, ManagedAgentActivityStatus::Completed) => {
            "Queried managed agents for "
        }
        (ManagedAgentOperation::QueryManaged, ManagedAgentActivityStatus::Failed) => {
            "Failed to query managed agents for "
        }
        (ManagedAgentOperation::Online, ManagedAgentActivityStatus::InProgress) => {
            "Bringing managed agent online "
        }
        (ManagedAgentOperation::Online, ManagedAgentActivityStatus::Completed) => {
            "Brought managed agent online "
        }
        (ManagedAgentOperation::Online, ManagedAgentActivityStatus::Failed) => {
            "Failed to bring managed agent online "
        }
        (ManagedAgentOperation::Offline, ManagedAgentActivityStatus::InProgress) => {
            "Taking managed agent offline "
        }
        (ManagedAgentOperation::Offline, ManagedAgentActivityStatus::Completed) => {
            "Took managed agent offline "
        }
        (ManagedAgentOperation::Offline, ManagedAgentActivityStatus::Failed) => {
            "Failed to take managed agent offline "
        }
        (ManagedAgentOperation::Restart, ManagedAgentActivityStatus::InProgress) => {
            "Restarting managed agent "
        }
        (ManagedAgentOperation::Restart, ManagedAgentActivityStatus::Completed) => {
            "Restarted managed agent "
        }
        (ManagedAgentOperation::Restart, ManagedAgentActivityStatus::Failed) => {
            "Failed to restart managed agent "
        }
        (ManagedAgentOperation::Replace, ManagedAgentActivityStatus::InProgress) => {
            "Replacing managed agent "
        }
        (ManagedAgentOperation::Replace, ManagedAgentActivityStatus::Completed) => {
            "Replaced managed agent "
        }
        (ManagedAgentOperation::Replace, ManagedAgentActivityStatus::Failed) => {
            "Failed to replace managed agent "
        }
        (ManagedAgentOperation::Close, ManagedAgentActivityStatus::InProgress) => {
            "Closing managed agent "
        }
        (ManagedAgentOperation::Close, ManagedAgentActivityStatus::Completed) => {
            "Closed managed agent "
        }
        (ManagedAgentOperation::Close, ManagedAgentActivityStatus::Failed) => {
            "Failed to close managed agent "
        }
        (ManagedAgentOperation::DirectorRotate, ManagedAgentActivityStatus::InProgress) => {
            "Rotating Director to "
        }
        (ManagedAgentOperation::DirectorRotate, ManagedAgentActivityStatus::Completed) => {
            "Rotated Director to "
        }
        (ManagedAgentOperation::DirectorRotate, ManagedAgentActivityStatus::Failed) => {
            "Failed to rotate Director to "
        }
    }
}

const MAX_TEMPLATE_CHARS: usize = 160;
const MAX_RENDERED_TEMPLATE_CHARS: usize = 240;
const MAX_ACTIVITY_PREVIEW_CHARS: usize = 200;

fn render_template(template: Option<&str>, fallback: String, variables: &[(&str, &str)]) -> String {
    let Some(template) = template else {
        return fallback;
    };
    if template.trim().is_empty()
        || template.chars().count() > MAX_TEMPLATE_CHARS
        || template.chars().any(char::is_control)
    {
        return fallback;
    }

    let mut rendered = String::new();
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        let prefix = &remaining[..open];
        if prefix.contains('}') {
            return fallback;
        }
        rendered.push_str(prefix);
        let after_open = &remaining[open + 1..];
        let Some(close) = after_open.find('}') else {
            return fallback;
        };
        let key = &after_open[..close];
        let Some((_, value)) = variables.iter().find(|(name, _)| *name == key) else {
            return fallback;
        };
        rendered.push_str(value);
        remaining = &after_open[close + 1..];
    }
    if remaining.contains('}') {
        return fallback;
    }
    rendered.push_str(remaining);
    bound_label(rendered)
}

fn bound_label(value: String) -> String {
    if value.chars().count() <= MAX_RENDERED_TEMPLATE_CHARS {
        return value;
    }
    let mut bounded = value
        .chars()
        .take(MAX_RENDERED_TEMPLATE_CHARS.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

fn bounded_preview(value: &str) -> String {
    let flattened = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if flattened.chars().count() <= MAX_ACTIVITY_PREVIEW_CHARS {
        return flattened;
    }
    let mut bounded = flattened
        .chars()
        .take(MAX_ACTIVITY_PREVIEW_CHARS.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

fn task_status_settings(
    status: TaskAssignmentActivityStatus,
    presentation: &TuiCutexActivitySettings,
) -> &TuiTaskAssignmentStatusPresentation {
    let task = &presentation.task_assignment;
    match status {
        TaskAssignmentActivityStatus::Committed => &task.committed,
        TaskAssignmentActivityStatus::CommunicationRecorded => &task.communication_recorded,
        TaskAssignmentActivityStatus::AttemptStarted => &task.attempt_started,
        TaskAssignmentActivityStatus::AttemptAcknowledged => &task.attempt_acknowledged,
        TaskAssignmentActivityStatus::AttemptProgressed => &task.attempt_progressed,
        TaskAssignmentActivityStatus::AttemptBlocked => &task.attempt_blocked,
        TaskAssignmentActivityStatus::AttemptResumed => &task.attempt_resumed,
        TaskAssignmentActivityStatus::RetryScheduled => &task.retry_scheduled,
        TaskAssignmentActivityStatus::ReviewReady => &task.review_ready,
        TaskAssignmentActivityStatus::Completed => &task.completed,
        TaskAssignmentActivityStatus::Failed => &task.failed,
        TaskAssignmentActivityStatus::Closed => &task.closed,
        TaskAssignmentActivityStatus::Declined => &task.declined,
        TaskAssignmentActivityStatus::Aborted => &task.aborted,
    }
}

fn task_assignment_summary_lines(
    activity: &TaskAssignmentActivityItem,
    presentation: &TuiCutexActivitySettings,
) -> Vec<Line<'static>> {
    let settings = task_status_settings(activity.status, presentation);
    if settings.show == Some(false) {
        return Vec::new();
    }
    let director = activity
        .director_metadata
        .as_ref()
        .and_then(|metadata| metadata.display_name.as_deref())
        .or(activity.director_agent_name.as_deref())
        .unwrap_or(&activity.director_agent_id);
    let assignee = activity
        .assignee_metadata
        .as_ref()
        .and_then(|metadata| metadata.display_name.as_deref())
        .or(activity.assignee_agent_name.as_deref())
        .unwrap_or(&activity.assignee_agent_id);
    let status = format!("{:?}", activity.status);
    let sequence = activity.sequence.to_string();
    let title = activity.task_title.as_deref().unwrap_or_default();
    let detail = activity.detail.as_deref().unwrap_or_default();
    let variables = [
        ("director", director),
        ("assignee", assignee),
        ("task", activity.task_id.as_str()),
        ("assignment", activity.id.as_str()),
        ("status", status.as_str()),
        ("sequence", sequence.as_str()),
        ("title", title),
        ("detail", detail),
    ];
    let base_style = match activity.status {
        TaskAssignmentActivityStatus::Failed
        | TaskAssignmentActivityStatus::Declined
        | TaskAssignmentActivityStatus::Aborted => Style::default().fg(Color::Red),
        TaskAssignmentActivityStatus::Completed | TaskAssignmentActivityStatus::Closed => {
            Style::default().fg(Color::Green)
        }
        _ => Style::default().fg(Color::Cyan),
    };
    let style = apply_text_style(base_style, &settings.style);
    let rich = cutex_rich_text::render(settings.rich_template.as_deref(), &variables, style);
    let mut lines = rich.unwrap_or_else(|| {
        let state = task_status_name(activity.status);
        let fallback = format!("Task Service · {state} · {director} → {assignee}");
        render_template(settings.template.as_deref(), fallback, &variables)
            .lines()
            .map(|line| Line::from(Span::styled(line.to_string(), style)))
            .collect()
    });
    if settings.rich_template.is_none() {
        lines.push(
            format!(
                "  task={} · assignment={} · status={} · sequence={}",
                activity.task_id, activity.id, status, activity.sequence
            )
            .dim()
            .into(),
        );
    }
    if settings.show_detail.unwrap_or(true) {
        let indent = " ".repeat(usize::from(settings.content_indent.unwrap_or(2).min(8)));
        if !title.is_empty() {
            lines.push(vec![indent.clone().into(), title.to_string().dim()].into());
        }
        if !detail.is_empty() {
            lines.push(vec![indent.into(), detail.to_string().dim()].into());
        }
    }
    lines
}

fn task_status_name(status: TaskAssignmentActivityStatus) -> &'static str {
    match status {
        TaskAssignmentActivityStatus::Committed => "assigned",
        TaskAssignmentActivityStatus::CommunicationRecorded => "delivered",
        TaskAssignmentActivityStatus::AttemptStarted => "started",
        TaskAssignmentActivityStatus::AttemptAcknowledged => "acknowledged",
        TaskAssignmentActivityStatus::AttemptProgressed => "progress",
        TaskAssignmentActivityStatus::AttemptBlocked => "blocked",
        TaskAssignmentActivityStatus::AttemptResumed => "resumed",
        TaskAssignmentActivityStatus::RetryScheduled => "retry",
        TaskAssignmentActivityStatus::ReviewReady => "review ready",
        TaskAssignmentActivityStatus::Completed => "completed",
        TaskAssignmentActivityStatus::Failed => "failed",
        TaskAssignmentActivityStatus::Closed => "closed",
        TaskAssignmentActivityStatus::Declined => "declined",
        TaskAssignmentActivityStatus::Aborted => "aborted",
    }
}

fn apply_text_style(style: Style, settings: &TuiCutexTextStyle) -> Style {
    cutex_rich_text::apply_text_style(style, settings)
}

impl HistoryCell for CutexActivityHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        #[expect(clippy::expect_used)]
        let state = self.state.read().expect("Cutex activity state poisoned");
        summary_lines_at_width_with_decision(
            &state.item,
            &self.presentation,
            width,
            state.template.as_ref(),
        )
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        plain_lines(self.display_lines(u16::MAX))
    }
}

impl HistoryCell for RecoveredCutexActivityHistoryCell {
    fn display_lines(&self, _width: u16) -> Vec<Line<'static>> {
        #[expect(clippy::expect_used)]
        let state = self
            .state
            .read()
            .expect("recovered Cutex activity state poisoned");
        let Some(first) = state.entries.first() else {
            return Vec::new();
        };
        let label = match (first.delivery.class, first.delivery.recovered) {
            (CutexUiActivityDeliveryClass::CatchUp, _) => "Recovered Cutex activity · catch-up",
            (CutexUiActivityDeliveryClass::Live, true) => "Recovered Cutex activity · live retry",
            (CutexUiActivityDeliveryClass::Live, false) => "Recovered Cutex activity",
        };
        vec![Line::from(vec![
            "↳ ".cyan(),
            label.bold(),
            format!(
                " · {} of {} (ctrl + t to view transcript)",
                state.received, first.delivery.batch_size
            )
            .dim(),
        ])]
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        plain_lines(self.transcript_lines(u16::MAX))
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        #[expect(clippy::expect_used)]
        let state = self
            .state
            .read()
            .expect("recovered Cutex activity state poisoned");
        let mut lines = self.display_lines(width);
        for entry in &state.entries {
            let rendered = summary_lines_at_width_with_decision(
                &entry.item,
                &self.presentation,
                width.saturating_sub(2),
                entry.template.as_ref(),
            );
            lines.extend(crate::render::line_utils::prefix_lines(
                rendered,
                "  ".into(),
                "  ".into(),
            ));
        }
        lines
    }

    fn has_stable_transcript_height(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::CutexParticipantPresentation;
    use codex_app_server_protocol::CutexUiActivityCheckpoint;
    use codex_app_server_protocol::CutexUiActivityDeliverySchema;
    use codex_app_server_protocol::InterAgentDeliveryMode;
    use codex_app_server_protocol::ManagedAgentActivityItem;
    use codex_app_server_protocol::OutboundInterAgentMessageItem;
    use codex_app_server_protocol::TaskAssignmentActivityItem;
    use codex_app_server_protocol::TaskWatchdogActivityKind;

    fn rendered(item: &ThreadItem, presentation: &TuiCutexActivitySettings) -> String {
        summary_lines(item, presentation)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn managed_activity(status: ManagedAgentActivityStatus) -> ThreadItem {
        ThreadItem::ManagedAgentActivity {
            activity: ManagedAgentActivityItem {
                id: "action-1".into(),
                event_id: "event-1".into(),
                sequence: 1,
                occurred_at_ms: 123,
                project_id: None,
                operation: ManagedAgentOperation::Create,
                status,
                action_id: None,
                phase_event_id: None,
                phase: None,
                managed_agent_id: "cutex.worker".into(),
                managed_agent_name: Some("Agent".into()),
                managed_agent_metadata: None,
                predecessor_agent_id: None,
                predecessor_agent_name: None,
                predecessor_metadata: None,
                successor_agent_id: None,
                successor_agent_name: None,
                successor_metadata: None,
                replace_policy: None,
                rotation_mode: None,
                authority_epoch: None,
                managed_agent_role: Some("Worker".into()),
                initial_task_preview: Some("deliver the meal".into()),
                detail: None,
                runtime_generation: Some(2),
            },
        }
    }

    fn task_activity(status: TaskAssignmentActivityStatus) -> ThreadItem {
        ThreadItem::TaskAssignmentActivity {
            activity: TaskAssignmentActivityItem {
                id: "assignment-1".into(),
                event_id: "event-1".into(),
                sequence: 1,
                occurred_at_ms: 123,
                task_id: "task-1".into(),
                task_title: Some("Deliver lunch".into()),
                director_agent_id: "cutex.director".into(),
                director_agent_name: Some("Director".into()),
                director_metadata: None,
                assignee_agent_id: "cutex.worker".into(),
                assignee_agent_name: Some("Agent".into()),
                assignee_metadata: None,
                status,
                attempt_id: Some("attempt-1".into()),
                attempt_number: Some(1),
                detail: None,
            },
        }
    }

    fn watchdog_activity(stage: TaskWatchdogStage) -> ThreadItem {
        ThreadItem::TaskWatchdogActivity {
            activity: TaskWatchdogActivityItem {
                id: "episode-1".into(),
                event_id: format!("fact-{stage:?}"),
                event_key: stage.event_key().into(),
                sequence: match stage {
                    TaskWatchdogStage::FirstStale => 10,
                    TaskWatchdogStage::DirectorEscalated => 11,
                },
                occurred_at_ms: 1_787_879_400_000,
                project_id: Some("project-1".into()),
                task_id: "task-1".into(),
                task_revision: 3,
                assignment_id: "assignment-1".into(),
                attempt_number: 2,
                director_agent_id: "cutex.director".into(),
                assignee_agent_id: "cutex.worker".into(),
                assignee_metadata: Some(CutexParticipantPresentation {
                    display_name: Some("Worker One".into()),
                    cutex_session_id: Some("cutex.worker".into()),
                    profile: Some("worker".into()),
                    model: Some("gpt-5.6".into()),
                    reasoning: Some("high".into()),
                    role: Some("Worker".into()),
                    runtime_backend: Some("native".into()),
                }),
                activity_watermark: "2026-08-28T01:00:00Z".into(),
                activity_kind: TaskWatchdogActivityKind::LastToolCall,
                idle_duration_secs: match stage {
                    TaskWatchdogStage::FirstStale => 600,
                    TaskWatchdogStage::DirectorEscalated => 1_200,
                },
                stage,
                source_sequence: 41,
            },
        }
    }

    #[test]
    fn outbound_activity_is_explicitly_directional() {
        let item = ThreadItem::OutboundInterAgentMessage {
            activity: OutboundInterAgentMessageItem {
                id: "message-1".into(),
                event_id: "event-1".into(),
                sequence: 2,
                occurred_at_ms: 123,
                sender_agent_id: "director".into(),
                sender_agent_name: Some("Director".into()),
                sender_metadata: None,
                recipient_agent_id: "worker".into(),
                recipient_agent_name: Some("Worker".into()),
                recipient_metadata: None,
                other_recipient_agent_ids: Vec::new(),
                delivery_mode: InterAgentDeliveryMode::AfterTurn,
                status: OutboundInterAgentMessageStatus::Sent,
                content_preview: Some("implement the focused change".into()),
                detail: None,
            },
        };
        let rendered = rendered(&item, &TuiCutexActivitySettings::default());
        assert!(rendered.contains("Sent message → Worker"));
        assert!(rendered.contains("outgoing"));
        assert!(rendered.contains("implement the focused change"));
    }

    #[test]
    fn outbound_defaults_and_custom_labels_cover_all_statuses() {
        let base = OutboundInterAgentMessageItem {
            id: "message-1".into(),
            event_id: "event-1".into(),
            sequence: 1,
            occurred_at_ms: 123,
            sender_agent_id: "director".into(),
            sender_agent_name: Some("Director".into()),
            sender_metadata: None,
            recipient_agent_id: "worker".into(),
            recipient_agent_name: Some("Worker".into()),
            recipient_metadata: None,
            other_recipient_agent_ids: Vec::new(),
            delivery_mode: InterAgentDeliveryMode::AfterTurn,
            status: OutboundInterAgentMessageStatus::Sending,
            content_preview: None,
            detail: None,
        };
        for (status, expected) in [
            (OutboundInterAgentMessageStatus::Sending, "Sending message"),
            (OutboundInterAgentMessageStatus::Sent, "Sent message"),
            (
                OutboundInterAgentMessageStatus::Failed,
                "Failed to send message",
            ),
        ] {
            let item = ThreadItem::OutboundInterAgentMessage {
                activity: OutboundInterAgentMessageItem {
                    status,
                    ..base.clone()
                },
            };
            assert!(
                rendered(&item, &TuiCutexActivitySettings::default())
                    .contains(&format!("{expected} → Worker"))
            );
        }

        let mut customized = TuiCutexActivitySettings::default();
        customized.outbound_message.sent = Some("{sender} dispatched to {recipient}".into());
        let item = ThreadItem::OutboundInterAgentMessage {
            activity: OutboundInterAgentMessageItem {
                status: OutboundInterAgentMessageStatus::Sent,
                ..base
            },
        };
        assert!(rendered(&item, &customized).contains("Director dispatched to Worker"));
    }

    #[test]
    fn outbound_message_preview_is_flattened_and_bounded() {
        let item = ThreadItem::OutboundInterAgentMessage {
            activity: OutboundInterAgentMessageItem {
                id: "message-1".into(),
                event_id: "event-1".into(),
                sequence: 1,
                occurred_at_ms: 123,
                sender_agent_id: "director".into(),
                sender_agent_name: Some("Director".into()),
                sender_metadata: None,
                recipient_agent_id: "worker".into(),
                recipient_agent_name: Some("Worker".into()),
                recipient_metadata: None,
                other_recipient_agent_ids: Vec::new(),
                delivery_mode: InterAgentDeliveryMode::AfterTurn,
                status: OutboundInterAgentMessageStatus::Sent,
                content_preview: Some(format!("first\n{}", "x".repeat(400))),
                detail: None,
            },
        };
        let output = rendered(&item, &TuiCutexActivitySettings::default());
        let preview = output
            .lines()
            .find(|line| line.contains("preview:"))
            .expect("preview line");
        assert!(!preview.contains('\n'));
        assert!(preview.ends_with('…'));
        assert!(preview.chars().count() <= "  preview: ".chars().count() + 200);
    }

    #[test]
    fn default_and_custom_managed_labels_cover_all_statuses() {
        let defaults = TuiCutexActivitySettings::default();
        assert!(
            rendered(
                &managed_activity(ManagedAgentActivityStatus::InProgress),
                &defaults
            )
            .contains("Starting managed agent Agent")
        );
        assert!(
            rendered(
                &managed_activity(ManagedAgentActivityStatus::Completed),
                &defaults
            )
            .contains("Started managed agent Agent")
        );
        assert!(
            rendered(
                &managed_activity(ManagedAgentActivityStatus::Failed),
                &defaults
            )
            .contains("Failed to start managed agent Agent")
        );

        let mut customized = defaults;
        customized.managed_agent.completed = Some("{agent} 已取餐".into());
        let item = managed_activity(ManagedAgentActivityStatus::Completed);
        let unchanged = item.clone();
        assert!(rendered(&item, &customized).contains("Agent 已取餐"));
        assert_eq!(
            item, unchanged,
            "presentation must not mutate typed activity"
        );
        assert!(
            !serde_json::to_string(&item)
                .expect("serialize typed activity")
                .contains("已取餐")
        );
    }

    #[test]
    fn malformed_or_oversized_templates_fall_back_safely() {
        for malformed in [
            "{arbitrary_field}",
            "{agent",
            "{agent}\nrun code",
            &"x".repeat(MAX_TEMPLATE_CHARS + 1),
        ] {
            let mut presentation = TuiCutexActivitySettings::default();
            presentation.managed_agent.completed = Some(malformed.to_string());
            let output = rendered(
                &managed_activity(ManagedAgentActivityStatus::Completed),
                &presentation,
            );
            assert!(output.contains("Started managed agent Agent"));
            assert!(!output.contains("arbitrary_field"));
        }
    }

    #[test]
    fn task_assignment_defaults_and_custom_labels_cover_every_status() {
        let statuses = [
            TaskAssignmentActivityStatus::Committed,
            TaskAssignmentActivityStatus::CommunicationRecorded,
            TaskAssignmentActivityStatus::AttemptStarted,
            TaskAssignmentActivityStatus::AttemptAcknowledged,
            TaskAssignmentActivityStatus::AttemptProgressed,
            TaskAssignmentActivityStatus::AttemptBlocked,
            TaskAssignmentActivityStatus::AttemptResumed,
            TaskAssignmentActivityStatus::RetryScheduled,
            TaskAssignmentActivityStatus::ReviewReady,
            TaskAssignmentActivityStatus::Completed,
            TaskAssignmentActivityStatus::Failed,
            TaskAssignmentActivityStatus::Closed,
            TaskAssignmentActivityStatus::Declined,
            TaskAssignmentActivityStatus::Aborted,
        ];
        for status in statuses {
            let output = rendered(&task_activity(status), &TuiCutexActivitySettings::default());
            assert!(output.contains("Director → Agent"));
            assert!(output.contains(&format!("{status:?}")));
            assert!(output.contains("task=task-1 · assignment=assignment-1"));
        }

        let mut customized = TuiCutexActivitySettings::default();
        customized.task_assignment.attempt_started.template =
            Some("{assignee} accepted {task}".into());
        assert!(
            rendered(
                &task_activity(TaskAssignmentActivityStatus::AttemptStarted),
                &customized,
            )
            .contains("Agent accepted task-1")
        );

        customized.task_assignment.review_ready.rich_template = Some(vec![
            TuiCutexRichSpan {
                text: "Task Service · review ready · ".into(),
                foreground: Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98)),
                bold: Some(true),
                dim: None,
                italic: None,
            },
            TuiCutexRichSpan {
                text: "{assignee}".into(),
                foreground: Some(TuiCutexForegroundColor::Magenta),
                bold: None,
                dim: None,
                italic: Some(true),
            },
        ]);
        let lines = summary_lines(
            &task_activity(TaskAssignmentActivityStatus::ReviewReady),
            &customized,
        );
        assert!(lines[0].to_string().contains("review ready · Agent"));
        let status = lines[0]
            .spans
            .iter()
            .find(|span| span.content == "Agent")
            .expect("styled review-ready status");
        assert_eq!(status.style.fg, Some(Color::Magenta));
        assert!(status.style.add_modifier.contains(Modifier::ITALIC));
        insta::assert_snapshot!(
            lines
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            @r###"
        Task Service · review ready · Agent
          Deliver lunch
        "###
        );
    }

    #[test]
    fn activity_visibility_is_frontend_only_and_per_kind() {
        let mut presentation = TuiCutexActivitySettings {
            managed_agent_visible: false,
            ..TuiCutexActivitySettings::default()
        };
        assert!(
            summary_lines(
                &managed_activity(ManagedAgentActivityStatus::Completed),
                &presentation,
            )
            .is_empty()
        );
        assert!(
            !summary_lines(
                &task_activity(TaskAssignmentActivityStatus::Committed),
                &presentation,
            )
            .is_empty()
        );

        presentation.visible = false;
        assert!(
            summary_lines(
                &task_activity(TaskAssignmentActivityStatus::Committed),
                &presentation,
            )
            .is_empty()
        );
    }

    #[test]
    fn watchdog_stages_render_one_configurable_card_with_metadata_fallback() {
        let settings = TuiCutexActivitySettings::default();
        let first = watchdog_activity(TaskWatchdogStage::FirstStale);
        assert_eq!(
            activity_key(&first, &settings).as_deref(),
            Some("watchdog:episode-1")
        );
        assert!(!is_terminal(&first, &settings));
        let first_lines = summary_lines(&first, &settings);
        assert_eq!(first_lines[0].spans[1].style.fg, Some(Color::Yellow));
        insta::assert_snapshot!(
            first_lines.into_iter().map(|line| line.to_string()).collect::<Vec<_>>().join("\n"),
            @r###"
        • Worker One appears idle for 10m
          task=task-1 · assignment=assignment-1 · attempt=2 · activity=LastToolCall · source_sequence=41
        "###
        );

        let escalated = watchdog_activity(TaskWatchdogStage::DirectorEscalated);
        assert_eq!(
            activity_key(&escalated, &settings),
            activity_key(&first, &settings)
        );
        let escalated_lines = summary_lines(&escalated, &settings);
        assert_eq!(escalated_lines[0].spans[1].style.fg, Some(Color::Red));
        assert!(
            escalated_lines[0].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );

        let ThreadItem::TaskWatchdogActivity { mut activity } = first else {
            unreachable!()
        };
        activity.assignee_metadata = None;
        assert!(
            rendered(&ThreadItem::TaskWatchdogActivity { activity }, &settings)
                .contains("cutex.worker appears idle")
        );
    }

    #[test]
    fn watchdog_templates_styles_visibility_and_bounds_are_frontend_only() {
        let item = watchdog_activity(TaskWatchdogStage::FirstStale);
        let unchanged = item.clone();
        let colors = [
            TuiCutexForegroundColor::Black,
            TuiCutexForegroundColor::Red,
            TuiCutexForegroundColor::Green,
            TuiCutexForegroundColor::Yellow,
            TuiCutexForegroundColor::Blue,
            TuiCutexForegroundColor::Magenta,
            TuiCutexForegroundColor::Cyan,
            TuiCutexForegroundColor::Gray,
            TuiCutexForegroundColor::White,
        ];
        for color in colors {
            let mut settings = TuiCutexActivitySettings::default();
            settings.task_watchdog.first_stale.template = Some(
                "{assignee}/{assignee_id} task={task} assignment={assignment} attempt={attempt} idle={idle} stage={stage} {profile}/{model}/{reasoning}/{role}".into(),
            );
            settings.task_watchdog.first_stale.style = TuiCutexTextStyle {
                foreground: Some(color),
                bold: Some(true),
                dim: Some(true),
                italic: Some(true),
            };
            let lines = summary_lines(&item, &settings);
            assert!(lines[0].to_string().contains("Worker One/cutex.worker"));
            assert!(
                lines[0].spans[1]
                    .style
                    .add_modifier
                    .contains(Modifier::ITALIC)
            );
        }
        assert_eq!(item, unchanged, "presentation never mutates typed facts");

        let mut hidden = TuiCutexActivitySettings::default();
        hidden.task_watchdog.visible = false;
        assert!(summary_lines(&item, &hidden).is_empty());
        hidden.task_watchdog.visible = true;
        hidden.task_watchdog.first_stale.show = Some(false);
        assert!(summary_lines(&item, &hidden).is_empty());

        let mut unsafe_template = TuiCutexActivitySettings::default();
        unsafe_template.task_watchdog.first_stale.template = Some("{unknown}".into());
        assert!(rendered(&item, &unsafe_template).contains("appears idle"));

        unsafe_template.task_watchdog.first_stale.template =
            Some(format!("{{{{watchdog}}}} {}", "x".repeat(170)));
        let bounded = rendered(&item, &unsafe_template);
        assert!(bounded.contains("{watchdog}"));
        assert!(!bounded.contains("appears idle"));
    }

    #[test]
    fn watchdog_multiline_template_preserves_safe_layout_indent_and_style() {
        let item = watchdog_activity(TaskWatchdogStage::FirstStale);
        let mut settings = TuiCutexActivitySettings::default();
        settings.task_watchdog.first_stale.template =
            Some("{assignee} has been idle\nTask {task}, attempt {attempt}".into());
        settings.task_watchdog.first_stale.content_indent = Some(2);
        settings.task_watchdog.first_stale.style = TuiCutexTextStyle {
            foreground: Some(TuiCutexForegroundColor::Magenta),
            bold: Some(true),
            dim: None,
            italic: Some(true),
        };
        let lines = summary_lines(&item, &settings);
        assert_eq!(lines[0].to_string(), "•   Worker One has been idle");
        assert_eq!(lines[1].to_string(), "    Task task-1, attempt 2");
        for line in &lines[..2] {
            assert_eq!(line.spans[1].style.fg, Some(Color::Magenta));
            assert!(line.spans[1].style.add_modifier.contains(Modifier::BOLD));
            assert!(line.spans[1].style.add_modifier.contains(Modifier::ITALIC));
        }
        insta::assert_snapshot!(
            lines.into_iter().map(|line| line.to_string()).collect::<Vec<_>>().join("\n"),
            @r###"
        •   Worker One has been idle
            Task task-1, attempt 2
          task=task-1 · assignment=assignment-1 · attempt=2 · activity=LastToolCall · source_sequence=41
        "###
        );

        settings.task_watchdog.first_stale.template = Some("safe\tunsafe".into());
        assert!(rendered(&item, &settings).contains("appears idle"));
    }

    #[test]
    fn watchdog_rich_template_uses_authoritative_utc_fact_timestamp() {
        use codex_config::types::TuiCutexRichSpan;

        let item = watchdog_activity(TaskWatchdogStage::DirectorEscalated);
        let mut settings = TuiCutexActivitySettings::default();
        settings.task_watchdog.director_escalated.rich_template = Some(vec![
            TuiCutexRichSpan {
                text: "task_service: ".into(),
                foreground: None,
                bold: None,
                dim: None,
                italic: None,
            },
            TuiCutexRichSpan {
                text: "我等了很久，我不会再等了。".into(),
                foreground: Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98)),
                bold: None,
                dim: None,
                italic: None,
            },
            TuiCutexRichSpan {
                text: "\nidle-agent: {assignee} idle-time: {idle} timestamp: {timestamp}".into(),
                foreground: Some(TuiCutexForegroundColor::Rgb(0xD3, 0xD3, 0xD3)),
                bold: Some(false),
                dim: None,
                italic: None,
            },
        ]);
        let lines = summary_lines(&item, &settings);
        assert_eq!(
            lines[0].to_string(),
            "• task_service: 我等了很久，我不会再等了。"
        );
        assert_eq!(
            lines[1].to_string(),
            "  idle-agent: Worker One idle-time: 20m timestamp: 2026-08-28T01:10:00.000Z"
        );
        assert_eq!(
            lines[0].spans[3].style.fg,
            Some(crate::terminal_palette::rgb_color((0x98, 0xFF, 0x98)))
        );
        assert_eq!(
            lines[1].spans[2].style.fg,
            Some(crate::terminal_palette::rgb_color((0xD3, 0xD3, 0xD3)))
        );
        assert!(
            !lines[1].spans[2]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );

        let ThreadItem::TaskWatchdogActivity { mut activity } = item else {
            unreachable!()
        };
        activity.occurred_at_ms = i64::MAX;
        let fallback = rendered(&ThreadItem::TaskWatchdogActivity { activity }, &settings);
        assert!(fallback.contains("Director escalated"));
        assert!(!fallback.contains("timestamp:"));
    }

    #[test]
    fn watchdog_live_cell_updates_in_place_without_changing_original_event_time_key() {
        let first = watchdog_activity(TaskWatchdogStage::FirstStale);
        let (cell, handle) =
            new_activity_cell(first.clone(), None, TuiCutexActivitySettings::default());
        let escalated = watchdog_activity(TaskWatchdogStage::DirectorEscalated);
        handle.update(escalated, None);
        assert!(
            cell.display_lines(100)[0]
                .to_string()
                .contains("Director escalated")
        );
        assert_eq!(occurred_at_ms(&first), Some(1_787_879_400_000));
    }

    #[test]
    fn one_live_cell_updates_from_sending_to_sent() {
        let item = ThreadItem::OutboundInterAgentMessage {
            activity: OutboundInterAgentMessageItem {
                id: "message-1".into(),
                event_id: "event-1".into(),
                sequence: 1,
                occurred_at_ms: 123,
                sender_agent_id: "director".into(),
                sender_agent_name: Some("Director".into()),
                sender_metadata: None,
                recipient_agent_id: "worker".into(),
                recipient_agent_name: Some("Worker".into()),
                recipient_metadata: None,
                other_recipient_agent_ids: Vec::new(),
                delivery_mode: InterAgentDeliveryMode::Soon,
                status: OutboundInterAgentMessageStatus::Sending,
                content_preview: Some("focus the implementation".into()),
                detail: None,
            },
        };
        let (cell, handle) =
            new_activity_cell(item.clone(), None, TuiCutexActivitySettings::default());
        assert!(
            cell.display_lines(80)[0]
                .to_string()
                .contains("Sending message")
        );

        let ThreadItem::OutboundInterAgentMessage { mut activity } = item else {
            unreachable!()
        };
        activity.event_id = "event-2".into();
        activity.sequence = 2;
        activity.status = OutboundInterAgentMessageStatus::Sent;
        handle.update(ThreadItem::OutboundInterAgentMessage { activity }, None);

        assert!(
            cell.display_lines(80)[0]
                .to_string()
                .contains("Sent message")
        );
    }

    #[test]
    fn recovered_batch_is_distinct_expandable_ordered_and_coalesces_assignment_lifecycle() {
        fn delivery(batch_index: u32, sequence: u64) -> CutexUiActivityDelivery {
            CutexUiActivityDelivery {
                schema: CutexUiActivityDeliverySchema::V1,
                class: CutexUiActivityDeliveryClass::CatchUp,
                recovered: false,
                batch_id: "catch_up:management-v2:10:13".into(),
                batch_index,
                batch_size: 4,
                source_checkpoint: CutexUiActivityCheckpoint {
                    stream_id: "management-v2".into(),
                    cursor: format!("cursor-{sequence}"),
                    sequence,
                },
                batch_checkpoint: CutexUiActivityCheckpoint {
                    stream_id: "management-v2".into(),
                    cursor: "cursor-13".into(),
                    sequence: 13,
                },
            }
        }

        let committed = task_activity(TaskAssignmentActivityStatus::Committed);
        let (cell, handle) = new_recovered_activity_cell(
            "task:assignment-1".into(),
            delivery(0, 10),
            committed,
            None,
            TuiCutexActivitySettings::default(),
        );

        let mut acknowledged = task_activity(TaskAssignmentActivityStatus::AttemptAcknowledged);
        let ThreadItem::TaskAssignmentActivity { activity } = &mut acknowledged else {
            unreachable!()
        };
        activity.event_id = "event-11".into();
        activity.sequence = 11;
        activity.occurred_at_ms = 111;
        handle.update(
            "task:assignment-1".into(),
            delivery(1, 11),
            acknowledged,
            None,
        );

        let mut managed = managed_activity(ManagedAgentActivityStatus::Completed);
        let ThreadItem::ManagedAgentActivity { activity } = &mut managed else {
            unreachable!()
        };
        activity.event_id = "event-12".into();
        activity.sequence = 12;
        activity.occurred_at_ms = 112;
        handle.update("managed:action-1".into(), delivery(2, 12), managed, None);

        let mut started = task_activity(TaskAssignmentActivityStatus::AttemptStarted);
        let ThreadItem::TaskAssignmentActivity { activity } = &mut started else {
            unreachable!()
        };
        activity.event_id = "event-13".into();
        activity.sequence = 13;
        activity.occurred_at_ms = 113;
        handle.update("task:assignment-1".into(), delivery(3, 13), started, None);

        let compact = cell
            .display_lines(100)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let expanded = cell
            .transcript_lines(100)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!expanded.contains("Committed task"));
        insta::assert_snapshot!(&compact, @r###"
        ↳ Recovered Cutex activity · catch-up · 4 of 4 (ctrl + t to view transcript)
        "###);
        insta::assert_snapshot!("recovered_cutex_batch_expanded", expanded);
    }
}
