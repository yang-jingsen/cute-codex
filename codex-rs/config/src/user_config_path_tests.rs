use super::*;
use pretty_assertions::assert_eq;

#[test]
fn user_config_path_value_handles_blank_relative_and_absolute_paths() {
    let codex_home = tempfile::tempdir().expect("tempdir");
    let absolute = tempfile::NamedTempFile::new().expect("absolute config path");

    let actual = [
        user_config_path_from_value(codex_home.path(), None),
        user_config_path_from_value(codex_home.path(), Some("  \t")),
        user_config_path_from_value(codex_home.path(), Some(" profiles/work.toml ")),
        user_config_path_from_value(
            codex_home.path(),
            absolute
                .path()
                .to_str()
                .map(|path| format!(" {path} "))
                .as_deref(),
        ),
    ];
    let expected = [
        None,
        None,
        Some(AbsolutePathBuf::resolve_path_against_base(
            "profiles/work.toml",
            codex_home.path(),
        )),
        Some(
            AbsolutePathBuf::from_absolute_path(absolute.path())
                .expect("fixture path should be absolute"),
        ),
    ];

    assert_eq!(actual, expected);
}
