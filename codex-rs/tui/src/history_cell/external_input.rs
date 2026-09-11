//! One view for the server's validated canonical external input projection.
use super::*;
use codex_protocol::external_input::Source;
use codex_protocol::external_input::SourceKind;
use codex_protocol::models::FunctionCallOutputBody;
use serde::Deserialize;
#[path = "external_job_view.rs"]
mod job_view;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Body {
    source: Source,
    #[serde(rename = "type")]
    event_type: String,
    text: String,
}
#[derive(Debug)]
pub(crate) struct ExternalInputHistoryCell {
    pub(super) id: String,
    header: String,
    body: String,
    view: Option<codex_protocol::external_input_view::View>,
    id_label: Option<String>,
}
impl ExternalInputHistoryCell {
    pub(crate) fn observe_display_id(&mut self, labels: &mut super::JobLabels) {
        if let Some(view) = &self.view
            && job_view::render(view, None).is_some()
            && let Some(id) = view.data["jobId"].as_str()
        {
            self.id_label = Some(labels.observe_display_id(id));
        }
    }
    pub(crate) fn parse(
        id: &str,
        name: &str,
        namespace: Option<&str>,
        output: &FunctionCallOutputBody,
        view: Option<&codex_protocol::external_input_view::View>,
    ) -> Option<Self> {
        if name != "external_event"
            || namespace != Some("external")
            || id.is_empty()
            || id.len() > 256
        {
            return None;
        }
        let FunctionCallOutputBody::Text(text) = output else {
            return None;
        };
        // Maximum UTF-8 body after JSON escaping, plus bounded source/type wrapper.
        if text.len() > 6 * 65536 + 4096 {
            return None;
        }
        let body: Body = serde_json::from_str(text).ok()?;
        if [&body.source.id, &body.event_type]
            .iter()
            .any(|s| s.is_empty() || s.len() > 256)
            || body.text.is_empty()
            || body.text.len() > 65536
        {
            return None;
        }
        let kind = match body.source.kind {
            SourceKind::Agent => "agent",
            SourceKind::Service => "service",
        };
        Some(Self {
            id: id.into(),
            header: format!(
                "External input · {kind}/{} · {}",
                body.source.id, body.event_type
            ),
            body: body.text,
            view: view.cloned(),
            id_label: None,
        })
    }
}
impl HistoryCell for ExternalInputHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        if let Some((header, body)) = self
            .view
            .as_ref()
            .and_then(|view| job_view::render(view, self.id_label.as_deref()))
        {
            let mut lines =
                super::event_presentation::render_event(Some(&header), &[], Some("•".dim()), width);
            lines.extend(
                super::event_presentation::render_event(None, &body, Some("•".dim()), width)
                    .into_iter()
                    .map(Stylize::dim),
            );
            return lines;
        }
        super::event_presentation::render_event(
            Some(&self.header),
            std::slice::from_ref(&self.body),
            Some("•".dim()),
            width,
        )
    }
    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = super::event_presentation::render_event(
            Some(&self.header),
            std::slice::from_ref(&self.body),
            None,
            width,
        );
        if let Some(view) = &self.view {
            lines.extend(super::event_presentation::render_event(
                Some("Display facts (not model input)"),
                &[view.canonical_json()],
                None,
                width,
            ));
        }
        lines
    }
    fn raw_lines(&self) -> Vec<Line<'static>> {
        let mut lines = super::event_presentation::render_event(
            Some(&self.header),
            std::slice::from_ref(&self.body),
            None,
            u16::MAX,
        );
        if let Some(view) = &self.view {
            lines.extend(super::event_presentation::render_event(
                Some("Display facts (not model input)"),
                &[view.canonical_json()],
                None,
                u16::MAX,
            ));
        }
        lines
    }
}

#[cfg(test)]
#[path = "external_input_view_tests.rs"]
mod view_tests;
