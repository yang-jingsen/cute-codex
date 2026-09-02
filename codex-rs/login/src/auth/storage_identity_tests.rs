use super::*;
use pretty_assertions::assert_eq;

#[test]
fn identities_preserve_default_and_isolate_override_paths() {
    let codex_home = tempfile::tempdir().expect("tempdir");
    let codex_home_path = codex_home.path().to_path_buf();
    let default = AuthStorageIdentity::from_value(codex_home_path.clone(), None);
    let blank = AuthStorageIdentity::from_value(codex_home_path.clone(), Some("  \t"));
    let first = AuthStorageIdentity::from_value(
        codex_home_path.clone(),
        Some(" profiles/first-auth.json "),
    );
    let first_again =
        AuthStorageIdentity::from_value(codex_home_path.clone(), first.auth_file.to_str());
    let second =
        AuthStorageIdentity::from_value(codex_home_path.clone(), Some("profiles/second-auth.json"));
    std::fs::create_dir_all(
        first
            .auth_file
            .parent()
            .expect("override auth file should have a parent"),
    )
    .expect("create override auth parent");
    std::fs::write(&first.auth_file, "{}").expect("create override auth file");
    let first_after_creation =
        AuthStorageIdentity::from_value(codex_home_path, first.auth_file.to_str());

    assert_eq!(default, blank);
    assert_eq!(default.auth_file, codex_home.path().join("auth.json"));
    assert_eq!(
        default.direct_store_key,
        format!(
            "cli|{}",
            short_path_hash(&canonical_or_original(codex_home.path()))
        )
    );
    assert_eq!(default.encrypted_secret_name.as_str(), "CODEX_AUTH");
    assert_eq!(first, first_again);
    assert_eq!(first, first_after_creation);
    assert_eq!(
        first.auth_file,
        codex_home.path().join("profiles/first-auth.json")
    );
    assert_ne!(first.direct_store_key, default.direct_store_key);
    assert_ne!(first.direct_store_key, second.direct_store_key);
    assert_ne!(first.encrypted_secret_name, default.encrypted_secret_name);
    assert_ne!(first.encrypted_secret_name, second.encrypted_secret_name);
}
