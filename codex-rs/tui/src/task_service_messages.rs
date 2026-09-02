//! Dedicated live-only rendering for typed, authenticated Task Service system messages.

use crate::text_formatting::truncate_text;
use codex_app_server_protocol::TaskServiceMessageClass;
use codex_app_server_protocol::TaskServiceMessagePresentation;
#[cfg(test)]
use codex_config::types::TuiCutexForegroundColor;
use codex_config::types::TuiTaskServiceMessagePresentation as ClassSettings;
use codex_config::types::TuiTaskServiceMessageSettings;
use ratatui::style::Color;
#[cfg(test)]
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize as _;
use ratatui::text::Line;
use ratatui::text::Span;
use sha2::Digest;
use sha2::Sha256;

use crate::cutex_rich_text;
use crate::cutex_template_groups::TemplateDecision;

const MAX_TEMPLATE_CHARS: usize = 320;
const MAX_PAYLOAD_CHARS: usize = 2_000;
const MAX_PAYLOAD_LINES: usize = 8;

pub(crate) fn history_lines(
    message_id: &str,
    presentation: &TaskServiceMessagePresentation,
    settings: Option<&TuiTaskServiceMessageSettings>,
    decision: Option<&TemplateDecision>,
) -> Vec<Line<'static>> {
    let defaults = TuiTaskServiceMessageSettings::default();
    let settings = settings.unwrap_or(&defaults);
    if !settings.visible {
        return Vec::new();
    }
    let class_settings = class_settings(presentation.class, settings);
    if class_settings.show == Some(false) {
        return Vec::new();
    }
    let fallback = default_label(presentation);
    let template = match decision {
        Some(TemplateDecision::Selected(template)) => Some(template.as_str()),
        Some(TemplateDecision::SafeDefault) => None,
        None => select_template(message_id, presentation, class_settings),
    };
    let style = cutex_rich_text::apply_text_style(
        Style::default().fg(default_color(presentation.class)),
        &class_settings.style,
    );
    let mut variables = vec![
        ("task_name", presentation.task_name.as_str()),
        ("assignment_id", presentation.assignment_id.as_str()),
        ("payload", presentation.semantic_payload.as_str()),
    ];
    if let Some(transition) = presentation.transition.as_deref() {
        variables.push(("transition", transition));
    }
    let mut lines =
        cutex_rich_text::render(class_settings.rich_template.as_deref(), &variables, style)
            .unwrap_or_else(|| {
                render_template(template, &fallback, presentation)
                    .lines()
                    .map(|line| Line::from(Span::styled(line.to_string(), style)))
                    .collect()
            });

    if class_settings.show_payload.unwrap_or(true) {
        let indent = " ".repeat(usize::from(
            class_settings.content_indent.unwrap_or(2).min(8),
        ));
        let payload = truncate_text(&presentation.semantic_payload, MAX_PAYLOAD_CHARS);
        lines.extend(
            payload
                .lines()
                .take(MAX_PAYLOAD_LINES)
                .map(|line| vec![indent.clone().into(), line.to_string().dim()].into()),
        );
    }
    lines
}

pub(crate) fn class_settings(
    class: TaskServiceMessageClass,
    settings: &TuiTaskServiceMessageSettings,
) -> &ClassSettings {
    match class {
        TaskServiceMessageClass::Assignment => &settings.assignment,
        TaskServiceMessageClass::Progress => &settings.progress,
        TaskServiceMessageClass::Blocked => &settings.blocked,
        TaskServiceMessageClass::Resumed => &settings.resumed,
        TaskServiceMessageClass::ReviewReady => &settings.review_ready,
        TaskServiceMessageClass::Retry => &settings.retry,
        TaskServiceMessageClass::TerminalClosure => &settings.terminal_closure,
    }
}

