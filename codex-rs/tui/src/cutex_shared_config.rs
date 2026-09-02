//! TUI-only Cutex presentation preferences and pure migration fixtures.
//!
//! The default input is `~/.cutex/configs/tui.toml`; `CUTEX_TUI_CONFIG_PATH` is the explicit
//! alternate-installation/test override. A later Release Agent may use
//! [`prepare_legacy_migration`] while performing its separately authorized atomic migration.
//! This module itself never writes live configuration.

use crate::legacy_core::config::Config;
use codex_config::types::TuiCutexActivitySettings;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

const CUTEX_TUI_CONFIG_ENV: &str = "CUTEX_TUI_CONFIG_PATH";
const CUTEX_TUI_CONFIG_RELATIVE_PATH: &str = ".cutex/configs/tui.toml";
const MAX_SHARED_CONFIG_BYTES: u64 = 256 * 1024;

#[derive(Debug)]
pub(crate) struct SharedCutexConfigError(String);

impl fmt::Display for SharedCutexConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

pub(crate) fn apply_shared_cutex_tui_settings(config: &mut Config) {
    let override_path = std::env::var_os(CUTEX_TUI_CONFIG_ENV).map(PathBuf::from);
    match load_shared_cutex_tui_settings(override_path.as_deref(), dirs::home_dir().as_deref()) {
        Ok(settings) => config.tui_cutex_activity = settings,
        Err(error) => {
            config.tui_cutex_activity = TuiCutexActivitySettings::default();
            config.startup_warnings.push(format!(
                "Cutex TUI presentation config was ignored; using safe defaults: {error}"
            ));
        }
    }
}

pub(crate) fn load_shared_cutex_tui_settings(
    override_path: Option<&Path>,
    home: Option<&Path>,
) -> Result<TuiCutexActivitySettings, SharedCutexConfigError> {
    let path = match override_path {
        Some(path) => path.to_path_buf(),
        None => home
            .ok_or_else(|| SharedCutexConfigError("cannot resolve the user home directory".into()))?
            .join(CUTEX_TUI_CONFIG_RELATIVE_PATH),
    };
    let mut file = match File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(TuiCutexActivitySettings::default());
        }
        Err(error) => {
            return Err(SharedCutexConfigError(format!(
                "cannot open {}: {error}",
                path.display()
            )));
        }
    };
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_SHARED_CONFIG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            SharedCutexConfigError(format!("cannot read {}: {error}", path.display()))
        })?;
    if bytes.len() as u64 > MAX_SHARED_CONFIG_BYTES {
        return Err(SharedCutexConfigError(format!(
            "{} exceeds the 256 KiB limit",
            path.display()
        )));
    }
    let source = std::str::from_utf8(&bytes).map_err(|error| {
        SharedCutexConfigError(format!("{} is not UTF-8: {error}", path.display()))
    })?;
    let settings = toml::from_str::<TuiCutexActivitySettings>(source).map_err(|error| {
        SharedCutexConfigError(format!("{} is invalid TOML: {error}", path.display()))
    })?;
    codex_config::validate_cutex_tui_settings(&settings).map_err(|error| {
        SharedCutexConfigError(format!("{} is unsafe: {error}", path.display()))
    })?;
    Ok(settings)
}

