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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum CustomStatusItemRender {
    #[default]
    Value,
    LabelValue {
        label: String,
    },
    Template {
        template: String,
    },
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
        (!trimmed.is_empty()).then(|| Line::from(Span::styled(trimmed.to_string(), self.style)))
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
                launch_environment_value(CODEX_LAUNCH_PROFILE_ENV_VAR)
            }
            CustomStatusItemSource::LaunchRuntime => {
                launch_environment_value(CODEX_LAUNCH_RUNTIME_ENV_VAR)
            }
            CustomStatusItemSource::LaunchProfileSource => {
                launch_environment_value(CODEX_LAUNCH_PROFILE_SOURCE_ENV_VAR)
            }
            CustomStatusItemSource::LaunchProfileType => {
                launch_environment_value(CODEX_LAUNCH_PROFILE_TYPE_ENV_VAR)
            }
            CustomStatusItemSource::LaunchProfileEmail => {
                launch_environment_value(CODEX_LAUNCH_PROFILE_EMAIL_ENV_VAR)
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

    match parse_custom_status_items(&contents, path.parent().map(Path::to_path_buf)) {
        Ok(items) => items,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                error = %err,
                "failed to parse custom status items catalog"
            );
            Vec::new()
        }
    }
}

fn parse_custom_status_items(
    contents: &str,
    catalog_dir: Option<PathBuf>,
) -> serde_json::Result<Vec<CustomStatusItem>> {
    let catalog = serde_json::from_str::<CustomStatusItemsCatalogFile>(contents)?;
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

    Ok(items)
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

fn launch_environment_value(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| normalize_status_text(&value, /*trim*/ true))
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

// The deployed catalog contract accepts explicit #RRGGBB values in addition to
// theme-friendly named ANSI colors.
#[allow(clippy::disallowed_methods)]
fn parse_hex_color(value: &str) -> Option<Color> {
    if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    let red = u8::from_str_radix(&value[0..2], 16).ok()?;
    let green = u8::from_str_radix(&value[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(Color::Rgb(red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use tempfile::tempdir;

    struct EnvVarGuard {
        key: &'static str,
        original: Option<std::ffi::OsString>,
    }

    impl EnvVarGuard {
        fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
            let original = std::env::var_os(key);
            // SAFETY: these tests use one serial-test key and restore every variable on drop.
            unsafe { std::env::set_var(key, value) };
            Self { key, original }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            // SAFETY: these tests use one serial-test key and restore every variable on drop.
            unsafe {
                match self.original.take() {
                    Some(value) => std::env::set_var(self.key, value),
                    None => std::env::remove_var(self.key),
                }
            }
        }
    }

    fn line_text(item: &CustomStatusItem, cwd: &Path) -> Option<String> {
        item.render_line(CustomStatusContext::StatusLine { cwd })
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
    }

    #[test]
    fn catalog_normalizes_metadata_and_keeps_first_nonblank_id() {
        let items = parse_custom_status_items(
            r#"{
                "items": [
                    {
                        "id": "  custom:one  ",
                        "title": "  One  ",
                        "description": "  First item  ",
                        "source": { "kind": "static", "value": "alpha" }
                    },
                    {
                        "id": "custom:one",
                        "title": "Duplicate",
                        "source": { "kind": "static", "value": "ignored" }
                    },
                    {
                        "id": " ",
                        "title": "Blank",
                        "source": { "kind": "static", "value": "ignored" }
                    },
                    {
                        "id": "custom:fallback-title",
                        "title": " ",
                        "description": " ",
                        "source": { "kind": "static", "value": "beta" }
                    }
                ]
            }"#,
            /*catalog_dir*/ None,
        )
        .expect("catalog should parse");

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "custom:one");
        assert_eq!(items[0].title, "One");
        assert_eq!(items[0].description.as_deref(), Some("First item"));
        assert_eq!(items[1].title, "custom:fallback-title");
        assert_eq!(items[1].description, None);
    }

    #[test]
    fn file_source_is_catalog_relative_and_template_normalizes_multiline_text() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("status.txt"), " first \n\n second \n")
            .expect("status file should write");
        let items = parse_custom_status_items(
            r#"{
                "items": [{
                    "id": "custom:file",
                    "title": "File",
                    "source": { "kind": "file_text", "path": "status.txt" },
                    "render": {
                        "kind": "template",
                        "template": "{title} [{id}] {value}"
                    }
                }]
            }"#,
            Some(dir.path().to_path_buf()),
        )
        .expect("catalog should parse");

        assert_eq!(
            line_text(&items[0], dir.path()),
            Some("File [custom:file] first second".to_string())
        );
    }

    #[test]
    #[serial(custom_status_environment)]
    fn environment_and_launch_sources_render_all_public_values() {
        let _custom = EnvVarGuard::set("CUTE_CODEX_STATUS_TEST_VALUE", "  custom \n value ");
        let _profile = EnvVarGuard::set(CODEX_LAUNCH_PROFILE_ENV_VAR, "profile-a");
        let _runtime = EnvVarGuard::set(CODEX_LAUNCH_RUNTIME_ENV_VAR, "host");
        let _source = EnvVarGuard::set(CODEX_LAUNCH_PROFILE_SOURCE_ENV_VAR, "managed");
        let _profile_type = EnvVarGuard::set(CODEX_LAUNCH_PROFILE_TYPE_ENV_VAR, "team");
        let _email = EnvVarGuard::set(CODEX_LAUNCH_PROFILE_EMAIL_ENV_VAR, "a@example.test");
        let dir = tempdir().expect("tempdir");
        let items = parse_custom_status_items(
            r#"{
                "items": [
                    { "id": "env", "title": "env", "source": { "kind": "env", "key": "CUTE_CODEX_STATUS_TEST_VALUE" } },
                    { "id": "profile", "title": "profile", "source": { "kind": "launch_profile" } },
                    { "id": "runtime", "title": "runtime", "source": { "kind": "launch_runtime" } },
                    { "id": "source", "title": "source", "source": { "kind": "launch_profile_source" } },
                    { "id": "type", "title": "type", "source": { "kind": "launch_profile_type" } },
                    { "id": "email", "title": "email", "source": { "kind": "launch_profile_email" } },
                    { "id": "cwd", "title": "cwd", "source": { "kind": "current_dir" } }
                ]
            }"#,
            /*catalog_dir*/ None,
        )
        .expect("catalog should parse");
        let values = items
            .iter()
            .map(|item| line_text(item, dir.path()).expect("source should resolve"))
            .collect::<Vec<_>>();

        assert_eq!(
            &values[..6],
            [
                "custom value",
                "profile-a",
                "host",
                "managed",
                "team",
                "a@example.test",
            ]
        );
        assert_eq!(
            values[6],
            crate::status::format_directory_display(dir.path(), None)
        );
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn label_render_and_explicit_style_are_preserved() {
        let items = parse_custom_status_items(
            r##"{
                "items": [{
                    "id": "custom:styled",
                    "title": "Styled",
                    "source": { "kind": "static", "value": "value" },
                    "render": { "kind": "label_value", "label": "Label" },
                    "style": {
                        "fg": "#F6A3C8",
                        "bg": "blue",
                        "bold": true,
                        "dim": true,
                        "italic": true,
                        "underlined": true
                    }
                }]
            }"##,
            /*catalog_dir*/ None,
        )
        .expect("catalog should parse");
        let line = items[0]
            .render_line(CustomStatusContext::Preview {
                cwd: Path::new("/repo"),
            })
            .expect("static value should render");
        let span = &line.spans[0];

        assert_eq!(span.content, "Label value");
        assert_eq!(span.style.fg, Some(Color::Rgb(246, 163, 200)));
        assert_eq!(span.style.bg, Some(Color::Blue));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
        assert!(span.style.add_modifier.contains(Modifier::DIM));
        assert!(span.style.add_modifier.contains(Modifier::ITALIC));
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn invalid_and_non_ascii_colors_are_ignored_without_panicking() {
        for value in ["#12345", "#GG0000", "#ééé", "unknown"] {
            assert_eq!(parse_color(value), None, "unexpected color for {value:?}");
        }
    }

    #[test]
    #[serial(custom_status_environment)]
    fn public_loader_uses_environment_catalog_and_fails_closed() {
        let dir = tempdir().expect("tempdir");
        let catalog = dir.path().join("items.json");
        fs::write(
            &catalog,
            r#"{
                "items": [{
                    "id": "custom:loaded",
                    "title": "Loaded",
                    "source": { "kind": "static", "value": "ok" }
                }]
            }"#,
        )
        .expect("catalog should write");
        let _catalog = EnvVarGuard::set(CODEX_CUSTOM_STATUS_ITEMS_FILE_ENV_VAR, &catalog);

        assert_eq!(load_custom_status_items().len(), 1);
        fs::write(&catalog, "not json").expect("invalid catalog should write");
        assert!(load_custom_status_items().is_empty());
    }
}
