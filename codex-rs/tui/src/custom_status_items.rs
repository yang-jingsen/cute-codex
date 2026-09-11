//! Loaded-once, inert statusline values from a reviewed launcher.
//!
//! These labels are presentation only. No environment, account, repository config,
//! model context, or runtime source resolution participates in their interpretation.
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::path::Path;
use std::sync::OnceLock;

use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use serde::Deserialize;

use crate::bottom_pane::StatusLineItem;

const MAX_FILE_BYTES: u64 = 8192;
static ITEMS: OnceLock<StatusItems> = OnceLock::new();

#[derive(Default)]
pub(crate) struct StatusItems(BTreeMap<String, (String, Style)>);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    items: Vec<Item>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    id: String,
    text: String,
    #[serde(default)]
    style: ItemStyle,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ItemStyle {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
    dim: bool,
    italic: bool,
    underlined: bool,
}

pub(crate) fn initialize(path: Option<&Path>) -> io::Result<()> {
    let items = path.map(StatusItems::load).transpose()?.unwrap_or_default();
    ITEMS
        .set(items)
        .map_err(|_| io::Error::other("status items already initialized"))
}

pub(crate) fn value(item: StatusLineItem) -> String {
    ITEMS.get().map_or_else(
        || format!("[{item} unavailable]"),
        |items| items.value(item),
    )
}

pub(crate) fn style(item: StatusLineItem) -> Option<Style> {
    ITEMS.get().and_then(|items| items.style(item))
}

pub(crate) fn is_custom(item: StatusLineItem) -> bool {
    matches!(
        item,
        StatusLineItem::CustomProfile | StatusLineItem::CustomBonVoyage
    )
}

impl StatusItems {
    fn load(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() || !path.symlink_metadata()?.file_type().is_file() {
            return Err(invalid(
                "status items require an absolute regular nonsymlink file",
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(invalid("status items require a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
        Self::parse(&bytes)
    }

    fn parse(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() > MAX_FILE_BYTES as usize {
            return Err(invalid("status items file exceeds 8192 bytes"));
        }
        let document: Document = serde_json::from_slice(bytes)
            .map_err(|_| invalid("invalid status items v1 JSON or unsupported fields"))?;
        if document.version != 1 || document.items.len() > 2 {
            return Err(invalid("unsupported status items version or item count"));
        }
        let mut items = BTreeMap::new();
        for item in document.items {
            if !matches!(item.id.as_str(), "custom:profile" | "custom:bon-voyage")
                || item.text.is_empty()
                || item.text.len() > 256
            {
                return Err(invalid(
                    "unsupported status item ID or text length (1..256 bytes)",
                ));
            }
            let text = item.text.chars().map(|c| {
                if c.is_control() || matches!(c, '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
                    ' '
                } else {
                    c
                }
            }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
            if text.is_empty() {
                return Err(invalid("status item text is empty after sanitation"));
            }
            let mut style = Style::default();
            if let Some(fg) = item.style.fg {
                style = style.fg(parse_color(&fg)?);
            }
            if let Some(bg) = item.style.bg {
                style = style.bg(parse_color(&bg)?);
            }
            for (enabled, modifier) in [
                (item.style.bold, Modifier::BOLD),
                (item.style.dim, Modifier::DIM),
                (item.style.italic, Modifier::ITALIC),
                (item.style.underlined, Modifier::UNDERLINED),
            ] {
                if enabled {
                    style = style.add_modifier(modifier);
                }
            }
            if items.insert(item.id, (text, style)).is_some() {
                return Err(invalid("duplicate status item ID"));
            }
        }
        Ok(Self(items))
    }

    fn value(&self, item: StatusLineItem) -> String {
        self.0
            .get(&item.to_string())
            .map_or_else(|| format!("[{item} unavailable]"), |(text, _)| text.clone())
    }

    fn style(&self, item: StatusLineItem) -> Option<Style> {
        self.0.get(&item.to_string()).map(|(_, style)| *style)
    }
}

#[allow(clippy::disallowed_methods)] // Explicit reviewed #RRGGBB preserves legacy display style.
fn parse_color(value: &str) -> io::Result<Color> {
    let hex = value
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| invalid("status item color must be #RRGGBB"))?;
    let rgb = u32::from_str_radix(hex, 16).map_err(|_| invalid("invalid status item color"))?;
    Ok(Color::Rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
#[path = "custom_status_items_tests.rs"]
mod tests;
