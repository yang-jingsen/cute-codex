/// The display name shown by this custom Codex build.
pub const CODEX_CLI_DISPLAY_NAME: &str = "cute-codex";

/// The current Codex CLI version as embedded at compile time.
pub const CODEX_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Optional compile-time build tag for locally patched builds.
///
/// Set `CUTE_CODEX_BUILD_TAG` before compiling to append a short marker to
/// the TUI's version label without changing the upstream semver.
pub const CODEX_CLI_BUILD_TAG: Option<&str> = option_env!("CUTE_CODEX_BUILD_TAG");

pub fn display_version_label(version: &str) -> String {
    display_version_label_with_tag(version, CODEX_CLI_BUILD_TAG)
}

fn display_version_label_with_tag(version: &str, build_tag: Option<&str>) -> String {
    match build_tag.map(str::trim) {
        Some(tag) if !tag.is_empty() => format!("v{version} {tag}"),
        _ => format!("v{version}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_version_label_omits_missing_or_blank_tag() {
        assert_eq!(display_version_label_with_tag("0.146.0", None), "v0.146.0");
        assert_eq!(
            display_version_label_with_tag("0.146.0", Some("  \t")),
            "v0.146.0"
        );
    }

    #[test]
    fn display_version_label_appends_trimmed_tag() {
        assert_eq!(
            display_version_label_with_tag("0.146.0", Some("  FT-D  ")),
            "v0.146.0 FT-D"
        );
    }

    #[test]
    fn display_version_label_uses_compiled_build_tag() {
        let expected = match CODEX_CLI_BUILD_TAG.map(str::trim) {
            Some(tag) if !tag.is_empty() => format!("v0.146.0 {tag}"),
            _ => "v0.146.0".to_string(),
        };
        assert_eq!(display_version_label("0.146.0"), expected);
    }
}
