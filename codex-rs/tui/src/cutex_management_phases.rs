//! Live-only presentation for authoritative Cutex Agent Management phase facts.

use codex_app_server_protocol::AgentManagementPhase;
use codex_app_server_protocol::CutexParticipantPresentation;
use codex_app_server_protocol::ManagedAgentActivityItem;
use codex_app_server_protocol::ManagedAgentOperation;
use codex_config::types::TuiAgentManagementOperationPresentation;
use codex_config::types::TuiAgentManagementPhaseDisplay;
use codex_config::types::TuiAgentManagementPhasePresentation;
use codex_config::types::TuiCutexActivitySettings;
#[cfg(test)]
use codex_config::types::TuiCutexForegroundColor;
use ratatui::style::Color;
#[cfg(test)]
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::text::Span;
use sha2::Digest;
use sha2::Sha256;
use unicode_width::UnicodeWidthStr;

use crate::cutex_rich_text;
use crate::cutex_template_groups::TemplateDecision;

const MAX_TEMPLATE_CHARS: usize = 320;
const MAX_TEMPLATE_LINES: usize = 4;
const MAX_RENDERED_CHARS: usize = 512;
const MAX_RENDERED_LINES: usize = 8;
const MAX_INDENT: u8 = 8;
const MIN_WRAP_WIDTH: u16 = 12;
const MAX_WRAP_WIDTH: u16 = 160;

pub(crate) fn display_mode(
    activity: &ManagedAgentActivityItem,
    presentation: &TuiCutexActivitySettings,
) -> TuiAgentManagementPhaseDisplay {
    operation_presentation(activity.operation, presentation)
        .phase_display
        .unwrap_or(presentation.phase_display)
}

pub(crate) fn is_shown(
    activity: &ManagedAgentActivityItem,
    presentation: &TuiCutexActivitySettings,
) -> bool {
    let Some(phase) = activity.phase else {
        return true;
    };
    phase_presentation(
        operation_presentation(activity.operation, presentation),
        phase,
    )
    .and_then(|settings| settings.show)
    .unwrap_or(true)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn summary_lines(
    activity: &ManagedAgentActivityItem,
    presentation: &TuiCutexActivitySettings,
    width: u16,
) -> Vec<Line<'static>> {
    summary_lines_with_decision(activity, presentation, width, None)
}

pub(crate) fn summary_lines_with_decision(
    activity: &ManagedAgentActivityItem,
    presentation: &TuiCutexActivitySettings,
    width: u16,
    decision: Option<&TemplateDecision>,
) -> Vec<Line<'static>> {
    let Some(phase) = activity.phase else {
        return Vec::new();
    };
    let settings = phase_presentation(
        operation_presentation(activity.operation, presentation),
        phase,
    );
    if settings.and_then(|settings| settings.show) == Some(false) {
        return Vec::new();
    }

    let fallback = default_label(activity, phase);
    let variables = template_variables(activity, phase);
    let rendered = bound_rendered(render_template(
        match decision {
            Some(TemplateDecision::Selected(template)) => Some(template.as_str()),
            Some(TemplateDecision::SafeDefault) => None,
            None => selected_template(settings, activity, phase),
        },
        &fallback,
        &variables,
    ));
    let indent = settings
        .and_then(|settings| settings.indent)
        .filter(|indent| *indent <= MAX_INDENT)
        .unwrap_or(0);
    let wrap_width = settings
        .and_then(|settings| settings.wrap_width)
        .filter(|width| (MIN_WRAP_WIDTH..=MAX_WRAP_WIDTH).contains(width))
        .unwrap_or(width.clamp(MIN_WRAP_WIDTH, MAX_WRAP_WIDTH))
        .min(width.max(/*other*/ 1));
    let content_width = usize::from(wrap_width)
        .saturating_sub(2 + usize::from(indent))
        .max(1);
    let style = phase_style(phase, settings);
    let rich_variables = variables
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect::<Vec<_>>();
    let rich = cutex_rich_text::render(
        settings.and_then(|settings| settings.rich_template.as_deref()),
        &rich_variables,
        style,
    );
    let wrapped = if let Some(lines) = rich {
        cutex_rich_text::wrap(lines, content_width)
    } else {
        let mut wrapped = Vec::new();
        for source_line in rendered.lines() {
            let source_line = if source_line.is_empty() {
                " "
            } else {
                source_line
            };
            for line in textwrap::wrap(source_line, content_width) {
                wrapped.push(Line::from(Span::styled(line.into_owned(), style)));
            }
        }
        if wrapped.len() > MAX_RENDERED_LINES {
            wrapped.truncate(MAX_RENDERED_LINES);
            if let Some(last) = wrapped.last_mut() {
                let mut text = last.to_string();
                truncate_with_ellipsis(&mut text, content_width);
                *last = Line::from(Span::styled(text, style));
            }
        }
        wrapped
    };

    let indentation = " ".repeat(usize::from(indent));
    wrapped
        .into_iter()
        .enumerate()
        .map(|(index, mut line)| {
            let prefix = if index == 0 { "• " } else { "  " };
            let mut spans = vec![prefix.dim(), Span::styled(indentation.clone(), style)];
            spans.append(&mut line.spans);
            Line::from(spans)
        })
        .collect()
}

