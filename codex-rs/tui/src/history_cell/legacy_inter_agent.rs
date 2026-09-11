//! Historical labels are data, not an authenticated current sender or delivery action.
use super::*;
use codex_protocol::items::LegacyInterAgentMessageItem;

#[derive(Debug)]
pub(crate) struct LegacyInterAgentHistoryCell(pub LegacyInterAgentMessageItem);

impl HistoryCell for LegacyInterAgentHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let header = format!(
            "Historical message · {} → {}",
            self.0.author, self.0.recipient
        );
        super::event_presentation::render_event(
            Some(&header),
            std::slice::from_ref(&self.0.content),
            Some("•".dim()),
            width,
        )
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines = self.display_lines(width);
        lines.extend(super::event_presentation::render_event(
            Some("Historical record (not delivery authority)"),
            &[serde_json::to_string(&self.0).expect("historical display fields serialize")],
            None,
            width,
        ));
        lines
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        self.transcript_lines(u16::MAX)
    }
}

#[cfg(test)]
#[path = "legacy_inter_agent_tests.rs"]
mod tests;