fn default_label(presentation: &TaskServiceMessagePresentation) -> String {
    let state = match presentation.class {
        TaskServiceMessageClass::Assignment => "assigned",
        TaskServiceMessageClass::Progress => "progress",
        TaskServiceMessageClass::Blocked => "blocked",
        TaskServiceMessageClass::Resumed => "resumed",
        TaskServiceMessageClass::ReviewReady => "review ready",
        TaskServiceMessageClass::Retry => "retry",
        TaskServiceMessageClass::TerminalClosure => "closed",
    };
    format!(
        "Task Service · {} · {} ({})",
        state, presentation.task_name, presentation.assignment_id
    )
}

fn select_template<'a>(
    message_id: &str,
    presentation: &TaskServiceMessagePresentation,
    settings: &'a ClassSettings,
) -> Option<&'a str> {
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
    for component in [
        message_id,
        presentation.assignment_id.as_str(),
        presentation.transition.as_deref().unwrap_or_default(),
        class_name(presentation.class),
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

fn render_template(
    template: Option<&str>,
    fallback: &str,
    presentation: &TaskServiceMessagePresentation,
) -> String {
    let Some(template) = template else {
        return fallback.to_string();
    };
    if template.trim().is_empty()
        || template.chars().count() > MAX_TEMPLATE_CHARS
        || template.lines().count() > 4
        || template
            .chars()
            .any(|value| value.is_control() && value != '\n')
    {
        return fallback.to_string();
    }
    let transition = presentation.transition.as_deref().unwrap_or_default();
    let variables = [
        ("task_name", presentation.task_name.as_str()),
        ("assignment_id", presentation.assignment_id.as_str()),
        ("transition", transition),
        ("payload", presentation.semantic_payload.as_str()),
    ];
    let mut rendered = String::new();
    let chars = template.chars().collect::<Vec<_>>();
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
            value => {
                rendered.push(value);
                index += 1;
            }
        }
    }
    if rendered.lines().count() > 4 {
        fallback.to_string()
    } else {
        truncate_text(&rendered, MAX_TEMPLATE_CHARS)
    }
}

pub(crate) fn class_name(class: TaskServiceMessageClass) -> &'static str {
    match class {
        TaskServiceMessageClass::Assignment => "assignment",
        TaskServiceMessageClass::Progress => "progress",
        TaskServiceMessageClass::Blocked => "blocked",
        TaskServiceMessageClass::Resumed => "resumed",
        TaskServiceMessageClass::ReviewReady => "review_ready",
        TaskServiceMessageClass::Retry => "retry",
        TaskServiceMessageClass::TerminalClosure => "terminal_closure",
    }
}

