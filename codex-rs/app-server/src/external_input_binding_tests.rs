#![expect(clippy::unwrap_used, reason = "private binding file fixture setup")]
use super::*;
use std::os::unix::fs::PermissionsExt;

fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("binding.json");
    std::fs::write(
        &path,
        r#"{"version":1,"ownerId":"owner","threadId":"thread","runtimeGeneration":7}"#,
    )
    .unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    (dir, path)
}

#[test]
fn binding_is_a_validated_launch_snapshot() {
    let (_dir, path) = fixture();
    let binding = ExternalInputBinding::load(&path).unwrap();
    std::fs::write(&path, "{}").unwrap();
    assert_eq!(binding.owner_id, "owner");
    assert_eq!(binding.runtime_generation, 7);
    assert!(ExternalInputBinding::load(&path).is_err());
}

#[test]
fn rejects_nonprivate_directory_file_and_symlinks() {
    let (dir, path) = fixture();
    for mode in [0o644, 0o660, 0o400, 0o1600] {
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(ExternalInputBinding::load(&path).is_err());
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(ExternalInputBinding::load(&path).is_err());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let alias = dir.path().join("alias");
    std::os::unix::fs::symlink(&path, &alias).unwrap();
    assert!(ExternalInputBinding::load(&alias).is_err());
    let parent_alias = dir.path().join("parent-alias");
    std::os::unix::fs::symlink(dir.path(), &parent_alias).unwrap();
    assert!(ExternalInputBinding::load(&parent_alias.join("binding.json")).is_err());
}

#[test]
fn rejects_unknown_fields_version_and_unbounded_launch_data() {
    let (_dir, path) = fixture();
    for text in [
        r#"{"version":2,"ownerId":"owner","threadId":"thread","runtimeGeneration":7}"#.to_string(),
        r#"{"version":1,"ownerId":"owner","threadId":"thread","runtimeGeneration":7,"role":"system"}"#.to_string(),
        " ".repeat(4097),
    ] {
        std::fs::write(&path, text).unwrap();
        assert!(ExternalInputBinding::load(&path).is_err());
    }
}
