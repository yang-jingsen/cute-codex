use serde::Deserialize;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ExternalInputBinding {
    pub version: u32,
    pub owner_id: String,
    pub thread_id: String,
    pub runtime_generation: u64,
}

impl ExternalInputBinding {
    #[cfg(unix)]
    pub(crate) fn load(path: &Path) -> std::io::Result<Self> {
        use rustix::fs::Mode;
        use rustix::fs::OFlags;
        use rustix::fs::fstat;
        use rustix::fs::open;
        use rustix::fs::openat;
        use std::io::Read;
        let invalid = || std::io::Error::other("invalid private ExternalInput launch binding");
        if !path.is_absolute()
            || path.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err(invalid());
        }
        let parent = path.parent().ok_or_else(invalid)?;
        let name = path.file_name().ok_or_else(invalid)?;
        let directory = open(
            parent,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?;
        let uid = rustix::process::geteuid().as_raw();
        let metadata = fstat(&directory)?;
        if metadata.st_uid != uid || metadata.st_mode & 0o077 != 0 {
            return Err(invalid());
        }
        let descriptor = openat(
            &directory,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )?;
        let metadata = fstat(&descriptor)?;
        if metadata.st_uid != uid
            || metadata.st_mode & 0o177777 != 0o100600
            || metadata.st_size > 4096
        {
            return Err(invalid());
        }
        let mut bytes = Vec::new();
        std::fs::File::from(descriptor)
            .take(4097)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(invalid());
        }
        let binding: Self = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if binding.version != 1
            || binding.owner_id.is_empty()
            || binding.owner_id.len() > 256
            || binding.thread_id.is_empty()
            || binding.thread_id.len() > 256
        {
            return Err(invalid());
        }
        Ok(binding)
    }

    #[cfg(not(unix))]
    pub(crate) fn load(_path: &Path) -> std::io::Result<Self> {
        Err(std::io::Error::other(
            "ExternalInput requires private Unix transport",
        ))
    }
}

#[cfg(all(test, unix))]
#[path = "external_input_binding_tests.rs"]
mod tests;
