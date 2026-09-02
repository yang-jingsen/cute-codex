//! Shared presentation for canonical inbound inter-agent message items.

use crate::history_cell::PlainHistoryCell;
use crate::text_formatting::truncate_text;
use codex_app_server_protocol::CutexParticipantPresentation;
use codex_app_server_protocol::InterAgentDeliveryMode;
use codex_app_server_protocol::TaskServiceMessagePresentation;
#[cfg(test)]
use codex_config::types::TuiCutexForegroundColor;
use codex_config::types::TuiCutexInboundMessageSettings;
use codex_config::types::TuiCutexTextStyle;
use codex_config::types::TuiTaskServiceMessageSettings;
#[cfg(test)]
use ratatui::style::Color;
#[cfg(test)]
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::style::Stylize as _;
use ratatui::text::Line;
use ratatui::text::Span;

use crate::cutex_rich_text;
use crate::cutex_template_groups::TemplateDecision;

const AUTHOR_PREVIEW_GRAPHEMES: usize = 240;
const MESSAGE_METADATA_GRAPHEMES: usize = 1_000;
const MESSAGE_CONTENT_GRAPHEMES: usize = 2_000;
const COMPACT_SUMMARY_GRAPHEMES: usize = 240;
const CUTEX_TEMPLATE_CHARS: usize = 512;
const CUTEX_TEMPLATE_LINES: usize = 4;
const CUTEX_CONTENT_INDENT: usize = 8;

#[derive(Clone, Copy)]
pub(crate) struct InterAgentMessagePresentation<'a> {
    id: &'a str,
    author: &'a str,
    recipient: &'a str,
    other_recipients: &'a [String],
    content: &'a str,
    delivery_mode: InterAgentDeliveryMode,
    author_metadata: Option<&'a CutexParticipantPresentation>,
    recipient_metadata: Option<&'a CutexParticipantPresentation>,
    cutex_settings: Option<&'a TuiCutexInboundMessageSettings>,
    task_service_presentation: Option<&'a TaskServiceMessagePresentation>,
    task_service_settings: Option<&'a TuiTaskServiceMessageSettings>,
    task_service_template: Option<&'a TemplateDecision>,
}

impl<'a> InterAgentMessagePresentation<'a> {
    pub(crate) fn new(
        id: &'a str,
        author: &'a str,
        recipient: &'a str,
        other_recipients: &'a [String],
        content: &'a str,
        delivery_mode: InterAgentDeliveryMode,
    ) -> Self {
        Self {
            id,
            author,
            recipient,
            other_recipients,
            content,
            delivery_mode,
            author_metadata: None,
            recipient_metadata: None,
            cutex_settings: None,
            task_service_presentation: None,
            task_service_settings: None,
            task_service_template: None,
        }
    }

    pub(crate) fn with_task_service_template(
        mut self,
        decision: Option<&'a TemplateDecision>,
    ) -> Self {
        self.task_service_template = decision;
        self
    }

    pub(crate) fn with_task_service_presentation(
        mut self,
        presentation: Option<&'a TaskServiceMessagePresentation>,
        settings: Option<&'a TuiTaskServiceMessageSettings>,
    ) -> Self {
        self.task_service_presentation = presentation;
        self.task_service_settings = settings;
        self
    }

    pub(crate) fn with_cutex_metadata(
        mut self,
        author_metadata: Option<&'a CutexParticipantPresentation>,
        recipient_metadata: Option<&'a CutexParticipantPresentation>,
        settings: Option<&'a TuiCutexInboundMessageSettings>,
    ) -> Self {
        self.author_metadata = author_metadata;
        self.recipient_metadata = recipient_metadata;
        self.cutex_settings = settings;
        self
    }

    pub(crate) fn history_cell(self) -> PlainHistoryCell {
        PlainHistoryCell::new(self.history_lines())
    }

