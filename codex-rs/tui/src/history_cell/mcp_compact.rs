//! Display-only Cutex hook. Detailed/raw rendering never enters this module.
use super::*;

pub(super) fn render(cell: &McpToolCallCell, width: u16) -> Option<Vec<Line<'static>>> {
    if let Some(title) = super::super::cutex_mcp_display::title(&cell.invocation)
        && cell
            .result
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .is_none_or(|r| {
                r.content
                    .iter()
                    .filter_map(|block| block.text())
                    .all(super::super::cutex_mcp_display::known_result_version)
            })
    {
        let uncertain = cell
            .result
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .is_some_and(|r| {
                r.content
                    .iter()
                    .filter_map(|block| block.text())
                    .any(super::super::cutex_mcp_display::outcome_uncertain)
            });
        let (label, bullet) = match cell.success() {
            _ if uncertain => ("Outcome uncertain", "•".dim()),
            Some(true) => ("Call completed", "•".green().bold()),
            Some(false) => ("Call failed", "•".red().bold()),
            None => ("Calling", "•".dim()),
        };
        let body = match &cell.result {
            Some(Ok(result)) => result
                .content
                .iter()
                .map(|block| block.render(usize::from(width).saturating_sub(2).max(1)))
                .collect(),
            Some(Err(error)) => vec![format_and_truncate_tool_result(
                &format!("Error: {error}"),
                TOOL_CALL_MAX_LINES,
                usize::from(width).saturating_sub(2).max(1),
            )],
            None => Vec::new(),
        };
        return Some(super::super::event_presentation::render_event(
            Some(&format!("{label} · {title}")),
            &body,
            Some(bullet),
            width,
        ));
    }
    None
}
