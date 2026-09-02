use crate::types::TuiAgentManagementOperationPresentation;
use crate::types::TuiAgentManagementPhasePresentation;
use crate::types::TuiCutexActivitySettings;
use crate::types::TuiCutexGroupedTemplates;
use crate::types::TuiCutexRichSpan;
use crate::types::TuiTaskServiceMessagePresentation;
use crate::types::TuiTaskWatchdogStagePresentation;

const MAX_TEMPLATE_CHARS: usize = 320;
const MAX_TEMPLATE_LINES: usize = 4;
const MAX_TEMPLATE_CANDIDATES: usize = 8;
const MAX_TEMPLATE_GROUPS: usize = 8;
const MAX_TEMPLATE_AGGREGATE_BYTES: usize = 4_096;
const MAX_RICH_SPANS: usize = 16;
const MAX_RICH_CONFIGURED_CHARS: usize = 1_024;
const MAX_RICH_LINES: usize = 8;
const MAX_INDENT: u8 = 8;
const MIN_WRAP_WIDTH: u16 = 12;
const MAX_WRAP_WIDTH: u16 = 160;

/// Strictly validates frontend-owned Cutex presentation configuration before it is used.
pub fn validate_cutex_tui_settings(settings: &TuiCutexActivitySettings) -> Result<(), String> {
    validate_template(
        settings.inbound_message.label.as_deref(),
        &[
            "author",
            "recipient",
            "mode",
            "profile",
            "model",
            "reasoning",
            "role",
            "runtime_backend",
        ],
    )?;
    if settings.inbound_message.content_indent > MAX_INDENT {
        return Err("inbound_message.content_indent exceeds 8".to_string());
    }

    for template in [
        settings.managed_agent.in_progress.as_deref(),
        settings.managed_agent.completed.as_deref(),
        settings.managed_agent.failed.as_deref(),
    ] {
        validate_template(template, &["agent", "operation", "status"])?;
    }
    for template in [
        settings.outbound_message.sending.as_deref(),
        settings.outbound_message.sent.as_deref(),
        settings.outbound_message.failed.as_deref(),
    ] {
        validate_template(template, &["sender", "recipient", "status"])?;
    }
    for presentation in task_assignment_presentations(settings) {
        validate_variants(
            presentation.template.as_deref(),
            None,
            None,
            presentation.rich_template.as_deref(),
            false,
            &[
                "director",
                "assignee",
                "task",
                "assignment",
                "status",
                "sequence",
                "title",
                "detail",
            ],
        )?;
        if presentation
            .content_indent
            .is_some_and(|value| value > MAX_INDENT)
        {
            return Err("Task assignment content_indent exceeds 8".to_string());
        }
    }

    for operation in operations(settings) {
        validate_operation(operation)?;
    }
    for presentation in task_service_presentations(settings) {
        validate_variants(
            presentation.template.as_deref(),
            presentation.templates.as_deref(),
            presentation.grouped_templates.as_deref(),
            presentation.rich_template.as_deref(),
            presentation.selection.is_some(),
            &["task_name", "assignment_id", "transition", "payload"],
        )?;
        if presentation
            .content_indent
            .is_some_and(|value| value > MAX_INDENT)
        {
            return Err("Task Service content_indent exceeds 8".to_string());
        }
    }
    for presentation in watchdog_presentations(settings) {
        validate_variants(
            presentation.template.as_deref(),
            presentation.templates.as_deref(),
            presentation.grouped_templates.as_deref(),
            presentation.rich_template.as_deref(),
            presentation.selection.is_some(),
            &[
                "assignee",
                "assignee_id",
                "task",
                "assignment",
                "attempt",
                "idle",
                "stage",
                "profile",
                "model",
                "reasoning",
                "role",
                "timestamp",
            ],
        )?;
        if presentation
            .content_indent
            .is_some_and(|value| value > MAX_INDENT)
        {
            return Err("Task watchdog content_indent exceeds 8".to_string());
        }
    }
    validate_grouped_continuations(settings)?;
    Ok(())
}

fn operations(
    settings: &TuiCutexActivitySettings,
) -> [&TuiAgentManagementOperationPresentation; 8] {
    [
        &settings.create,
        &settings.query_managed,
        &settings.online,
        &settings.offline,
        &settings.restart,
        &settings.close,
        &settings.replace,
        &settings.director_rotate,
    ]
}