    pub(crate) fn compact_summary(self) -> String {
        let content = self
            .content
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let author = self.author_display();
        let prefix = if self.is_tagged_cutex() {
            "Cutex message"
        } else {
            "Agent message"
        };
        let summary = if content.is_empty() {
            format!("{prefix} [{}] from {author}", self.delivery_mode_label(),)
        } else {
            format!(
                "{prefix} [{}] from {author}: {content}",
                self.delivery_mode_label(),
            )
        };
        truncate_text(&summary, COMPACT_SUMMARY_GRAPHEMES)
    }

    fn history_lines(self) -> Vec<Line<'static>> {
        if let Some(presentation) = self.task_service_presentation {
            return crate::task_service_messages::history_lines(
                self.id,
                presentation,
                self.task_service_settings,
                self.task_service_template,
            );
        }
        if self.is_tagged_cutex() {
            return self.cutex_history_lines();
        }
        self.native_history_lines()
    }

    fn native_history_lines(self) -> Vec<Line<'static>> {
        let author = truncate_text(self.author, AUTHOR_PREVIEW_GRAPHEMES);
        let mode = self.delivery_mode_label();
        let mut lines = vec![
            vec![
                "Agent message".bold(),
                " [".dim(),
                mode.cyan().bold(),
                "]".dim(),
                " from ".dim(),
                author.cyan().bold(),
            ]
            .into(),
        ];

        let mut metadata = vec![
            format!("id={}", self.id),
            format!("to={}", self.recipient),
            format!("mode={mode}"),
        ];
        if !self.other_recipients.is_empty() {
            metadata.push(format!(
                "other_recipients={}",
                self.other_recipients.join(",")
            ));
        }
        lines.push(
            truncate_text(&metadata.join(" | "), MESSAGE_METADATA_GRAPHEMES)
                .dim()
                .into(),
        );

        let content = truncate_text(self.content, MESSAGE_CONTENT_GRAPHEMES);
        lines.extend(
            content
                .lines()
                .map(|line| vec!["  ".into(), line.to_string().into()].into()),
        );
        lines
    }

    fn cutex_history_lines(self) -> Vec<Line<'static>> {
        let defaults = TuiCutexInboundMessageSettings::default();
        let settings = self.cutex_settings.unwrap_or(&defaults);
        let author = self.author_display();
        let recipient = self.recipient_display();
        let mode = self.delivery_mode_label();
        let variables = [
            ("author", author.as_str()),
            ("recipient", recipient.as_str()),
            ("mode", mode),
            (
                "profile",
                author_value(self.author_metadata, |value| &value.profile),
            ),
            (
                "model",
                author_value(self.author_metadata, |value| &value.model),
            ),
            (
                "reasoning",
                author_value(self.author_metadata, |value| &value.reasoning),
            ),
            (
                "role",
                author_value(self.author_metadata, |value| &value.role),
            ),
            (
                "runtime_backend",
                author_value(self.author_metadata, |value| &value.runtime_backend),
            ),
        ];
        let default_label = format!("Cutex message [{mode}] from {author}");
        let label = render_cutex_template(settings.label.as_deref(), &default_label, &variables);
        let style = text_style(&settings.style);
        let mut lines = label
            .lines()
            .map(|line| Line::from(Span::styled(line.to_string(), style)))
            .collect::<Vec<_>>();

        if settings.show_metadata {
            let mut metadata = Vec::new();
            if settings.show_ids {
                metadata.push(format!("id={}", self.id));
                metadata.push(format!("from={}", self.author));
                metadata.push(format!("to={}", self.recipient));
                if let Some(session) = self
                    .author_metadata
                    .and_then(|metadata| metadata.cutex_session_id.as_deref())
                {
                    metadata.push(format!("session={session}"));
                }
            }
            for (label, value) in [
                (
                    "profile",
                    author_value(self.author_metadata, |value| &value.profile),
                ),
                (
                    "model",
                    author_value(self.author_metadata, |value| &value.model),
                ),
                (
                    "reasoning",
                    author_value(self.author_metadata, |value| &value.reasoning),
                ),
                (
                    "role",
                    author_value(self.author_metadata, |value| &value.role),
                ),
                (
                    "runtime",
                    author_value(self.author_metadata, |value| &value.runtime_backend),
                ),
            ] {
                if !value.is_empty() {
                    metadata.push(format!("{label}={value}"));
                }
            }
            if !metadata.is_empty() {
                lines.push(
                    truncate_text(&metadata.join(" | "), MESSAGE_METADATA_GRAPHEMES)
                        .dim()
                        .into(),
                );
            }
        }

        let indent = " ".repeat(usize::from(settings.content_indent).min(CUTEX_CONTENT_INDENT));
        let content = truncate_text(self.content, MESSAGE_CONTENT_GRAPHEMES);
        lines.extend(
            content
                .lines()
                .map(|line| vec![indent.clone().into(), line.to_string().into()].into()),
        );
        lines
    }

    fn is_tagged_cutex(self) -> bool {
        self.author_metadata.is_some() || self.recipient_metadata.is_some()
    }

    fn author_display(self) -> String {
        self.author_metadata
            .and_then(|metadata| metadata.display_name.clone())
            .unwrap_or_else(|| self.author.to_string())
    }

    fn recipient_display(self) -> String {
        self.recipient_metadata
            .and_then(|metadata| metadata.display_name.clone())
            .unwrap_or_else(|| self.recipient.to_string())
    }

    fn delivery_mode_label(self) -> &'static str {
        match self.delivery_mode {
            InterAgentDeliveryMode::AfterTurn => "after-turn",
            InterAgentDeliveryMode::Soon => "soon",
            InterAgentDeliveryMode::Passive => "passive",
            InterAgentDeliveryMode::Interrupt => "interrupt",
        }
    }
}

