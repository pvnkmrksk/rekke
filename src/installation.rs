//! User-writable configuration seeds for installed apps; explicit paths stay independent.
use crate::config::{Config, SourceConfig};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

const SEEDS: [(&str, &str); 6] = [
    ("camera-a.toml", include_str!("../configs/camera-a.toml")),
    ("camera-b.toml", include_str!("../configs/camera-b.toml")),
    ("demo.toml", include_str!("../configs/demo.toml")),
    ("video.toml", include_str!("../configs/video.toml")),
    ("network.toml", include_str!("../configs/network.toml")),
    ("rigs.toml", include_str!("../configs/rigs.toml")),
];
pub fn user_directory() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support/Rekke"))
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Rekke"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("rekke"))
    }
}
pub fn installed(exe: &Path) -> bool {
    let Some(parent) = exe.parent() else {
        return false;
    };
    parent.ends_with("Contents/MacOS")
        || parent.join("defaults/configs").is_dir()
        || parent
            .parent()
            .is_some_and(|p| p.join("share/rekke/configs").is_dir())
}
pub fn seed(directory: &Path) -> Result<PathBuf> {
    let config_folder = directory.join("configs");
    std::fs::create_dir_all(&config_folder)?;
    let primary = config_folder.join("camera-a.toml");
    // Preserve previous Wingtrace app state during the rename. Videos remain
    // in their old directory; only the config and referenced baseline migrate.
    if !primary.exists()
        && let Some(parent) = directory.parent()
    {
        let old = parent.join("Wingtrace/wingtrace.toml");
        if old.is_file() {
            migrate(&old, &primary).context("Import previous Wingtrace configuration")?;
        }
    }
    for (filename, contents) in SEEDS {
        let path = config_folder.join(filename);
        if !path.exists() {
            crate::config::atomic_write(&path, contents.as_bytes())?;
        }
    }
    Ok(primary)
}
fn migrate(old: &Path, new: &Path) -> Result<()> {
    let mut config = Config::load(old)?;
    let background = config.resolve(old, &config.tracking.background_path);
    if !Path::new(&config.tracking.background_path).is_absolute() {
        if background.is_file() {
            config.tracking.background_path = "rig-a.background.png".into();
            let contents = std::fs::read(&background)?;
            crate::config::atomic_write(
                &config.resolve(new, &config.tracking.background_path),
                &contents,
            )?;
        } else {
            config.tracking.background_path = "rig-a.background.png".into();
        }
    }
    if let SourceConfig::Video { path, .. } = &config.source {
        let absolute = config.resolve(old, path).to_string_lossy().into_owned();
        if let SourceConfig::Video { path, .. } = &mut config.source {
            *path = absolute;
        }
    }
    config.save(new)
}
pub fn select(cwd: &Path, exe: Option<&Path>, data: Option<&Path>) -> Result<PathBuf> {
    // A normal drag-installed Mac bundle always uses writable user state.
    if exe
        .and_then(Path::parent)
        .is_some_and(|p| p.ends_with("Contents/MacOS"))
        && let Some(data) = data
    {
        return seed(data);
    }
    for path in [
        cwd.join("rekke.toml"),
        cwd.join("wingtrace.toml"),
        cwd.join("configs/camera-a.toml"),
    ] {
        if path.is_file() {
            return Ok(path);
        }
    }
    if let Some(exe) = exe {
        if let Some(parent) = exe.parent() {
            let portable = parent.join("configs/camera-a.toml");
            if portable.is_file() {
                return Ok(portable);
            }
        }
        if installed(exe)
            && let Some(data) = data
        {
            return seed(data);
        }
    }
    Ok(cwd.join("rekke.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_mac_bundle_uses_writable_state_and_keeps_previous_edits() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("user/Rekke");
        let exe = root
            .path()
            .join("Applications/Rekke.app/Contents/MacOS/rekke");
        let path = select(root.path(), Some(&exe), Some(&data)).unwrap();
        assert_eq!(path, data.join("configs/camera-a.toml"));
        assert!(data.join("configs/rigs.toml").is_file());
        let mut c = Config::load(&path).unwrap();
        c.head.x = 325.0;
        c.tracking.asymmetric = true;
        c.save(&path).unwrap();
        select(root.path(), Some(&exe), Some(&data)).unwrap();
        assert_eq!(Config::load(&path).unwrap(), c);
    }
    #[test]
    fn rename_migrates_exact_markers_and_baseline_without_copying_videos() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("Wingtrace/wingtrace.toml");
        let mut c = Config::demo();
        c.tracking.right.outer_radius = 173.0;
        c.tracking.background_path = "reference.png".into();
        c.save(&old).unwrap();
        let bg = crate::frame::GrayFrame::new(640, 480, vec![73; 640 * 480], 0, 0).unwrap();
        bg.save(&old.parent().unwrap().join("reference.png"))
            .unwrap();
        std::fs::create_dir(old.parent().unwrap().join("recordings")).unwrap();
        let path = seed(&root.path().join("Rekke")).unwrap();
        let next = Config::load(&path).unwrap();
        assert_eq!(next.tracking.right.outer_radius, 173.0);
        assert_eq!(next.head, c.head);
        let loaded =
            crate::frame::GrayFrame::load(&next.resolve(&path, &next.tracking.background_path))
                .unwrap();
        assert_eq!(loaded.pixels, bg.pixels);
        assert!(!root.path().join("Rekke/recordings").exists());
        assert!(old.exists());
    }
    #[test]
    fn installed_windows_and_linux_use_user_seeds_while_portable_configs_win() {
        let root = tempfile::tempdir().unwrap();
        let exe = root.path().join("program/rekke.exe");
        std::fs::create_dir_all(exe.parent().unwrap().join("defaults/configs")).unwrap();
        let path = select(root.path(), Some(&exe), Some(&root.path().join("data"))).unwrap();
        assert!(path.starts_with(root.path().join("data")));
        let portable = exe.parent().unwrap().join("configs/camera-a.toml");
        Config::default().save(&portable).unwrap();
        assert_eq!(select(root.path(), Some(&exe), None).unwrap(), portable);
        let linux = root.path().join("usr/bin/rekke");
        std::fs::create_dir_all(root.path().join("usr/share/rekke/configs")).unwrap();
        assert!(installed(&linux));
    }
}