fn selected_template<'a>(
    settings: Option<&'a TuiAgentManagementPhasePresentation>,
    activity: &ManagedAgentActivityItem,
    phase: AgentManagementPhase,
) -> Option<&'a str> {
    let settings = settings?;
    if settings.template.is_some() && settings.templates.is_some() {
        return None;
    }
    if let Some(template) = settings.template.as_deref() {
        return Some(template);
    }
    let templates = settings.templates.as_deref()?;
    if templates.is_empty() || templates.len() > 8 || settings.selection.is_none() {
        return None;
    }
    let mut hash = Sha256::new();
    // Frontend-local contract: length-framed event/action identity, operation, phase, then the
    // ordered candidate list. A redraw or duplicate delivery therefore cannot reroll a phase.
    // TODO: standardize this seed across frontends if the protocol later carries one.
    for component in [
        activity.event_id.as_str(),
        activity.phase_event_id.as_deref().unwrap_or_default(),
        activity.action_id.as_deref().unwrap_or(&activity.id),
        operation_name(activity.operation),
        phase_name(phase),
    ] {
        hash.update(component.len().to_be_bytes());
        hash.update(component.as_bytes());
    }
    for candidate in templates {
        hash.update(candidate.len().to_be_bytes());
        hash.update(candidate.as_bytes());
    }
    let digest = hash.finalize();
    let mut prefix = [0_u8; 8];
    prefix.copy_from_slice(&digest[..8]);
    let index = u64::from_be_bytes(prefix) as usize % templates.len();
    templates.get(index).map(String::as_str)
}

pub(crate) fn operation_name(operation: ManagedAgentOperation) -> &'static str {
    match operation {
        ManagedAgentOperation::Create => "create",
        ManagedAgentOperation::QueryManaged => "query_managed",
        ManagedAgentOperation::Online => "online",
        ManagedAgentOperation::Offline => "offline",
        ManagedAgentOperation::Restart => "restart",
        ManagedAgentOperation::Close => "close",
        ManagedAgentOperation::Replace => "replace",
        ManagedAgentOperation::DirectorRotate => "director_rotate",
    }
}

fn operation_presentation(
    operation: ManagedAgentOperation,
    presentation: &TuiCutexActivitySettings,
) -> &TuiAgentManagementOperationPresentation {
    match operation {
        ManagedAgentOperation::Create => &presentation.create,
        ManagedAgentOperation::QueryManaged => &presentation.query_managed,
        ManagedAgentOperation::Online => &presentation.online,
        ManagedAgentOperation::Offline => &presentation.offline,
        ManagedAgentOperation::Restart => &presentation.restart,
        ManagedAgentOperation::Close => &presentation.close,
        ManagedAgentOperation::Replace => &presentation.replace,
        ManagedAgentOperation::DirectorRotate => &presentation.director_rotate,
    }
}

pub(crate) fn phase_settings<'a>(
    activity: &ManagedAgentActivityItem,
    presentation: &'a TuiCutexActivitySettings,
) -> Option<&'a TuiAgentManagementPhasePresentation> {
    activity.phase.and_then(|phase| {
        phase_presentation(
            operation_presentation(activity.operation, presentation),
            phase,
        )
    })
}

fn phase_presentation(
    operation: &TuiAgentManagementOperationPresentation,
    phase: AgentManagementPhase,
) -> Option<&TuiAgentManagementPhasePresentation> {
    match phase {
        AgentManagementPhase::Prepared => operation.prepared.as_ref(),
        AgentManagementPhase::PrivateCwdReady => operation.private_cwd_ready.as_ref(),
        AgentManagementPhase::NativeBootstrapPending => operation.native_bootstrap_pending.as_ref(),
        AgentManagementPhase::NativeSessionCaptured => operation.native_session_captured.as_ref(),
        AgentManagementPhase::Adopted => operation.adopted.as_ref(),
        AgentManagementPhase::Configured => operation.configured.as_ref(),
        AgentManagementPhase::Online => operation.online.as_ref(),
        AgentManagementPhase::Ready => operation.ready.as_ref(),
        AgentManagementPhase::MessagePending => operation.message_pending.as_ref(),
        AgentManagementPhase::MessageQueued => operation.message_queued.as_ref(),
        AgentManagementPhase::PredecessorClosing => operation.predecessor_closing.as_ref(),
        AgentManagementPhase::PredecessorClosed => operation.predecessor_closed.as_ref(),
        AgentManagementPhase::AuthorityTransferPending => {
            operation.authority_transfer_pending.as_ref()
        }
        AgentManagementPhase::AuthorityTransferred => operation.authority_transferred.as_ref(),
        AgentManagementPhase::SuccessorReady => operation.successor_ready.as_ref(),
        AgentManagementPhase::Complete => operation.complete.as_ref(),
        AgentManagementPhase::NoWrite => operation.no_write.as_ref(),
        AgentManagementPhase::OwnerActionRequired => operation.owner_action_required.as_ref(),
        AgentManagementPhase::Failure => operation.failure.as_ref(),
    }
}