fn default_color(class: TaskServiceMessageClass) -> Color {
    match class {
        TaskServiceMessageClass::Blocked | TaskServiceMessageClass::Retry => Color::Yellow,
        TaskServiceMessageClass::ReviewReady | TaskServiceMessageClass::TerminalClosure => {
            Color::Green
        }
        _ => Color::Cyan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_config::types::TuiCutexRichSpan;
    use codex_config::types::TuiCutexTemplateSelection;

    fn assignment() -> TaskServiceMessagePresentation {
        TaskServiceMessagePresentation {
            class: TaskServiceMessageClass::Assignment,
            project_id: None,
            task_name: "presentation-r11".to_string(),
            assignment_id: "assignment-01".to_string(),
            transition: None,
            semantic_payload: "Implement the exact opaque contract.".to_string(),
        }
    }

    fn terminal_closure() -> TaskServiceMessagePresentation {
        TaskServiceMessagePresentation {
            class: TaskServiceMessageClass::TerminalClosure,
            project_id: None,
            task_name: "presentation-r11".to_string(),
            assignment_id: "assignment-01".to_string(),
            transition: Some("TerminalClosure".to_string()),
            semantic_payload: "Task closed successfully.".to_string(),
        }
    }

    #[test]
    fn dedicated_renderer_uses_semantics_and_omits_provider_mechanics() {
        let rendered = history_lines("external-message-ignored", &assignment(), None, None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Task Service · assigned · presentation-r11 (assignment-01)"));
        assert!(rendered.contains("Implement the exact opaque contract."));
        assert!(!rendered.contains("external-message-ignored"));
    }

    #[test]
    fn terminal_closure_is_hidden_by_default_but_can_be_shown_and_styled() {
        assert!(history_lines("closure", &terminal_closure(), None, None).is_empty());

        let mut settings = TuiTaskServiceMessageSettings::default();
        settings.terminal_closure.show = Some(true);
        settings.terminal_closure.show_payload = Some(false);
        settings.terminal_closure.rich_template = Some(vec![TuiCutexRichSpan {
            text: "Task Service · {transition} · {assignment_id}".into(),
            foreground: Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98)),
            bold: Some(true),
            dim: None,
            italic: None,
        }]);
        let lines = history_lines("closure", &terminal_closure(), Some(&settings), None);
        assert_eq!(
            lines[0].to_string(),
            "Task Service · TerminalClosure · assignment-01"
        );
        assert_eq!(
            lines[0].spans[0].style.fg,
            Some(crate::terminal_palette::rgb_color((0x98, 0xFF, 0x98)))
        );
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn configured_variants_are_repeatable_and_distinct_ids_distribute() {
        let mut settings = TuiTaskServiceMessageSettings::default();
        settings.assignment.templates = Some(vec!["A {task_name}".into(), "B {task_name}".into()]);
        settings.assignment.selection = Some(TuiCutexTemplateSelection::StableRandom);
        settings.assignment.show_payload = Some(false);
        let first = history_lines("event-1", &assignment(), Some(&settings), None);
        assert_eq!(
            first,
            history_lines("event-1", &assignment(), Some(&settings), None)
        );
        let choices = (0..64)
            .map(|index| {
                history_lines(
                    &format!("event-{index}"),
                    &assignment(),
                    Some(&settings),
                    None,
                )[0]
                .to_string()
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(choices.len(), 2);
    }

    #[test]
    fn grouped_live_decision_uses_dedicated_task_renderer_and_style() {
        let mut settings = TuiTaskServiceMessageSettings::default();
        settings.assignment.style.foreground = Some(TuiCutexForegroundColor::Magenta);
        settings.assignment.style.bold = Some(true);
        settings.assignment.show_payload = Some(true);
        let decision = TemplateDecision::Selected("alpha · {task_name} accepted".into());
        let rendered = history_lines(
            "transport-id-is-not-correlation",
            &assignment(),
            Some(&settings),
            Some(&decision),
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        insta::assert_snapshot!(rendered, @r###"
        alpha · presentation-r11 accepted
          Implement the exact opaque contract.
        "###);
    }

    #[test]
    fn rich_template_reuses_task_class_semantics_and_inherits_base_style() {
        let mut settings = TuiTaskServiceMessageSettings::default();
        settings.assignment.style.bold = Some(true);
        settings.assignment.show_payload = Some(false);
        settings.assignment.rich_template = Some(vec![
            TuiCutexRichSpan {
                text: "task_service: ".into(),
                foreground: None,
                bold: None,
                dim: None,
                italic: None,
            },
            TuiCutexRichSpan {
                text: "{task_name}\n{assignment_id}".into(),
                foreground: Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98)),
                bold: Some(false),
                dim: Some(true),
                italic: None,
            },
        ]);
        let lines = history_lines("event-rich", &assignment(), Some(&settings), None);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].to_string(), "task_service: presentation-r11");
        assert_eq!(lines[1].to_string(), "assignment-01");
        assert_eq!(lines[0].spans[0].style.fg, Some(Color::Cyan));
        assert!(
            lines[0].spans[0]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert_eq!(
            lines[0].spans[1].style.fg,
            Some(crate::terminal_palette::rgb_color((0x98, 0xFF, 0x98)))
        );
        assert!(
            !lines[0].spans[1]
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
        assert!(lines[0].spans[1].style.add_modifier.contains(Modifier::DIM));

        settings.assignment.rich_template = Some(vec![TuiCutexRichSpan {
            text: "{transition}".into(),
            foreground: None,
            bold: None,
            dim: None,
            italic: None,
        }]);
        assert_eq!(
            history_lines("event-rich", &assignment(), Some(&settings), None)[0].to_string(),
            "Task Service · assigned · presentation-r11 (assignment-01)"
        );
    }
}
