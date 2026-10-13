//! Launch several independent rigs from one manifest (`rigs.toml`).
//!
//! Each `[[rigs]]` entry runs as its own process with its own window (or
//! headless), camera, ports, background and recordings. Settings in the
//! manifest (label, device or video, ports) apply to that run; markers and
//! detector settings live in the rig's own config file, which is created
//! from defaults the first time if it does not exist.
use crate::{
    config::{Config, Overrides, SourceConfig, camera_source},
    installation::next_rig,
    lock::ConfigLock,
    recording::safe_name,
};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigSet {
    pub rigs: Vec<RigEntry>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigEntry {
    /// Name shown in the window title, logs and recording folders.
    #[serde(default)]
    pub label: Option<String>,
    /// This rig's config file (markers, detector, background). Defaults to
    /// `<label>.toml` beside the manifest; created if missing.
    #[serde(default)]
    pub config: Option<PathBuf>,
    /// Camera: "auto", a number from `rekkefly --list-cameras`, a device path
    /// or part of the camera name.
    #[serde(default)]
    pub device: Option<String>,
    /// Replay this video instead of a camera (relative to the manifest).
    #[serde(default)]
    pub video: Option<PathBuf>,
    /// Kinefly-format ZMQ publisher port.
    #[serde(default)]
    pub zmq_port: Option<u16>,
    /// Local UDP sender port for full measurements.
    #[serde(default)]
    pub udp_port: Option<u16>,
    /// UDP receiver IP:port for full measurements.
    #[serde(default)]
    pub udp_target: Option<String>,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub record: bool,
}
fn enabled() -> bool {
    true
}
/// One rig ready to start: its config file, effective settings and overrides.
#[derive(Clone, Debug)]
pub struct Rig {
    pub path: PathBuf,
    pub config: Config,
    pub overrides: Overrides,
    pub record: bool,
}
impl RigSet {
    pub fn load(path: &Path) -> Result<Self> {
        toml::from_str(
            &std::fs::read_to_string(path)
                .with_context(|| format!("Read rig manifest {}", path.display()))?,
        )
        .with_context(|| format!("Parse rig manifest {}", path.display()))
    }
    /// Complete preflight before opening cameras or starting any children.
    /// Creates missing rig configs, then rejects any shared camera, port,
    /// label, background or log file.
    pub fn resolve(&self, manifest: &Path, demo: bool) -> Result<Vec<Rig>> {
        let folder = manifest.parent().unwrap_or(Path::new("."));
        let mut paths = HashSet::new();
        let mut names = HashSet::new();
        let mut udp = HashSet::new();
        let mut zmq = HashSet::new();
        let mut cameras = HashSet::new();
        let mut backgrounds = HashSet::new();
        let mut logs = HashSet::new();
        let mut result = Vec::new();
        for (index, rig) in self.rigs.iter().enumerate().filter(|(_, r)| r.enabled) {
            let number = index + 1;
            ensure!(
                rig.device.is_none() || rig.video.is_none(),
                "Rig {number}: set either device or video, not both"
            );
            let relative = match (&rig.config, &rig.label) {
                (Some(config), _) => config.clone(),
                (None, Some(label)) => PathBuf::from(format!("{}.toml", safe_name(label))),
                (None, None) => bail!("Rig {number}: give it a label or a config file"),
            };
            let path = folder.join(&relative);
            if !path.exists() {
                let letter = (b'a' + (index % 26) as u8) as char;
                let mut fresh = next_rig(&Config::default(), index as u16, letter);
                if let Some(label) = &rig.label {
                    fresh.instance = label.clone();
                    fresh.tracking.background_path = format!("{}.background.png", safe_name(label));
                }
                fresh
                    .save(&path)
                    .with_context(|| format!("Create rig config {}", path.display()))?;
                println!("Created {} for rig {number}", path.display());
            }
            let path = path.canonicalize().context("Resolve rig configuration")?;
            ensure!(
                paths.insert(path.clone()),
                "Each rig needs its own config: {}",
                path.display()
            );
            let saved = Config::load(&path).with_context(|| format!("Rig {number}"))?;
            let mut overrides = Overrides {
                label: rig.label.clone(),
                zmq_port: rig.zmq_port,
                udp_port: rig.udp_port,
                udp_target: rig.udp_target.clone(),
                record: rig.record,
                ..Default::default()
            };
            if let Some(device) = &rig.device {
                overrides.source = Some(camera_source(&saved, device));
            }
            if let Some(video) = &rig.video {
                let video = folder.join(video);
                ensure!(
                    video.is_file(),
                    "Rig {number}: video {} not found",
                    video.display()
                );
                let (width, height, fps) = saved.source.dimensions();
                overrides.source = Some(SourceConfig::Video {
                    path: video.canonicalize()?.to_string_lossy().into_owned(),
                    width,
                    height,
                    fps,
                    repeat: false,
                    realtime: true,
                });
            }
            let config = overrides.apply(&saved);
            config
                .validate()
                .with_context(|| format!("Rig {number} ({})", config.instance))?;
            ensure!(
                names.insert(config.instance.clone()),
                "Two rigs are labelled {}",
                config.instance
            );
            if config.output.enabled {
                let address: std::net::SocketAddr = config.output.bind.parse()?;
                ensure!(
                    udp.insert(address.port()),
                    "Rigs share UDP sender port {}; give each rig its own udp_port",
                    address.port()
                );
            }
            if config.output.zmq_enabled {
                let (_, port) = crate::zmq_output::parse_endpoint(&config.output.zmq_bind)?;
                ensure!(
                    zmq.insert(port),
                    "Rigs share ZMQ port {port}; give each rig its own zmq_port"
                );
            }
            if !demo && let SourceConfig::Camera { device, .. } = &config.source {
                let key = match crate::camera::parse(device) {
                    crate::camera::Selector::Auto => "0".to_owned(),
                    _ => device.trim().to_owned(),
                };
                ensure!(
                    cameras.insert(key),
                    "Rigs share camera {device}; give each rig its own device (see rekkefly --list-cameras)"
                );
            }
            ensure!(
                backgrounds.insert(absolute_path(
                    &config.resolve(&path, &config.tracking.background_path)
                )),
                "Rigs share a background file"
            );
            if !config.output.jsonl_path.is_empty() {
                ensure!(
                    logs.insert(absolute_path(
                        &config.resolve(&path, &config.output.jsonl_path)
                    )),
                    "Rigs share a JSONL output file"
                );
            }
            result.push(Rig {
                path,
                config,
                overrides,
                record: rig.record,
            });
        }
        ensure!(!result.is_empty(), "No enabled rigs in manifest");
        Ok(result)
    }
}
fn absolute_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                result.pop();
            }
            std::path::Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}