/// Pure output for the later, separately authorized one-time migration.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct LegacyMigrationFixture {
    pub(crate) settings: TuiCutexActivitySettings,
    pub(crate) shared_toml: String,
    pub(crate) codex_toml_without_legacy_table: String,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn prepare_legacy_migration(
    codex_source: &str,
) -> Result<Option<LegacyMigrationFixture>, SharedCutexConfigError> {
    let mut document = toml::from_str::<toml::Value>(codex_source).map_err(|error| {
        SharedCutexConfigError(format!("legacy Codex config is invalid: {error}"))
    })?;
    let Some(tui) = document.get_mut("tui").and_then(toml::Value::as_table_mut) else {
        return Ok(None);
    };
    let Some(legacy) = tui.remove("cutex_activity") else {
        return Ok(None);
    };
    let settings: TuiCutexActivitySettings = legacy.try_into().map_err(|error| {
        SharedCutexConfigError(format!("legacy Cutex presentation is invalid: {error}"))
    })?;
    codex_config::validate_cutex_tui_settings(&settings).map_err(|error| {
        SharedCutexConfigError(format!("legacy Cutex presentation is unsafe: {error}"))
    })?;
    let shared_toml = toml::to_string_pretty(&settings).map_err(|error| {
        SharedCutexConfigError(format!("cannot serialize shared presentation: {error}"))
    })?;
    let verified: TuiCutexActivitySettings = toml::from_str(&shared_toml).map_err(|error| {
        SharedCutexConfigError(format!(
            "serialized shared presentation did not parse: {error}"
        ))
    })?;
    if verified != settings {
        return Err(SharedCutexConfigError(
            "serialized shared presentation changed semantics".to_string(),
        ));
    }
    let codex_toml_without_legacy_table = toml::to_string_pretty(&document).map_err(|error| {
        SharedCutexConfigError(format!("cannot serialize migrated Codex config: {error}"))
    })?;
    Ok(Some(LegacyMigrationFixture {
        settings,
        shared_toml,
        codex_toml_without_legacy_table,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_config::types::TuiCutexForegroundColor;
    use tempfile::tempdir;

    #[test]
    fn profiles_share_the_user_scoped_file_and_missing_uses_defaults() {
        let home = tempdir().expect("home");
        assert_eq!(
            load_shared_cutex_tui_settings(None, Some(home.path())).expect("missing is safe"),
            TuiCutexActivitySettings::default()
        );
        let path = home.path().join(CUTEX_TUI_CONFIG_RELATIVE_PATH);
        std::fs::create_dir_all(path.parent().expect("config parent")).expect("create namespace");
        std::fs::write(
            &path,
            "outbound_message_visible = false\n[managed_agent]\ncompleted = \"{agent} 已取餐\"\n",
        )
        .expect("write fixture");
        let first = load_shared_cutex_tui_settings(None, Some(home.path())).expect("first profile");
        let second =
            load_shared_cutex_tui_settings(None, Some(home.path())).expect("second profile");
        assert_eq!(first, second);
        assert!(!first.outbound_message_visible);
    }

    #[test]
    fn malformed_unknown_and_oversized_files_fail_safely() {
        let directory = tempdir().expect("directory");
        let path = directory.path().join("tui.toml");
        for source in ["unknown = true\n", "[create.ready]\ntemplates = []\n"] {
            std::fs::write(&path, source).expect("write fixture");
            assert!(load_shared_cutex_tui_settings(Some(&path), None).is_err());
        }
        std::fs::write(&path, vec![b'x'; MAX_SHARED_CONFIG_BYTES as usize + 1])
            .expect("write oversized fixture");
        assert!(load_shared_cutex_tui_settings(Some(&path), None).is_err());
    }

    #[test]
    fn shared_file_loads_grouped_management_and_task_templates() {
        let directory = tempdir().expect("directory");
        let path = directory.path().join("tui.toml");
        std::fs::write(
            &path,
            r#"
[create.prepared]
selection = "stable_random"
grouped_templates = [{ group = "alpha", templates = ["{agent_name} accepted"] }]

[create.ready]
selection = "stable_random"
grouped_templates = [{ group = "alpha", templates = ["{agent_name} delivered"] }]

[task_service_message.assignment]
selection = "stable_random"
grouped_templates = [{ group = "beta", templates = ["{task_name} accepted"] }]

[task_service_message.review_ready]
selection = "stable_random"
grouped_templates = [{ group = "beta", templates = ["{task_name} delivered"] }]

[task_watchdog.first_stale]
selection = "stable_random"
grouped_templates = [{ group = "gamma", templates = ["{assignee} idle {idle}"] }]

[task_watchdog.director_escalated]
selection = "stable_random"
grouped_templates = [{ group = "gamma", templates = ["{assignee} escalated"] }]
"#,
        )
        .expect("write grouped fixture");
        let loaded = load_shared_cutex_tui_settings(Some(&path), None).expect("valid grouped file");
        assert_eq!(
            loaded.create.ready.unwrap().grouped_templates.unwrap()[0].group,
            "alpha"
        );
        assert_eq!(
            loaded
                .task_service_message
                .review_ready
                .grouped_templates
                .unwrap()[0]
                .group,
            "beta"
        );
        assert_eq!(
            loaded
                .task_watchdog
                .director_escalated
                .grouped_templates
                .unwrap()[0]
                .group,
            "gamma"
        );
    }

    #[test]
    fn owner_rich_watchdog_example_loads_with_strict_truecolor() {
        let directory = tempdir().expect("directory");
        let path = directory.path().join("tui.toml");
        std::fs::write(
            &path,
            r##"
[task_watchdog.director_escalated]
rich_template = [
  { text = "task_service: " },
  { text = "我等了很久，我不会再等了。", foreground = "#98FF98" },
  { text = "\nidle-agent: {assignee} idle-time: {idle} timestamp: {timestamp}", foreground = "#D3D3D3", bold = false },
]
"##,
        )
        .expect("write rich example");
        let loaded = load_shared_cutex_tui_settings(Some(&path), None).expect("valid rich file");
        let rich = loaded
            .task_watchdog
            .director_escalated
            .rich_template
            .expect("rich template");
        assert_eq!(rich.len(), 3);
        assert_eq!(
            rich[1].foreground,
            Some(TuiCutexForegroundColor::Rgb(0x98, 0xFF, 0x98))
        );
        assert_eq!(
            rich[2].foreground,
            Some(TuiCutexForegroundColor::Rgb(0xD3, 0xD3, 0xD3))
        );
    }

    #[test]
    fn migration_fixture_transfers_exact_semantics_and_preserves_original_on_failure() {
        let source = r#"
model = "glm"
[tui]
animations = false
[tui.cutex_activity]
outbound_message_visible = false
[tui.cutex_activity.director_rotate.successor_ready]
templates = ["恭喜 {agent_name} 已经可以撑地了", "{agent_name} 堂堂登场！"]
selection = "stable_random"
"#;
        let fixture = prepare_legacy_migration(source)
            .expect("valid migration")
            .expect("legacy table");
        assert!(!fixture.settings.outbound_message_visible);
        assert!(
            !fixture
                .codex_toml_without_legacy_table
                .contains("cutex_activity")
        );
        assert!(
            fixture
                .codex_toml_without_legacy_table
                .contains("model = \"glm\"")
        );
        assert_eq!(
            toml::from_str::<TuiCutexActivitySettings>(&fixture.shared_toml)
                .expect("shared fixture"),
            fixture.settings
        );

        let invalid = "[tui.cutex_activity.create.ready]\ntemplate = \"{unknown}\"\n";
        assert!(prepare_legacy_migration(invalid).is_err());
        assert_eq!(invalid, invalid.to_string());
    }
}
