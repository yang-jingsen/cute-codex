//! Display-only hook; unknown returned shapes preserve upstream presentation.
use super::super::cutex_mcp_display;
use super::*;

pub(super) fn render(cell: &McpToolCallCell, width: u16) -> Option<Vec<Line<'static>>> {
    let running = cutex_mcp_display::title(&cell.invocation)?;
    let (header, bullet) = match &cell.result {
        None => (running, "•".dim()),
        Some(Err(_)) => (format!("Failed · {running}"), "•".red().bold()),
        Some(Ok(result)) => {
            let outcome = if result.content.len() == 1 {
                result.content[0]
                    .text()
                    .and_then(|text| cutex_mcp_display::outcome(&cell.invocation, text))
            } else {
                None
            };
            match outcome {
                Some(outcome) if outcome.uncertain => (outcome.text, "•".dim()),
                Some(outcome) if outcome.failed => (outcome.text, "•".red().bold()),
                Some(_) if result.is_error => return None,
                // Exact configured Bon voyage ! foreground, adapted to terminal capability.
                Some(outcome) => (
                    outcome.text,
                    "•"
                        .fg(crate::terminal_palette::rgb_color((0xF6, 0xA3, 0xC8)))
                        .bold(),
                ),
                None => return None,
            }
        }
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
    Some(super::super::event_presentation::render_event(
        Some(&header),
        &body,
        Some(bullet),
        width,
    ))
}