fn default_label(activity: &ManagedAgentActivityItem, phase: AgentManagementPhase) -> String {
    let target = participant_name(
        activity.managed_agent_metadata.as_ref(),
        activity.managed_agent_name.as_deref(),
        Some(&activity.managed_agent_id),
    )
    .map(safe_placeholder_value)
    .unwrap_or_else(|| "managed agent".to_string());
    let predecessor = participant_name(
        activity.predecessor_metadata.as_ref(),
        activity.predecessor_agent_name.as_deref(),
        activity.predecessor_agent_id.as_deref(),
    )
    .map(safe_placeholder_value)
    .unwrap_or_else(|| target.clone());
    let successor = participant_name(
        activity.successor_metadata.as_ref(),
        activity.successor_agent_name.as_deref(),
        activity.successor_agent_id.as_deref(),
    )
    .map(safe_placeholder_value)
    .unwrap_or_else(|| target.clone());
    match phase {
        AgentManagementPhase::Prepared => format!("Prepared Agent Management action for {target}"),
        AgentManagementPhase::PrivateCwdReady => format!("Private workspace ready for {target}"),
        AgentManagementPhase::NativeBootstrapPending => {
            format!("Native bootstrap pending for {target}")
        }
        AgentManagementPhase::NativeSessionCaptured => {
            format!("Native session captured for {target}")
        }
        AgentManagementPhase::Adopted => format!("Adopted managed agent {target}"),
        AgentManagementPhase::Configured => format!("Configured managed agent {target}"),
        AgentManagementPhase::Online => format!("Managed agent online {target}"),
        AgentManagementPhase::Ready => format!("Managed agent ready {target}"),
        AgentManagementPhase::MessagePending => format!("Message pending for {target}"),
        AgentManagementPhase::MessageQueued => format!("Message queued for {target}"),
        AgentManagementPhase::PredecessorClosing => format!("Closing predecessor {predecessor}"),
        AgentManagementPhase::PredecessorClosed => format!("Predecessor closed {predecessor}"),
        AgentManagementPhase::AuthorityTransferPending => {
            format!("Authority transfer pending for {successor}")
        }
        AgentManagementPhase::AuthorityTransferred => {
            format!("Authority transferred to {successor}")
        }
        AgentManagementPhase::SuccessorReady => format!("Successor ready {successor}"),
        AgentManagementPhase::Complete => format!("Agent Management action complete for {target}"),
        AgentManagementPhase::NoWrite => format!("Agent Management made no change for {target}"),
        AgentManagementPhase::OwnerActionRequired => format!("Owner action required for {target}"),
        AgentManagementPhase::Failure => format!("Agent Management action failed for {target}"),
    }
}

fn template_variables(
    activity: &ManagedAgentActivityItem,
    phase: AgentManagementPhase,
) -> Vec<(&'static str, String)> {
    let mut variables = Vec::new();
    if let Some(value) = participant_name(
        activity.managed_agent_metadata.as_ref(),
        activity.managed_agent_name.as_deref(),
        Some(&activity.managed_agent_id),
    ) {
        variables.push(("agent_name", safe_placeholder_value(value)));
    }
    if let Some(value) = participant_name(
        activity.predecessor_metadata.as_ref(),
        activity.predecessor_agent_name.as_deref(),
        activity.predecessor_agent_id.as_deref(),
    ) {
        variables.push(("predecessor_name", safe_placeholder_value(value)));
    }
    if let Some(value) = participant_name(
        activity.successor_metadata.as_ref(),
        activity.successor_agent_name.as_deref(),
        activity.successor_agent_id.as_deref(),
    ) {
        variables.push(("successor_name", safe_placeholder_value(value)));
    }
    if let Some(value) = activity.predecessor_agent_id.as_ref() {
        variables.push(("predecessor_id", safe_placeholder_value(value)));
    }
    if let Some(value) = activity.successor_agent_id.as_ref() {
        variables.push(("successor_id", safe_placeholder_value(value)));
    }
    variables.push(("phase", phase_name(phase).to_string()));
    if let Some(value) = activity.authority_epoch {
        variables.push(("authority_epoch", value.to_string()));
    }
    variables
}