fn validate_operation(operation: &TuiAgentManagementOperationPresentation) -> Result<(), String> {
    for phase in phases(operation).into_iter().flatten() {
        validate_phase(phase)?;
    }
    Ok(())
}

fn phases(
    operation: &TuiAgentManagementOperationPresentation,
) -> [Option<&TuiAgentManagementPhasePresentation>; 19] {
    [
        operation.prepared.as_ref(),
        operation.private_cwd_ready.as_ref(),
        operation.native_bootstrap_pending.as_ref(),
        operation.native_session_captured.as_ref(),
        operation.adopted.as_ref(),
        operation.configured.as_ref(),
        operation.online.as_ref(),
        operation.ready.as_ref(),
        operation.message_pending.as_ref(),
        operation.message_queued.as_ref(),
        operation.predecessor_closing.as_ref(),
        operation.predecessor_closed.as_ref(),
        operation.authority_transfer_pending.as_ref(),
        operation.authority_transferred.as_ref(),
        operation.successor_ready.as_ref(),
        operation.complete.as_ref(),
        operation.no_write.as_ref(),
        operation.owner_action_required.as_ref(),
        operation.failure.as_ref(),
    ]
}

fn validate_phase(phase: &TuiAgentManagementPhasePresentation) -> Result<(), String> {
    validate_variants(
        phase.template.as_deref(),
        phase.templates.as_deref(),
        phase.grouped_templates.as_deref(),
        phase.rich_template.as_deref(),
        phase.selection.is_some(),
        &[
            "agent_name",
            "predecessor_name",
            "successor_name",
            "predecessor_id",
            "successor_id",
            "phase",
            "authority_epoch",
        ],
    )?;
    if phase.indent.is_some_and(|value| value > MAX_INDENT) {
        return Err("phase indent exceeds 8".to_string());
    }
    if phase
        .wrap_width
        .is_some_and(|value| !(MIN_WRAP_WIDTH..=MAX_WRAP_WIDTH).contains(&value))
    {
        return Err("phase wrap_width must be between 12 and 160".to_string());
    }
    Ok(())
}

fn task_service_presentations(
    settings: &TuiCutexActivitySettings,
) -> [&TuiTaskServiceMessagePresentation; 7] {
    let messages = &settings.task_service_message;
    [
        &messages.assignment,
        &messages.progress,
        &messages.blocked,
        &messages.resumed,
        &messages.review_ready,
        &messages.retry,
        &messages.terminal_closure,
    ]
}

fn task_assignment_presentations(
    settings: &TuiCutexActivitySettings,
) -> [&crate::types::TuiTaskAssignmentStatusPresentation; 14] {
    let task = &settings.task_assignment;
    [
        &task.committed,
        &task.communication_recorded,
        &task.attempt_started,
        &task.attempt_acknowledged,
        &task.attempt_progressed,
        &task.attempt_blocked,
        &task.attempt_resumed,
        &task.retry_scheduled,
        &task.review_ready,
        &task.completed,
        &task.failed,
        &task.closed,
        &task.declined,
        &task.aborted,
    ]
}

fn watchdog_presentations(
    settings: &TuiCutexActivitySettings,
) -> [&TuiTaskWatchdogStagePresentation; 2] {
    [
        &settings.task_watchdog.first_stale,
        &settings.task_watchdog.director_escalated,
    ]
}

