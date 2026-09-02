//! Shared bounded rendering for mixed-style Cutex presentation templates.

use codex_config::types::TuiCutexForegroundColor;
use codex_config::types::TuiCutexRichSpan;
use codex_config::types::TuiCutexTextStyle;
use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use unicode_width::UnicodeWidthChar;

const MAX_RICH_SPANS: usize = 16;
const MAX_CONFIGURED_CHARS: usize = 1_024;
const MAX_RENDERED_CHARS: usize = 2_048;
const MAX_RENDERED_LINES: usize = 8;

pub(crate) fn color(value: TuiCutexForegroundColor) -> Color {
    match value {
        TuiCutexForegroundColor::Black => Color::Black,
        TuiCutexForegroundColor::Red => Color::Red,
        TuiCutexForegroundColor::Green => Color::Green,
        TuiCutexForegroundColor::Yellow => Color::Yellow,
        TuiCutexForegroundColor::Blue => Color::Blue,
        TuiCutexForegroundColor::Magenta => Color::Magenta,
        TuiCutexForegroundColor::Cyan => Color::Cyan,
        TuiCutexForegroundColor::Gray => Color::Gray,
        TuiCutexForegroundColor::White => Color::White,
        TuiCutexForegroundColor::Rgb(red, green, blue) => {
            crate::terminal_palette::rgb_color((red, green, blue))
        }
    }
}

pub(crate) fn apply_text_style(mut style: Style, settings: &TuiCutexTextStyle) -> Style {
    apply_overrides(
        &mut style,
        settings.foreground,
        settings.bold,
        settings.dim,
        settings.italic,
    );
    style
}

fn span_style(mut base: Style, span: &TuiCutexRichSpan) -> Style {
    apply_overrides(&mut base, span.foreground, span.bold, span.dim, span.italic);
    base
}

fn apply_overrides(
    style: &mut Style,
    foreground: Option<TuiCutexForegroundColor>,
    bold: Option<bool>,
    dim: Option<bool>,
    italic: Option<bool>,
) {
    if let Some(foreground) = foreground {
        *style = style.fg(color(foreground));
    }
    for (enabled, modifier) in [
        (bold, Modifier::BOLD),
        (dim, Modifier::DIM),
        (italic, Modifier::ITALIC),
    ] {
        if let Some(enabled) = enabled {
            *style = if enabled {
                style.add_modifier(modifier)
            } else {
                style.remove_modifier(modifier)
            };
        }
    }
}

/// Render a validated rich template. Any runtime mismatch returns None so the caller can use
/// its existing safe default presentation.
pub(crate) fn render(
    configured: Option<&[TuiCutexRichSpan]>,
    variables: &[(&str, &str)],
    base_style: Style,
) -> Option<Vec<Line<'static>>> {
    let spans = configured?;
    if spans.is_empty()
        || spans.len() > MAX_RICH_SPANS
        || spans
            .iter()
            .map(|span| span.text.chars().count())
            .sum::<usize>()
            > MAX_CONFIGURED_CHARS
        || spans.iter().all(|span| span.text.trim().is_empty())
        || spans.iter().any(|span| {
            span.text
                .chars()
                .any(|character| character.is_control() && character != '\n')
        })
    {
        return None;
    }

    let mut lines = vec![Vec::<Span<'static>>::new()];
    let mut rendered_chars = 0;
    for configured_span in spans {
        let text = substitute(&configured_span.text, variables)?;
        rendered_chars += text.chars().count();
        if rendered_chars > MAX_RENDERED_CHARS {
            return None;
        }
        let style = span_style(base_style, configured_span);
        for (index, fragment) in text.split('\n').enumerate() {
            if index > 0 {
                if lines.len() == MAX_RENDERED_LINES {
                    return None;
                }
                lines.push(Vec::new());
            }
            if !fragment.is_empty()
                && let Some(line) = lines.last_mut()
            {
                line.push(Span::styled(fragment.to_string(), style));
            }
        }
    }
    Some(lines.into_iter().map(Line::from).collect())
}

fn substitute(template: &str, variables: &[(&str, &str)]) -> Option<String> {
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
                let relative_end = chars[index + 1..].iter().position(|value| *value == '}')?;
                let end = index + 1 + relative_end;
                let key = chars[index + 1..end].iter().collect::<String>();
                let (_, value) = variables.iter().find(|(name, _)| *name == key)?;
                rendered.push_str(value);
                index = end + 1;
            }
            '}' => return None,
            value => {
                rendered.push(value);
                index += 1;
            }
        }
    }
    Some(rendered)
}

pub(crate) fn wrap(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut wrapped = Vec::<Line<'static>>::new();
    for line in lines {
        let mut output = Vec::<Span<'static>>::new();
        let mut used = 0;
        for span in line.spans {
            let mut chunk = String::new();
            for character in span.content.chars() {
                let character_width = character.width().unwrap_or(0);
                if used > 0 && used + character_width > width {
                    if !chunk.is_empty() {
                        output.push(Span::styled(std::mem::take(&mut chunk), span.style));
                    }
                    wrapped.push(Line::from(std::mem::take(&mut output)));
                    if wrapped.len() == MAX_RENDERED_LINES {
                        append_ellipsis(&mut wrapped);
                        return wrapped;
                    }
                    used = 0;
                }
                chunk.push(character);
                used += character_width;
            }
            if !chunk.is_empty() {
                output.push(Span::styled(chunk, span.style));
            }
        }
        wrapped.push(Line::from(output));
        if wrapped.len() == MAX_RENDERED_LINES {
            return wrapped;
        }
    }
    wrapped
}

fn append_ellipsis(lines: &mut [Line<'static>]) {
    let Some(last) = lines.last_mut() else {
        return;
    };
    let style = last.spans.last().map(|span| span.style).unwrap_or_default();
    last.spans.push(Span::styled("…", style));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixed_styles_inherit_and_override_in_order_across_lines() {
        let spans = vec![
            TuiCutexRichSpan {
                text: "base ".into(),
                foreground: None,
                bold: None,
                dim: None,
                italic: None,
            },
            TuiCutexRichSpan {
                text: "{name}\nmeta".into(),
                foreground: Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98)),
                bold: Some(false),
                dim: Some(true),
                italic: Some(true),
            },
        ];
        let base = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
        let lines = render(Some(&spans), &[("name", "worker")], base).expect("rich output");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans[0].content, "base ");
        assert_eq!(lines[0].spans[0].style, base);
        assert_eq!(lines[0].spans[1].content, "worker");
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
        assert!(
            lines[0].spans[1]
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
        assert_eq!(lines[1].spans[0].content, "meta");
    }

    #[test]
    fn runtime_bounds_and_missing_values_fail_closed() {
        let span = |text: String| TuiCutexRichSpan {
            text,
            foreground: None,
            bold: None,
            dim: None,
            italic: None,
        };
        assert!(render(Some(&[span("{missing}".into())]), &[], Style::default()).is_none());
        assert!(
            render(
                Some(&[span("{value}".into())]),
                &[("value", &"x".repeat(MAX_RENDERED_CHARS + 1))],
                Style::default()
            )
            .is_none()
        );
    }
}
