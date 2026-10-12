use crate::config::{Config, SourceConfig};
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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RigEntry {
    pub config: PathBuf,
    #[serde(default = "enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub record: bool,
}
fn enabled() -> bool {
    true
}
impl RigSet {
    pub fn load(path: &Path) -> Result<Self> {
        Ok(toml::from_str(
            &std::fs::read_to_string(path)
                .with_context(|| format!("Read rig manifest {}", path.display()))?,
        )?)
    }
    /// Complete preflight before opening cameras or starting any children.
    pub fn resolve(&self, manifest: &Path, demo: bool) -> Result<Vec<(PathBuf, Config, bool)>> {
        let mut paths = HashSet::new();
        let mut names = HashSet::new();
        let mut udp = HashSet::new();
        let mut zmq = HashSet::new();
        let mut cameras = HashSet::new();
        let mut backgrounds = HashSet::new();
        let mut logs = HashSet::new();
        let mut result = Vec::new();
        for rig in self.rigs.iter().filter(|r| r.enabled) {
            let path = manifest
                .parent()
                .unwrap_or(Path::new("."))
                .join(&rig.config)
                .canonicalize()
                .context("Resolve rig configuration")?;
            ensure!(
                paths.insert(path.clone()),
                "Each rig needs its own config: {}",
                path.display()
            );
            let config = Config::load(&path)?;
            ensure!(
                names.insert(config.instance.clone()),
                "Duplicate instance {}",
                config.instance
            );
            if config.output.enabled {
                let address: std::net::SocketAddr = config.output.bind.parse()?;
                ensure!(
                    udp.insert(address.port()),
                    "Rigs share UDP sender port {}",
                    address.port()
                );
            }
            if config.output.zmq_enabled {
                let endpoint: zeromq::Endpoint = config.output.zmq_bind.parse()?;
                if let zeromq::Endpoint::Tcp(_, port) = endpoint {
                    ensure!(zmq.insert(port), "Rigs share ZMQ port {port}");
                }
            }
            if !demo && let SourceConfig::Camera { device, .. } = &config.source {
                let key = if device == "auto" {
                    "0"
                } else {
                    device.as_str()
                };
                ensure!(
                    cameras.insert(key.to_owned()),
                    "Rigs share camera {device}; map each rig to a distinct device"
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
            result.push((path, config, rig.record));
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
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    let exe = std::env::current_exe()?;
    let mut children = Children(Vec::new());
    for (path, config, record) in rigs {
        let mut command = Command::new(&exe);
        command
            .arg("--config")
            .arg(&path)
            .arg("--supervised")
            .stdin(Stdio::piped());
        if options.headless {
            command.arg("--headless");
        }
        if options.record || record {
            command.arg("--record");
        }
        if options.demo {
            command.arg("--demo");
        }
        if let Some(frames) = options.frames {
            command.arg("--frames").arg(frames.to_string());
        }
        let child = command
            .spawn()
            .with_context(|| format!("Launch {}", config.instance))?;
        println!(
            "Started {} (PID {}) from {}",
            config.instance,
            child.id(),
            path.display()
        );
        children.0.push((config.instance, child));
    }
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