fn validate_variants(
    singular: Option<&str>,
    candidates: Option<&[String]>,
    groups: Option<&[TuiCutexGroupedTemplates]>,
    rich: Option<&[TuiCutexRichSpan]>,
    has_selection: bool,
    placeholders: &[&str],
) -> Result<(), String> {
    if usize::from(singular.is_some())
        + usize::from(candidates.is_some())
        + usize::from(groups.is_some())
        + usize::from(rich.is_some())
        > 1
    {
        return Err(
            "rich_template, template, templates, and grouped_templates are mutually exclusive"
                .to_string(),
        );
    }
    if has_selection && candidates.is_none() && groups.is_none() {
        return Err("selection requires templates or grouped_templates".to_string());
    }
    if (candidates.is_some() || groups.is_some()) && !has_selection {
        return Err(
            "templates and grouped_templates require selection = \"stable_random\"".to_string(),
        );
    }
    validate_template(singular, placeholders)?;
    validate_rich_template(rich, placeholders)?;
    let mut aggregate_bytes = 0;
    if let Some(candidates) = candidates {
        if candidates.is_empty() || candidates.len() > MAX_TEMPLATE_CANDIDATES {
            return Err("templates must contain between 1 and 8 candidates".to_string());
        }
        for candidate in candidates {
            validate_template(Some(candidate), placeholders)?;
            aggregate_bytes += candidate.len();
        }
    }
    if let Some(groups) = groups {
        if groups.is_empty() || groups.len() > MAX_TEMPLATE_GROUPS {
            return Err("grouped_templates must contain between 1 and 8 groups".to_string());
        }
        let mut group_ids = std::collections::HashSet::new();
        for group in groups {
            if !valid_group_id(&group.group) || !group_ids.insert(group.group.as_str()) {
                return Err("group IDs must be unique 1-64 byte safe ASCII values".to_string());
            }
            if group.templates.is_empty() || group.templates.len() > MAX_TEMPLATE_CANDIDATES {
                return Err("each group must contain between 1 and 8 candidates".to_string());
            }
            for candidate in &group.templates {
                validate_template(Some(candidate), placeholders)?;
                aggregate_bytes += candidate.len();
            }
        }
    }
    if aggregate_bytes > MAX_TEMPLATE_AGGREGATE_BYTES {
        return Err("templates exceed 4096 aggregate bytes".to_string());
    }
    Ok(())
}

fn validate_rich_template(
    rich: Option<&[TuiCutexRichSpan]>,
    placeholders: &[&str],
) -> Result<(), String> {
    let Some(spans) = rich else {
        return Ok(());
    };
    if spans.is_empty() || spans.len() > MAX_RICH_SPANS {
        return Err("rich_template must contain between 1 and 16 spans".to_string());
    }
    let configured_chars = spans
        .iter()
        .map(|span| span.text.chars().count())
        .sum::<usize>();
    let line_count = 1 + spans
        .iter()
        .map(|span| {
            span.text
                .chars()
                .filter(|character| *character == '\n')
                .count()
        })
        .sum::<usize>();
    if configured_chars > MAX_RICH_CONFIGURED_CHARS || line_count > MAX_RICH_LINES {
        return Err(
            "rich_template exceeds 1024 configured characters or 8 rendered lines".to_string(),
        );
    }
    if spans.iter().all(|span| span.text.trim().is_empty()) {
        return Err("rich_template aggregate must contain non-whitespace text".to_string());
    }
    for span in spans {
        if span
            .text
            .chars()
            .any(|character| character.is_control() && character != '\n')
        {
            return Err("rich_template contains unsafe control characters".to_string());
        }
        validate_placeholders(&span.text, placeholders)?;
    }
    Ok(())
}

fn valid_group_id(group: &str) -> bool {
    !group.is_empty()
        && group.len() <= 64
        && group
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
}

fn validate_grouped_continuations(settings: &TuiCutexActivitySettings) -> Result<(), String> {
    for operation in operations(settings) {
        let initial_is_grouped = operation
            .prepared
            .as_ref()
            .and_then(|phase| phase.grouped_templates.as_ref())
            .is_some();
        if phases(operation)[1..]
            .iter()
            .flatten()
            .any(|phase| phase.grouped_templates.is_some())
            && !initial_is_grouped
        {
            return Err(
                "grouped Management continuation requires grouped prepared templates".to_string(),
            );
        }
    }
    let task = &settings.task_service_message;
    if task_service_presentations(settings)
        .into_iter()
        .skip(1)
        .any(|stage| stage.grouped_templates.is_some())
        && task.assignment.grouped_templates.is_none()
    {
        return Err(
            "grouped Task Service continuation requires grouped assignment templates".to_string(),
        );
    }
    if settings
        .task_watchdog
        .director_escalated
        .grouped_templates
        .is_some()
        && settings
            .task_watchdog
            .first_stale
            .grouped_templates
            .is_none()
    {
        return Err(
            "grouped Task watchdog continuation requires grouped first_stale templates".to_string(),
        );
    }
    Ok(())
}

fn validate_template(template: Option<&str>, placeholders: &[&str]) -> Result<(), String> {
    let Some(template) = template else {
        return Ok(());
    };
    if template.trim().is_empty()
        || template.chars().count() > MAX_TEMPLATE_CHARS
        || template.lines().count() > MAX_TEMPLATE_LINES
        || template
            .chars()
            .any(|character| character.is_control() && character != '\n')
    {
        return Err("template is empty, oversized, or contains unsafe controls".to_string());
    }
    validate_placeholders(template, placeholders)
}

