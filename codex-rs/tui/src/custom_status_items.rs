use ratatui::style::Color;
use ratatui::style::Modifier;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::text::Span;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const CODEX_CUSTOM_STATUS_ITEMS_FILE_ENV_VAR: &str = "CODEX_CUSTOM_STATUS_ITEMS_FILE";
pub(crate) const CODEX_LAUNCH_PROFILE_ENV_VAR: &str = "CODEX_LAUNCH_PROFILE";
pub(crate) const CODEX_LAUNCH_RUNTIME_ENV_VAR: &str = "CODEX_LAUNCH_RUNTIME";
pub(crate) const CODEX_LAUNCH_PROFILE_SOURCE_ENV_VAR: &str = "CODEX_LAUNCH_PROFILE_SOURCE";
pub(crate) const CODEX_LAUNCH_PROFILE_TYPE_ENV_VAR: &str = "CODEX_LAUNCH_PROFILE_TYPE";
pub(crate) const CODEX_LAUNCH_PROFILE_EMAIL_ENV_VAR: &str = "CODEX_LAUNCH_PROFILE_EMAIL";

#[derive(Debug, Clone)]
pub(crate) struct CustomStatusItem {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) description: Option<String>,
    source: CustomStatusItemSource,
    render: CustomStatusItemRender,
    style: Style,
    catalog_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum CustomStatusContext<'a> {
    StatusLine { cwd: &'a Path },
    Preview { cwd: &'a Path },
}

#[derive(Debug, Deserialize)]
struct CustomStatusItemsCatalogFile {
    #[serde(default)]
    items: Vec<CustomStatusItemConfig>,
}

#[derive(Debug, Deserialize)]
struct CustomStatusItemConfig {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    source: CustomStatusItemSource,
    #[serde(default)]
    render: CustomStatusItemRender,
    #[serde(default)]
    style: CustomStatusItemStyle,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CustomStatusItemSource {
    Static {
        value: String,
    },
    Env {
        key: String,
        #[serde(default = "default_true")]
        trim: bool,
    },
    FileText {
        path: String,
        #[serde(default = "default_true")]
        trim: bool,
    },
    LaunchProfile,
    LaunchRuntime,
    LaunchProfileSource,
    LaunchProfileType,
    LaunchProfileEmail,
    CurrentDir,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CustomStatusItemRender {
    Value,
    LabelValue { label: String },
    Template { template: String },
}

impl Default for CustomStatusItemRender {
    fn default() -> Self {
        Self::Value
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CustomStatusItemStyle {
    #[serde(default)]
    fg: Option<String>,
    #[serde(default)]
    bg: Option<String>,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    dim: bool,
    #[serde(default)]
    italic: bool,
    #[serde(default)]
    underlined: bool,
}

fn default_true() -> bool {
    true
}

impl CustomStatusItem {
    pub(crate) fn render_line(&self, context: CustomStatusContext<'_>) -> Option<Line<'static>> {
        let raw_value = self.resolve_value(context)?;
        let rendered = self.render.render(&self.id, &self.title, &raw_value);
        let trimmed = rendered.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Line::from(Span::styled(trimmed.to_string(), self.style)))
        }
    }

    fn resolve_value(&self, context: CustomStatusContext<'_>) -> Option<String> {
        let value = match &self.source {
            CustomStatusItemSource::Static { value } => Some(value.clone()),
            CustomStatusItemSource::Env { key, trim } => {
                let value = std::env::var(key).ok()?;
                Some(normalize_status_text(&value, *trim))
            }
            CustomStatusItemSource::FileText { path, trim } => {
                let resolved = self.resolve_catalog_relative_path(path);
                let text = fs::read_to_string(&resolved).ok()?;
                Some(normalize_status_text(&text, *trim))
            }
            CustomStatusItemSource::LaunchProfile => {
                let value = std::env::var(CODEX_LAUNCH_PROFILE_ENV_VAR).ok()?;
                Some(normalize_status_text(&value, /*trim*/ true))
            }
            CustomStatusItemSource::LaunchRuntime => {
                let value = std::env::var(CODEX_LAUNCH_RUNTIME_ENV_VAR).ok()?;
                Some(normalize_status_text(&value, /*trim*/ true))
            }
            CustomStatusItemSource::LaunchProfileSource => {
                let value = std::env::var(CODEX_LAUNCH_PROFILE_SOURCE_ENV_VAR).ok()?;
                Some(normalize_status_text(&value, /*trim*/ true))
            }
            CustomStatusItemSource::LaunchProfileType => {
                let value = std::env::var(CODEX_LAUNCH_PROFILE_TYPE_ENV_VAR).ok()?;
                Some(normalize_status_text(&value, /*trim*/ true))
            }
            CustomStatusItemSource::LaunchProfileEmail => {
                let value = std::env::var(CODEX_LAUNCH_PROFILE_EMAIL_ENV_VAR).ok()?;
                Some(normalize_status_text(&value, /*trim*/ true))
            }
            CustomStatusItemSource::CurrentDir => match context {
                CustomStatusContext::StatusLine { cwd } | CustomStatusContext::Preview { cwd } => {
                    Some(crate::status::format_directory_display(
                        cwd, /*max_width*/ None,
                    ))
                }
            },
        }?;

        (!value.is_empty()).then_some(value)
    }

    fn resolve_catalog_relative_path(&self, configured_path: &str) -> PathBuf {
        let path = PathBuf::from(configured_path);
        if path.is_absolute() {
            return path;
        }

        self.catalog_dir
            .as_ref()
            .map(|dir| dir.join(path.clone()))
            .unwrap_or(path)
    }
}

impl CustomStatusItemRender {
    fn render(&self, id: &str, title: &str, value: &str) -> String {
        match self {
            CustomStatusItemRender::Value => value.to_string(),
            CustomStatusItemRender::LabelValue { label } => format!("{label} {value}"),
            CustomStatusItemRender::Template { template } => template
                .replace("{id}", id)
                .replace("{title}", title)
                .replace("{value}", value),
        }
    }
}

pub(crate) fn load_custom_status_items() -> Vec<CustomStatusItem> {
    let path = match std::env::var(CODEX_CUSTOM_STATUS_ITEMS_FILE_ENV_VAR) {
        Ok(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => return Vec::new(),
    };

    let contents = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to read custom status items catalog"
            );
            return Vec::new();
        }
    };

    let catalog = match serde_json::from_str::<CustomStatusItemsCatalogFile>(&contents) {
        Ok(catalog) => catalog,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to parse custom status items catalog"
            );
            return Vec::new();
        }
    };

    let catalog_dir = path.parent().map(Path::to_path_buf);
    let mut seen = HashSet::new();
    let mut items = Vec::new();

    for item in catalog.items {
        let id = item.id.trim();
        if id.is_empty() || !seen.insert(id.to_string()) {
            continue;
        }

        let title = item.title.trim();
        items.push(CustomStatusItem {
            id: id.to_string(),
            title: if title.is_empty() {
                id.to_string()
            } else {
                title.to_string()
            },
            description: item
                .description
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            source: item.source,
            render: item.render,
            style: item.style.to_style(),
            catalog_dir: catalog_dir.clone(),
        });
    }

    items
}

