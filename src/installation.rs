//! User-writable configuration seeds for installed apps; explicit paths stay independent.
use crate::{
    config::{Config, SourceConfig},
    lock::ConfigLock,
};
use anyhow::{Context, Result, bail};
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
/// The preferred config if no other window holds it, else the next free
/// `camera-*.toml` beside it. When every rig config is open, create the next
/// one (camera-c.toml: camera 2, ports +2, its own background).
pub fn free_config(preferred: &Path) -> Result<(PathBuf, ConfigLock)> {
    if let Some(lock) = ConfigLock::try_acquire(preferred)? {
        return Ok((preferred.to_path_buf(), lock));
    }
    let folder = preferred.parent().unwrap_or(Path::new("."));
    let rig_name = |p: &Path| {
        p.file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("camera-")?.strip_suffix(".toml"))
            .map(str::to_owned)
    };
    let mut others: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p != preferred && rig_name(p).is_some())
                .collect()
        })
        .unwrap_or_default();
    others.sort();
    for path in others {
        if Config::load(&path).is_ok()
            && let Some(lock) = ConfigLock::try_acquire(&path)?
        {
            return Ok((path, lock));
        }
    }
    if rig_name(preferred).is_some() && preferred.is_file() {
        let template = Config::load(preferred)?;
        for (index, letter) in ('a'..='z').enumerate() {
            let path = folder.join(format!("camera-{letter}.toml"));
            if path.exists() {
                continue;
            }
            let config = next_rig(&template, index as u16, letter);
            config.save(&path)?;
            let lock = ConfigLock::acquire(&path)?;
            return Ok((path, lock));
        }
    }
    bail!(
        "{} is already open in another Rekke window. Close it, or start another config with --config.",
        preferred.display()
    )
}
/// A copy of `template` for rig number `index`: its own camera, ports and files.
pub fn next_rig(template: &Config, index: u16, letter: char) -> Config {
    let mut c = template.clone();
    c.instance = format!("rig-{letter}");
    c.tracking.background_path = format!("rig-{letter}.background.png");
    if let SourceConfig::Camera { device, .. } = &mut c.source {
        *device = index.to_string();
    }
    let shift = |address: &str, base: u16| {
        address
            .rsplit_once(':')
            .map(|(host, _)| format!("{host}:{}", base + index))
            .unwrap_or_else(|| address.to_owned())
    };
    c.output.bind = shift(&c.output.bind, 5501);
    c.output.target = shift(&c.output.target, 5601);
    c.output.zmq_bind = shift(&c.output.zmq_bind, 9871);
    c.output.jsonl_path.clear();
    c.recording.enabled = false;
    c
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
    fn seeded_configs_parse_match_the_defaults_and_keep_comments_on_save() {
        let root = tempfile::tempdir().unwrap();
        let path = seed(root.path()).unwrap();
        let folder = path.parent().unwrap();
        for (name, _) in SEEDS {
            if name != "rigs.toml" {
                Config::load(&folder.join(name)).unwrap();
            }
        }
        assert_eq!(Config::load(&path).unwrap(), Config::default());
        let mut c = Config::load(&path).unwrap();
        c.head.x = 333.0;
        c.tracking.left.method = crate::config::Method::Edge;
        c.save(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("# Name shown in the window title"));
        assert!(text.contains("# device: \"auto\""));
        assert_eq!(Config::load(&path).unwrap(), c);
        let rigs = crate::rigs::RigSet::load(&folder.join("rigs.toml")).unwrap();
        assert_eq!(rigs.rigs.len(), 2);
    }
    #[test]
    fn a_second_window_takes_the_next_rig_and_a_third_creates_one() {
        let root = tempfile::tempdir().unwrap();
        let first = seed(root.path()).unwrap();
        let (a, _lock_a) = free_config(&first).unwrap();
        assert_eq!(a, first);
        let (b, _lock_b) = free_config(&first).unwrap();
        assert!(b.ends_with("camera-b.toml"));
        let (c, _lock_c) = free_config(&first).unwrap();
        assert!(c.ends_with("camera-c.toml"));
        let rig = Config::load(&c).unwrap();
        assert_eq!(rig.instance, "rig-c");
        assert!(matches!(&rig.source, SourceConfig::Camera { device, .. } if device == "2"));
        assert_eq!(rig.output.zmq_bind, "tcp://*:9873");
        assert_eq!(rig.output.bind, "127.0.0.1:5503");
        assert_eq!(rig.tracking.background_path, "rig-c.background.png");
        drop(_lock_b);
        let (again, _) = free_config(&first).unwrap();
        assert!(again.ends_with("camera-b.toml"), "a closed rig is reused");
    }
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