fn author_value<'a>(
    metadata: Option<&'a CutexParticipantPresentation>,
    field: impl FnOnce(&'a CutexParticipantPresentation) -> &'a Option<String>,
) -> &'a str {
    metadata
        .and_then(|metadata| field(metadata).as_deref())
        .unwrap_or("")
}

fn render_cutex_template(
    template: Option<&str>,
    fallback: &str,
    variables: &[(&str, &str)],
) -> String {
    let Some(template) = template else {
        return fallback.to_string();
    };
    if template.is_empty()
        || template.chars().count() > CUTEX_TEMPLATE_CHARS
        || template.split('\n').count() > CUTEX_TEMPLATE_LINES
        || template.chars().any(|ch| ch.is_control() && ch != '\n')
    {
        return fallback.to_string();
    }
    let mut rendered = String::new();
    let mut remaining = template;
    while let Some(open) = remaining.find('{') {
        if remaining[..open].contains('}') {
            return fallback.to_string();
        }
        rendered.push_str(&remaining[..open]);
        let after_open = &remaining[open + 1..];
        let Some(close) = after_open.find('}') else {
            return fallback.to_string();
        };
        let key = &after_open[..close];
        let Some((_, value)) = variables.iter().find(|(name, _)| *name == key) else {
            return fallback.to_string();
        };
        rendered.push_str(value);
        remaining = &after_open[close + 1..];
    }
    if remaining.contains('}') {
        return fallback.to_string();
    }
    rendered.push_str(remaining);
    if rendered.split('\n').count() > CUTEX_TEMPLATE_LINES {
        fallback.to_string()
    } else {
        truncate_text(&rendered, CUTEX_TEMPLATE_CHARS)
    }
}

fn text_style(settings: &TuiCutexTextStyle) -> Style {
    cutex_rich_text::apply_text_style(Style::default(), settings)
}

#[cfg(test)]
#[path = "inter_agent_messages_tests.rs"]
mod tests;