impl CustomStatusItemStyle {
    fn to_style(&self) -> Style {
        let mut style = Style::default();

        if let Some(fg) = self.fg.as_deref().and_then(parse_color) {
            style = style.fg(fg);
        }
        if let Some(bg) = self.bg.as_deref().and_then(parse_color) {
            style = style.bg(bg);
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.dim {
            style = style.add_modifier(Modifier::DIM);
        }
        if self.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.underlined {
            style = style.add_modifier(Modifier::UNDERLINED);
        }

        style
    }
}

fn normalize_status_text(value: &str, trim: bool) -> String {
    let normalized = value
        .lines()
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if trim {
        normalized.trim().to_string()
    } else {
        normalized
    }
}

fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim();
    if let Some(hex) = value.strip_prefix('#') {
        return parse_hex_color(hex);
    }

    match value.to_ascii_lowercase().as_str() {
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" | "grey" => Some(Color::Gray),
        "darkgray" | "darkgrey" => Some(Color::DarkGray),
        "lightred" => Some(Color::LightRed),
        "lightgreen" => Some(Color::LightGreen),
        "lightyellow" => Some(Color::LightYellow),
        "lightblue" => Some(Color::LightBlue),
        "lightmagenta" => Some(Color::LightMagenta),
        "lightcyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        _ => None,
    }
}

fn parse_hex_color(value: &str) -> Option<Color> {
    if value.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&value[0..2], 16).ok()?;
    let g = u8::from_str_radix(&value[2..4], 16).ok()?;
    let b = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}
