//! One running instance per config file.
//!
//! Each window or headless instance holds an OS file lock beside its config
//! (`.<name>.lock`). The OS releases it when the process exits or crashes, so
//! there is nothing to clean up. A second Rekke started without `--config`
//! uses the next free rig config instead of fighting over a camera and ports.
use anyhow::{Result, bail};
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

pub struct ConfigLock {
    _file: File,
    path: PathBuf,
}
impl ConfigLock {
    pub fn lock_path(config: &Path) -> PathBuf {
        let name = config
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "rekke.toml".into());
        config.with_file_name(format!(".{name}.lock"))
    }
    /// `Ok(None)` if another running instance holds this config.
    pub fn try_acquire(config: &Path) -> Result<Option<Self>> {
        let path = Self::lock_path(config);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let file = match OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            // Read-only folders cannot hold locks; do not block the user.
            Err(_) => return Ok(Some(Self::unlocked(path))),
        };
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file, path })),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            // Filesystems without lock support: proceed without exclusivity.
            Err(std::fs::TryLockError::Error(_)) => Ok(Some(Self { _file: file, path })),
        }
    }
    fn unlocked(path: PathBuf) -> Self {
        Self {
            _file: tempfile::tempfile().expect("temporary file"),
            path,
        }
    }
    pub fn acquire(config: &Path) -> Result<Self> {
        match Self::try_acquire(config)? {
            Some(lock) => Ok(lock),
            None => bail!(
                "{} is already open in another Rekke window or rig. Close it first, or start a different config (rekke --config other.toml).",
                config.display()
            ),
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_config_can_be_held_by_one_instance_at_a_time() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("camera-a.toml");
        let first = ConfigLock::acquire(&config).unwrap();
        assert_eq!(first.path(), dir.path().join(".camera-a.toml.lock"));
        assert!(ConfigLock::try_acquire(&config).unwrap().is_none());
        assert!(
            ConfigLock::acquire(&config)
                .err()
                .unwrap()
                .to_string()
                .contains("already open")
        );
        let other = ConfigLock::try_acquire(&dir.path().join("camera-b.toml")).unwrap();
        assert!(other.is_some());
        drop(first);
        assert!(ConfigLock::try_acquire(&config).unwrap().is_some());
    }
}
