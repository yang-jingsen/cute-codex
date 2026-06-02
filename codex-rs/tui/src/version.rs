/// The display name shown by this custom Codex build.
pub const CODEX_CLI_DISPLAY_NAME: &str = "cute-codex";

/// The current Codex CLI version as embedded at compile time.
pub const CODEX_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Optional compile-time build tag for locally patched builds.
///
/// Set `CUTE_CODEX_BUILD_TAG` in the environment before `cargo build` to append
/// a short marker in the TUI header, for example `FT` or `FT-D`.
pub const CODEX_CLI_BUILD_TAG: Option<&str> = option_env!("CUTE_CODEX_BUILD_TAG");

pub fn display_version_label(version: &str) -> String {
    match CODEX_CLI_BUILD_TAG.map(str::trim) {
        Some(tag) if !tag.is_empty() => format!("v{version} {tag}"),
        _ => format!("v{version}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_version_label_uses_plain_version_when_tag_missing() {
        let tag = CODEX_CLI_BUILD_TAG.map(str::trim).unwrap_or_default();
        if tag.is_empty() {
            assert_eq!(display_version_label("0.122.0"), "v0.122.0");
        }
    }

    #[test]
    fn display_version_label_appends_tag_when_present() {
        let tag = CODEX_CLI_BUILD_TAG.map(str::trim).unwrap_or_default();
        if !tag.is_empty() {
            assert_eq!(display_version_label("0.122.0"), format!("v0.122.0 {tag}"));
        }
    }
}
