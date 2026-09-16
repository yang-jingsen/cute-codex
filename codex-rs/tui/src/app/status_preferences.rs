//! Optional Cutex destination for presentation preferences only.
use std::ffi::OsString;
use std::path::Path;

use crate::legacy_core::config::edit::ConfigEditsBuilder;

pub(super) async fn save(
    default_path: &Path,
    shared_path: Option<OsString>,
    ids: &[String],
    use_theme_colors: bool,
) -> anyhow::Result<()> {
    let path = shared_path
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| default_path.to_path_buf());
    ConfigEditsBuilder::for_config_path(&path)
        .with_edits([
            crate::legacy_core::config::edit::status_line_items_edit(ids),
            crate::legacy_core::config::edit::status_line_use_colors_edit(use_theme_colors),
        ])
        .apply()
        .await?;
    Ok(())
}

#[cfg(test)]
#[path = "status_preferences_tests.rs"]
mod tests;