fn participant_name<'a>(
    metadata: Option<&'a CutexParticipantPresentation>,
    name: Option<&'a str>,
    id: Option<&'a str>,
) -> Option<&'a str> {
    metadata
        .and_then(|metadata| metadata.display_name.as_deref())
        .filter(|value| !value.trim().is_empty())
        .or(name)
        .filter(|value| !value.trim().is_empty())
        .or(id)
        .filter(|value| !value.trim().is_empty())
}

fn safe_placeholder_value(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() && character.is_whitespace() {
                ' '
            } else if character.is_control() {
                '�'
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_template(template: Option<&str>, fallback: &str, variables: &[(&str, String)]) -> String {
    let Some(template) = template else {
        return fallback.to_string();
    };
    if template.trim().is_empty()
        || template.chars().count() > MAX_TEMPLATE_CHARS
        || template.lines().count() > MAX_TEMPLATE_LINES
        || template
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return fallback.to_string();
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
                    return fallback.to_string();
                };
                let end = index + 1 + relative_end;
                let key = chars[index + 1..end].iter().collect::<String>();
                let Some((_, value)) = variables.iter().find(|(name, _)| *name == key) else {
                    return fallback.to_string();
                };
                rendered.push_str(value);
                index = end + 1;
            }
            '}' => return fallback.to_string(),
            character => {
                rendered.push(character);
                index += 1;
            }
        }
        if rendered.chars().count() > MAX_RENDERED_CHARS {
            return truncate_rendered(rendered);
        }
    }
    rendered
}

fn truncate_rendered(value: String) -> String {
    let mut bounded = value
        .chars()
        .take(MAX_RENDERED_CHARS.saturating_sub(1))
        .collect::<String>();
    bounded.push('…');
    bounded
}

fn bound_rendered(value: String) -> String {
    if value.chars().count() <= MAX_RENDERED_CHARS {
        value
    } else {
        truncate_rendered(value)
    }
}

fn truncate_with_ellipsis(value: &mut String, width: usize) {
    while UnicodeWidthStr::width(value.as_str()) >= width {
        value.pop();
    }
    value.push('…');
}

fn phase_style(
    phase: AgentManagementPhase,
    settings: Option<&TuiAgentManagementPhasePresentation>,
) -> Style {
    let default_color = match phase {
        AgentManagementPhase::Complete
        | AgentManagementPhase::Ready
        | AgentManagementPhase::AuthorityTransferred
        | AgentManagementPhase::SuccessorReady => Color::Green,
        AgentManagementPhase::PredecessorClosing
        | AgentManagementPhase::PredecessorClosed
        | AgentManagementPhase::NoWrite
        | AgentManagementPhase::OwnerActionRequired
        | AgentManagementPhase::Failure => Color::Red,
        _ => Color::Cyan,
    };
    let Some(settings) = settings else {
        return Style::default().fg(default_color);
    };
    cutex_rich_text::apply_text_style(
        Style::default().fg(default_color),
        &codex_config::types::TuiCutexTextStyle {
            foreground: settings.foreground,
            bold: settings.bold,
            dim: settings.dim,
            italic: settings.italic,
        },
    )
}

pub(crate) fn phase_name(phase: AgentManagementPhase) -> &'static str {
    match phase {
        AgentManagementPhase::Prepared => "prepared",
        AgentManagementPhase::PrivateCwdReady => "private_cwd_ready",
        AgentManagementPhase::NativeBootstrapPending => "native_bootstrap_pending",
        AgentManagementPhase::NativeSessionCaptured => "native_session_captured",
        AgentManagementPhase::Adopted => "adopted",
        AgentManagementPhase::Configured => "configured",
        AgentManagementPhase::Online => "online",
        AgentManagementPhase::Ready => "ready",
        AgentManagementPhase::MessagePending => "message_pending",
        AgentManagementPhase::MessageQueued => "message_queued",
        AgentManagementPhase::PredecessorClosing => "predecessor_closing",
        AgentManagementPhase::PredecessorClosed => "predecessor_closed",
        AgentManagementPhase::AuthorityTransferPending => "authority_transfer_pending",
        AgentManagementPhase::AuthorityTransferred => "authority_transferred",
        AgentManagementPhase::SuccessorReady => "successor_ready",
        AgentManagementPhase::Complete => "complete",
        AgentManagementPhase::NoWrite => "no_write",
        AgentManagementPhase::OwnerActionRequired => "owner_action_required",
        AgentManagementPhase::Failure => "failure",
    }
}

#[cfg(test)]
#[path = "cutex_management_phases_tests.rs"]
mod tests;
