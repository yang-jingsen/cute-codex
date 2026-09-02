use codex_secrets::SecretName;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::path::PathBuf;

pub const CODEX_AUTH_FILE_ENV: &str = "CODEX_AUTH_FILE";
const DEFAULT_AUTH_FILE_NAME: &str = "auth.json";
const DEFAULT_AUTH_SECRET_NAME: &str = "CODEX_AUTH";

/// Returns the auth file selected by `CODEX_AUTH_FILE`, if the environment
/// variable contains a nonblank value.
pub fn auth_file_path_from_env(codex_home: &Path) -> Option<PathBuf> {
    let value = std::env::var(CODEX_AUTH_FILE_ENV).ok();
    auth_file_path_from_value(codex_home, value.as_deref())
}

/// Resolves the auth file for a process launched at `codex_home`.
pub fn resolve_auth_file_path(codex_home: &Path) -> PathBuf {
    auth_file_path_from_env(codex_home).unwrap_or_else(|| codex_home.join(DEFAULT_AUTH_FILE_NAME))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AuthStorageIdentity {
    pub(super) codex_home: PathBuf,
    pub(super) auth_file: PathBuf,
    pub(super) direct_store_key: String,
    pub(super) encrypted_secret_name: SecretName,
}

impl AuthStorageIdentity {
    pub(super) fn from_env(codex_home: PathBuf) -> Self {
        let value = std::env::var(CODEX_AUTH_FILE_ENV).ok();
        Self::from_value(codex_home, value.as_deref())
    }

    pub(super) fn from_value(codex_home: PathBuf, value: Option<&str>) -> Self {
        let auth_file_override = auth_file_path_from_value(&codex_home, value);
        let auth_file = auth_file_override
            .clone()
            .unwrap_or_else(|| codex_home.join(DEFAULT_AUTH_FILE_NAME));
        let identity_path = auth_file_override
            .as_deref()
            .map(stable_override_identity_path)
            .unwrap_or_else(|| canonical_or_original(&codex_home));
        let path_hash = short_path_hash(&identity_path);
        let encrypted_secret_name = auth_file_override.map_or_else(
            || secret_name(DEFAULT_AUTH_SECRET_NAME),
            |_| {
                secret_name(&format!(
                    "{DEFAULT_AUTH_SECRET_NAME}_{}",
                    path_hash.to_uppercase()
                ))
            },
        );

        Self {
            codex_home,
            auth_file,
            direct_store_key: format!("cli|{path_hash}"),
            encrypted_secret_name,
        }
    }
}

fn auth_file_path_from_value(codex_home: &Path, value: Option<&str>) -> Option<PathBuf> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            codex_config::AbsolutePathBuf::resolve_path_against_base(value, codex_home)
                .to_path_buf()
        })
}

fn stable_override_identity_path(path: &Path) -> PathBuf {
    let mut current = path;
    let mut missing_components = Vec::new();
    loop {
        if let Ok(mut canonical) = current.canonicalize() {
            for component in missing_components.iter().rev() {
                canonical.push(component);
            }
            return canonical;
        }
        let Some(file_name) = current.file_name() else {
            return path.to_path_buf();
        };
        missing_components.push(file_name.to_os_string());
        let Some(parent) = current.parent() else {
            return path.to_path_buf();
        };
        current = parent;
    }
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn short_path_hash(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    hex.get(..16).unwrap_or(&hex).to_string()
}

fn secret_name(value: &str) -> SecretName {
    SecretName::new(value)
        .unwrap_or_else(|err| unreachable!("generated auth secret name should be valid: {err}"))
}

#[cfg(test)]
#[path = "storage_identity_tests.rs"]
mod tests;
