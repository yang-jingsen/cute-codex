//! One view for the server's validated canonical external input projection.
use super::*;
use codex_protocol::external_input::Source;
use codex_protocol::external_input::SourceKind;
use codex_protocol::models::FunctionCallOutputBody;
use serde::Deserialize;

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
}
impl ExternalInputHistoryCell {
    pub(crate) fn parse(
        id: &str,
        name: &str,
        namespace: Option<&str>,
        output: &FunctionCallOutputBody,
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
        })
    }
}
impl HistoryCell for ExternalInputHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        super::event_presentation::render_event(
            Some(&self.header),
            std::slice::from_ref(&self.body),
            Some("•".dim()),
            width,
        )
    }
    fn raw_lines(&self) -> Vec<Line<'static>> {
        super::event_presentation::render_event(
            Some(&self.header),
            std::slice::from_ref(&self.body),
            None,
            u16::MAX,
        )
    }
}