#[derive(Clone, Copy, Default)]
pub struct LaunchOptions {
    pub headless: bool,
    pub record: bool,
    pub demo: bool,
    pub frames: Option<u64>,
}
/// Command-line arguments that start one rig as a child process.
pub fn child_arguments(rig: &Rig, options: LaunchOptions) -> Vec<String> {
    let mut args = vec![
        "--config".into(),
        rig.path.to_string_lossy().into_owned(),
        "--supervised".into(),
    ];
    let o = &rig.overrides;
    if let Some(label) = &o.label {
        args.extend(["--label".into(), label.clone()]);
    }
    // --demo replaces every rig's source with the simulated fly.
    match &o.source {
        _ if options.demo => {}
        Some(SourceConfig::Camera { device, .. }) => {
            args.extend(["--device".into(), device.clone()]);
        }
        Some(SourceConfig::Video { path, .. }) => {
            args.extend(["--video".into(), path.clone()]);
        }
        _ => {}
    }
    if let Some(port) = o.zmq_port {
        args.extend(["--zmq-port".into(), port.to_string()]);
    }
    if let Some(port) = o.udp_port {
        args.extend(["--udp-port".into(), port.to_string()]);
    }
    if let Some(target) = &o.udp_target {
        args.extend(["--udp-target".into(), target.clone()]);
    }
    if options.headless {
        args.push("--headless".into());
    }
    if options.record || rig.record {
        args.push("--record".into());
    }
    if options.demo {
        args.push("--demo".into());
    }
    if let Some(frames) = options.frames {
        args.extend(["--frames".into(), frames.to_string()]);
    }
    args
}
fn summary(config: &Config) -> String {
    let source = match &config.source {
        SourceConfig::Camera { device, .. } => format!("camera {device}"),
        SourceConfig::Video { path, .. } => format!("video {path}"),
        SourceConfig::Network { url, .. } => format!("stream {url}"),
        SourceConfig::Demo { .. } => "simulated fly".into(),
    };
    let mut outputs = Vec::new();
    if config.output.zmq_enabled {
        outputs.push(format!("ZMQ {}", config.output.zmq_bind));
    }
    if config.output.enabled {
        outputs.push(format!(
            "UDP {} → {}",
            config.output.bind, config.output.target
        ));
    }
    format!("{source} · {}", outputs.join(" · "))
}
struct Children(Vec<(String, Child)>);
impl Children {
    fn shutdown(&mut self) {
        for (_, child) in &mut self.0 {
            if let Some(mut input) = child.stdin.take() {
                let _ = input.write_all(b"stop\n");
            }
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        for (_, child) in &mut self.0 {
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Err(_) => break,
                    _ => {}
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        self.shutdown();
    }
}
pub fn launch(manifest: &Path, options: LaunchOptions) -> Result<()> {
    let rigs = RigSet::load(manifest)?.resolve(manifest, options.demo)?;
    for rig in &rigs {
        ensure!(
            ConfigLock::try_acquire(&rig.path)?.is_some(),
            "{} ({}) is already open in another Rekkefly window; close it first",
            rig.config.instance,
            rig.path.display()
        );
    }
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    let exe = std::env::current_exe()?;
    let mut children = Children(Vec::new());
    for rig in &rigs {
        let child = Command::new(&exe)
            .args(child_arguments(rig, options))
            .stdin(Stdio::piped())
            .spawn()
            .with_context(|| format!("Launch {}", rig.config.instance))?;
        let mut shown = rig.config.clone();
        if options.demo {
            let (width, height, fps) = shown.source.dimensions();
            shown.source = SourceConfig::Demo {
                width,
                height,
                fps,
                beat_hz: 0.8,
            };
        }
        println!(
            "Started {:<12} PID {:<7} {} [{}]",
            rig.config.instance,
            child.id(),
            summary(&shown),
            rig.path.display()
        );
        children.0.push((rig.config.instance.clone(), child));
    }
    println!("Press Ctrl+C to stop every rig.");
    while !children.0.is_empty() && !stop.load(Ordering::Relaxed) {
        let mut index = 0;
        while index < children.0.len() {
            if let Some(status) = children.0[index].1.try_wait()? {
                let (name, _) = children.0.remove(index);
                if !status.success() {
                    bail!("Rig {name} exited with {status}; stopping other rigs");
                }
                println!("Rig {name} finished");
            } else {
                index += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    children.shutdown();
    Ok(())
}
