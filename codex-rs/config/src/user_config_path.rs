use codex_utils_absolute_path::AbsolutePathBuf;
use std::path::Path;

use crate::CONFIG_TOML_FILE;

pub const CODEX_CONFIG_FILE_ENV: &str = "CODEX_CONFIG_FILE";

/// Returns the user config path selected by `CODEX_CONFIG_FILE`, if the
/// environment variable contains a nonblank value.
pub fn user_config_path_from_env(codex_home: &Path) -> Option<AbsolutePathBuf> {
    let value = std::env::var(CODEX_CONFIG_FILE_ENV).ok();
    user_config_path_from_value(codex_home, value.as_deref())
}

/// Resolves the active user config file for a process launched at `codex_home`.
pub fn resolve_user_config_path(codex_home: &Path) -> AbsolutePathBuf {
    user_config_path_from_env(codex_home)
        .unwrap_or_else(|| AbsolutePathBuf::resolve_path_against_base(CONFIG_TOML_FILE, codex_home))
}

pub(crate) fn user_config_path_from_value(
    codex_home: &Path,
    value: Option<&str>,
) -> Option<AbsolutePathBuf> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| AbsolutePathBuf::resolve_path_against_base(value, codex_home))
}

#[cfg(test)]
#[path = "user_config_path_tests.rs"]
mod tests;