fn validate_placeholders(template: &str, placeholders: &[&str]) -> Result<(), String> {
    let chars = template.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '{' if chars.get(index + 1) == Some(&'{') => index += 2,
            '}' if chars.get(index + 1) == Some(&'}') => index += 2,
            '{' => {
                let Some(relative_end) = chars[index + 1..].iter().position(|value| *value == '}')
                else {
                    return Err("template has an unclosed placeholder".to_string());
                };
                let end = index + 1 + relative_end;
                let key = chars[index + 1..end].iter().collect::<String>();
                if !placeholders.contains(&key.as_str()) {
                    return Err(format!("unknown template placeholder {{{key}}}"));
                }
                index = end + 1;
            }
            '}' => return Err("template has an unmatched closing brace".to_string()),
            _ => index += 1,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TuiCutexForegroundColor;
    use crate::types::TuiCutexGroupedTemplates;
    use crate::types::TuiCutexRichSpan;
    use crate::types::TuiCutexTemplateSelection;

    #[test]
    fn accepts_singular_and_stable_candidate_templates() {
        let mut settings = TuiCutexActivitySettings::default();
        settings.director_rotate.successor_ready = Some(TuiAgentManagementPhasePresentation {
            templates: Some(vec![
                "恭喜 {agent_name} 已经可以撑地了".to_string(),
                "{agent_name} 堂堂登场！".to_string(),
            ]),
            selection: Some(TuiCutexTemplateSelection::StableRandom),
            ..Default::default()
        });
        validate_cutex_tui_settings(&settings).expect("valid settings");
    }

    #[test]
    fn rejects_ambiguous_or_unsafe_candidates() {
        let mut settings = TuiCutexActivitySettings::default();
        settings.create.ready = Some(TuiAgentManagementPhasePresentation {
            template: Some("{agent_name}".to_string()),
            templates: Some(vec!["{unknown}".to_string()]),
            ..Default::default()
        });
        assert!(validate_cutex_tui_settings(&settings).is_err());
    }

    #[test]
    fn accepts_strict_named_groups_and_rejects_ambiguity_and_bad_ids() {
        let group = TuiCutexGroupedTemplates {
            group: "delivery.alpha:1".into(),
            templates: vec!["{agent_name} picked up".into()],
        };
        let mut settings = TuiCutexActivitySettings::default();
        settings.create.prepared = Some(TuiAgentManagementPhasePresentation {
            grouped_templates: Some(vec![group.clone()]),
            selection: Some(TuiCutexTemplateSelection::StableRandom),
            ..Default::default()
        });
        settings.create.ready = Some(TuiAgentManagementPhasePresentation {
            grouped_templates: Some(vec![group]),
            selection: Some(TuiCutexTemplateSelection::StableRandom),
            ..Default::default()
        });
        validate_cutex_tui_settings(&settings).expect("valid grouped settings");

        settings.create.ready.as_mut().unwrap().template = Some("ambiguous".into());
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings.create.ready.as_mut().unwrap().template = None;
        settings
            .create
            .ready
            .as_mut()
            .unwrap()
            .grouped_templates
            .as_mut()
            .unwrap()[0]
            .group = "bad group".into();
        assert!(validate_cutex_tui_settings(&settings).is_err());
        let invalid = TuiCutexGroupedTemplates {
            group: "duplicate".into(),
            templates: vec!["safe".into()],
        };
        settings.create.prepared.as_mut().unwrap().grouped_templates = Some(vec![invalid; 9]);
        assert!(validate_cutex_tui_settings(&settings).is_err());

        settings = TuiCutexActivitySettings::default();
        settings.task_service_message.progress.grouped_templates =
            Some(vec![TuiCutexGroupedTemplates {
                group: "alpha".into(),
                templates: vec!["{task_name} progressed".into()],
            }]);
        settings.task_service_message.progress.selection =
            Some(TuiCutexTemplateSelection::StableRandom);
        assert!(validate_cutex_tui_settings(&settings).is_err());
    }

    #[test]
    fn watchdog_templates_are_bounded_allowlisted_and_grouped_from_first_stage() {
        let mut settings = TuiCutexActivitySettings::default();
        settings.task_watchdog.first_stale.grouped_templates =
            Some(vec![TuiCutexGroupedTemplates {
                group: "alpha".into(),
                templates: vec!["{assignee} idle {idle} on {task}".into()],
            }]);
        settings.task_watchdog.first_stale.selection =
            Some(TuiCutexTemplateSelection::StableRandom);
        settings.task_watchdog.director_escalated.grouped_templates =
            Some(vec![TuiCutexGroupedTemplates {
                group: "alpha".into(),
                templates: vec!["{assignee_id} escalated {assignment}/{attempt}".into()],
            }]);
        settings.task_watchdog.director_escalated.selection =
            Some(TuiCutexTemplateSelection::StableRandom);
        validate_cutex_tui_settings(&settings).expect("safe watchdog config");

        settings.task_watchdog.first_stale.template = Some("{payload}".into());
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings.task_watchdog.first_stale.template = None;
        settings.task_watchdog.first_stale.grouped_templates = None;
        assert!(validate_cutex_tui_settings(&settings).is_err());

        settings.task_watchdog.director_escalated.grouped_templates = None;
        settings.task_watchdog.director_escalated.selection = None;
        settings.task_watchdog.first_stale.template = Some("x".repeat(MAX_TEMPLATE_CHARS + 1));
        assert!(validate_cutex_tui_settings(&settings).is_err());
    }

    fn rich(text: impl Into<String>) -> TuiCutexRichSpan {
        TuiCutexRichSpan {
            text: text.into(),
            foreground: Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98)),
            bold: Some(true),
            dim: None,
            italic: None,
        }
    }

    #[test]
    fn rich_templates_are_reused_bounded_and_mutually_exclusive() {
        let mut settings = TuiCutexActivitySettings::default();
        settings.create.ready = Some(TuiAgentManagementPhasePresentation {
            rich_template: Some(vec![rich("{agent_name} ready")]),
            ..Default::default()
        });
        settings.task_service_message.progress.rich_template =
            Some(vec![rich("{task_name}: {payload}")]);
        settings.task_watchdog.director_escalated.rich_template =
            Some(vec![rich("{assignee} {idle} {timestamp}")]);
        validate_cutex_tui_settings(&settings).expect("valid reusable rich templates");

        settings
            .create
            .ready
            .as_mut()
            .expect("ready phase")
            .template = Some("ambiguous".into());
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings
            .create
            .ready
            .as_mut()
            .expect("ready phase")
            .template = None;
        settings
            .create
            .ready
            .as_mut()
            .expect("ready phase")
            .selection = Some(TuiCutexTemplateSelection::StableRandom);
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings
            .create
            .ready
            .as_mut()
            .expect("ready phase")
            .selection = None;
        settings.task_service_message.progress.rich_template =
            Some((0..=MAX_RICH_SPANS).map(|_| rich("x")).collect());
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings.task_service_message.progress.rich_template = Some(vec![rich("bad\tcontrol")]);
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings.task_service_message.progress.rich_template =
            Some(vec![rich("{unknown_placeholder}")]);
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings.task_service_message.progress.rich_template = Some(vec![rich(" \n ")]);
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings.task_service_message.progress.rich_template =
            Some(vec![rich("x".repeat(MAX_RICH_CONFIGURED_CHARS + 1))]);
        assert!(validate_cutex_tui_settings(&settings).is_err());
        settings.task_service_message.progress.rich_template =
            Some(vec![rich("x\n".repeat(MAX_RICH_LINES))]);
        assert!(validate_cutex_tui_settings(&settings).is_err());
    }

    #[test]
    fn colors_accept_legacy_names_and_strict_truecolor_only() {
        for (source, expected) in [
            ("foreground = \"magenta\"", TuiCutexForegroundColor::Magenta),
            (
                "foreground = \"#98FF98\"",
                TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98),
            ),
            (
                "foreground = \"#d3d3d3\"",
                TuiCutexForegroundColor::Rgb(0xD3, 0xD3, 0xD3),
            ),
        ] {
            let style =
                toml::from_str::<crate::types::TuiCutexTextStyle>(source).expect("supported color");
            assert_eq!(style.foreground, Some(expected));
        }
        for source in [
            "foreground = \"#FFF\"",
            "foreground = \"#98FF98AA\"",
            "foreground = \" #98FF98\"",
            "foreground = \"#98FF9G\"",
            "foreground = \"mint\"",
        ] {
            assert!(toml::from_str::<crate::types::TuiCutexTextStyle>(source).is_err());
        }
    }
}
