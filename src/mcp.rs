//! A Model Context Protocol server on stdin and stdout, so AI agents and
//! other automation can drive Rekkefly: find cameras, read and edit rig
//! configs, run rigs headless, read live wing angles, look at what the
//! tracker sees, record, and track or cut videos.
//!
//! Newline-delimited JSON-RPC 2.0, as the MCP stdio transport specifies.
//! Only protocol messages go to stdout; logs go to stderr.
use crate::{
    analyze::{self, TrackOptions},
    avi::VideoFormat,
    clip::{self, ClipOptions},
    config::{Config, Method, Overrides, SourceConfig, camera_source},
    engine::{Engine, StartOptions, State},
    lock::ConfigLock,
    overlay,
    rigs::RigSet,
    source,
    tracker::Measurement,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Newest first. The newest is offered when a client asks for another.
pub const PROTOCOL_VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const INSTRUCTIONS: &str = "Rekkefly tracks the wing angles of tethered insects \
(the new Kinefly). Typical session: list_cameras and list_configs; start_rig with a \
config (or a camera device, a video or demo); snapshot to see the frame with the \
tracking overlay and check the markers; update_config to move them (image pixels; \
sector angles are image degrees clockwise from +x); rig_status for live angles and \
measure for statistics over a trial; record; stop_rig. Offline: track_video summarises a whole video, snapshot with \
`video` shows any frame. Wing angles are 0 perpendicular to the body axis and \
positive toward the head; left minus right (L−R) is the usual steering signal.";

struct Rig {
    engine: Engine,
    path: PathBuf,
    overrides: Overrides,
    /// The running configuration: the saved file plus this run's overrides.
    config: Config,
    started: Instant,
    _lock: ConfigLock,
}
pub struct Server {
    /// Where config names without a folder are looked up.
    directory: PathBuf,
    rigs: BTreeMap<String, Rig>,
}
/// What a tool returns: JSON for the caller, optionally an image to look at.
struct Output {
    value: Value,
    image: Option<Vec<u8>>,
}
impl From<Value> for Output {
    fn from(value: Value) -> Self {
        Self { value, image: None }
    }
}

/// Serve until stdin closes, then stop every rig cleanly.
pub fn serve(input: impl BufRead, mut output: impl Write, directory: PathBuf) -> Result<()> {
    let mut server = Server::new(directory);
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle_line(&line) {
            serde_json::to_writer(&mut output, &reply)?;
            output.write_all(b"\n")?;
            output.flush()?;
        }
    }
    server.stop_all();
    Ok(())
}
fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message.into()}})
}
impl Server {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            rigs: BTreeMap::new(),
        }
    }
    /// One JSON-RPC message or batch in, the reply out (none for notifications).
    pub fn handle_line(&mut self, line: &str) -> Option<Value> {
        match serde_json::from_str::<Value>(line) {
            Ok(Value::Array(batch)) if !batch.is_empty() => {
                let replies: Vec<Value> =
                    batch.into_iter().filter_map(|m| self.handle(m)).collect();
                (!replies.is_empty()).then_some(Value::Array(replies))
            }
            Ok(message) => self.handle(message),
            Err(e) => Some(error(Value::Null, -32700, format!("Parse error: {e}"))),
        }
    }
    fn handle(&mut self, message: Value) -> Option<Value> {
        let Value::Object(message) = message else {
            return Some(error(Value::Null, -32600, "Invalid request"));
        };
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response from the client; this server sends no requests.
            return None;
        };
        let params = message.get("params").cloned().unwrap_or(json!({}));
        let id = id?; // Notifications (initialized, cancelled) need no reply.
        let result = match method {
            "initialize" => Ok(self.initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": tools()})),
            "tools/call" => self.call(&params),
            "resources/list" => Ok(json!({"resources": []})),
            "prompts/list" => Ok(json!({"prompts": []})),
            _ => return Some(error(id, -32601, format!("Method not found: {method}"))),
        };
        Some(match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(e) => error(id, -32602, format!("{e:#}")),
        })
    }
    fn initialize(&self, params: &Value) -> Value {
        let requested = params.get("protocolVersion").and_then(Value::as_str);
        let version = requested
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        json!({
            "protocolVersion": version,
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {
                "name": "rekkefly",
                "title": "Rekkefly",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": INSTRUCTIONS,
        })
    }
    /// `tools/call`: unknown tools and arguments are protocol errors; a tool
    /// that fails reports `isError` so the agent can read why and retry.
    fn call(&mut self, params: &Value) -> Result<Value> {
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .context("tools/call needs a tool name")?;
        let tool = tools()
            .into_iter()
            .find(|t| t["name"] == name)
            .ok_or_else(|| anyhow!("Unknown tool: {name}"))?;
        let args = match params.get("arguments") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(args)) => args.clone(),
            Some(_) => bail!("arguments must be an object"),
        };
        let known = tool["inputSchema"]["properties"]
            .as_object()
            .expect("every tool lists its properties");
        if let Some(unknown) = args.keys().find(|k| !known.contains_key(*k)) {
            bail!(
                "{name} has no argument {unknown:?}; it takes: {}",
                known.keys().cloned().collect::<Vec<_>>().join(", ")
            );
        }
        let result = match name {
            "list_cameras" => self.list_cameras(),
            "list_configs" => self.list_configs(),
            "get_config" => self.get_config(&args),
            "update_config" => self.update_config(&args),
            "start_rig" => self.start_rig(&args),
            "launch_rigs" => self.launch_rigs(&args),
            "rig_status" => self.rig_status(&args),
            "measure" => self.measure(&args),
            "stop_rig" => self.stop_rig(&args),
            "record" => self.record(&args),
            "save_background" => self.save_background(&args),
            "snapshot" => self.snapshot(&args),
            "track_video" => self.track_video(&args),
            "clip_video" => self.clip_video(&args),
            "benchmark" => self.benchmark(&args),
            _ => unreachable!("every listed tool is handled"),
        };
        Ok(match result {
            Ok(Output { value, image }) => {
                let mut content = Vec::new();
                if let Some(jpeg) = image {
                    content.push(json!({
                        "type": "image",
                        "mimeType": "image/jpeg",
                        "data": base64(&jpeg),
                    }));
                }
                content.push(json!({
                    "type": "text",
                    "text": serde_json::to_string_pretty(&value)?,
                }));
                let mut result = json!({"content": content, "isError": false});
                if value.is_object() {
                    result["structuredContent"] = value;
                }
                result
            }
            Err(e) => json!({
                "content": [{"type": "text", "text": format!("{e:#}")}],
                "isError": true,
            }),
        })
    }
    /// A config name in the config folder, or a path.
    fn config_path(&self, name: &str) -> PathBuf {
        let path = Path::new(name);
        if path.is_absolute() || name.contains(['/', '\\']) {
            return path.to_path_buf();
        }
        let file = if name.ends_with(".toml") {
            name.to_owned()
        } else {
            format!("{name}.toml")
        };
        self.directory.join(file)
    }
    fn rig(&self, args: &Map<String, Value>) -> Result<(&String, &Rig)> {
        let name = text(args, "rig")?.context("rig is required (see rig_status)")?;
        self.rigs.get_key_value(&name).ok_or_else(|| {
            anyhow!(
                "No rig {name:?} is running here. Running: {}",
                self.running()
            )
        })
    }
    fn running(&self) -> String {
        if self.rigs.is_empty() {
            "none".into()
        } else {
            self.rigs.keys().cloned().collect::<Vec<_>>().join(", ")
        }
    }
    fn list_cameras(&self) -> Result<Output> {
        let cameras = source::list_cameras()?;
        Ok(json!({
            "cameras": cameras,
            "hint": "Use the number before the colon (or the device path or part of the name) as `device`.",
        })
        .into())
    }
    fn list_configs(&self) -> Result<Output> {
        let mut configs = Vec::new();
        let mut manifests = Vec::new();
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&self.directory)
            .with_context(|| format!("Read {}", self.directory.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "toml"))
            .collect();
        entries.sort();
        for path in entries {
            if let Ok(set) = RigSet::load(&path) {
                manifests.push(json!({
                    "path": path,
                    "rigs": set.rigs.iter().map(|r| json!({
                        "label": r.label, "config": r.config, "device": r.device,
                        "video": r.video, "zmq_port": r.zmq_port, "udp_port": r.udp_port,
                        "enabled": r.enabled, "record": r.record,
                    })).collect::<Vec<_>>(),
                }));
                continue;
            }
            let name = path.file_stem().unwrap_or_default().to_string_lossy();
            let running = self
                .rigs
                .iter()
                .find(|(_, r)| r.path == path)
                .map(|(k, _)| k);
            let status = if let Some(rig) = running {
                format!("running here as {rig}")
            } else if matches!(ConfigLock::try_acquire(&path), Ok(None)) {
                "open in another Rekkefly".into()
            } else {
                "available".into()
            };
            configs.push(match Config::load(&path) {
                Ok(c) => json!({
                    "name": name, "path": path, "instance": c.instance,
                    "source": source_summary(&c.source),
                    "zmq": c.output.zmq_enabled.then_some(&c.output.zmq_bind),
                    "udp": c.output.enabled.then_some(&c.output.target),
                    "status": status,
                }),
                Err(e) => json!({"name": name, "path": path, "error": format!("{e:#}")}),
            });
        }
        Ok(json!({"directory": self.directory, "configs": configs, "manifests": manifests}).into())
    }
    fn get_config(&self, args: &Map<String, Value>) -> Result<Output> {
        let path = self.config_path(&text(args, "config")?.context("config is required")?);
        let config = Config::load(&path)?;
        let toml = std::fs::read_to_string(&path)?;
        Ok(json!({"path": path, "config": config, "toml": toml}).into())
    }
    fn update_config(&mut self, args: &Map<String, Value>) -> Result<Output> {
        let path = self.config_path(&text(args, "config")?.context("config is required")?);
        let changes = args
            .get("changes")
            .and_then(Value::as_object)
            .context("changes must be an object of the fields to change")?;
        let create = flag(args, "create")?.unwrap_or(false);
        let old = if path.is_file() {
            Config::load(&path)?
        } else {
            ensure!(
                create,
                "{} does not exist; pass create: true to start from the defaults",
                path.display()
            );
            Config {
                instance: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                ..Config::default()
            }
        };
        let mut value = serde_json::to_value(&old)?;
        // A new source kind replaces the old source instead of merging with it.
        if let (Some(new), Some(current)) = (
            changes.get("source").and_then(Value::as_object),
            value.get_mut("source"),
        ) && new
            .get("kind")
            .is_some_and(|k| Some(k) != current.get("kind"))
        {
            let mut source = Map::new();
            for key in ["width", "height", "fps"] {
                if let Some(v) = current.get(key) {
                    source.insert(key.into(), v.clone());
                }
            }
            *current = Value::Object(source);
        }
        merge(&mut value, &Value::Object(changes.clone()));
        let new: Config = serde_json::from_value(value).context("The changed config")?;
        new.validate()?;
        new.save(&path)?;
        let mut note = format!("Saved {}", path.display());
        if let Some(rig) = self.rigs.values_mut().find(|r| r.path == path) {
            let running = rig.overrides.apply(&new);
            if rig.engine.state().example.is_some() {
                note += "; this rig tracks a video with its own markers, so restart it to use the config's";
            } else {
                match rig.engine.update(&running) {
                    Ok(()) => {
                        rig.config = running;
                        note += "; applied to the running rig";
                    }
                    Err(e) => note += &format!("; restart the rig to apply: {e:#}"),
                }
            }
        }
        Ok(json!({"path": path, "config": new, "note": note}).into())
    }
    fn start_rig(&mut self, args: &Map<String, Value>) -> Result<Output> {
        let (path, lock) = match text(args, "config")? {
            Some(name) => {
                let path = self.config_path(&name);
                ensure!(
                    path.is_file(),
                    "{} does not exist (list_configs shows the configs; update_config with create: true makes one)",
                    path.display()
                );
                let lock = ConfigLock::acquire(&path)?;
                (path, lock)
            }
            None => crate::installation::free_config(&self.directory.join("camera-a.toml"))?,
        };
        let saved = if path.is_file() {
            Config::load(&path)?
        } else {
            Config::default()
        };
        let mut overrides = Overrides {
            label: text(args, "label")?,
            zmq_port: port(args, "zmq_port")?,
            udp_port: port(args, "udp_port")?,
            udp_target: text(args, "udp_target")?,
            record: flag(args, "record")?.unwrap_or(false),
            ..Default::default()
        };
        let video = text(args, "video")?;
        let (width, height, fps) = saved.source.dimensions();
        match (
            text(args, "device")?,
            &video,
            flag(args, "demo")?.unwrap_or(false),
        ) {
            (Some(device), None, false) => overrides.source = Some(camera_source(&saved, &device)),
            (None, Some(video), false) => {
                let video = Path::new(video)
                    .canonicalize()
                    .with_context(|| format!("Video {video}"))?;
                overrides.source = Some(SourceConfig::Video {
                    path: video.to_string_lossy().into_owned(),
                    width,
                    height,
                    fps,
                    repeat: flag(args, "loop")?.unwrap_or(false),
                    realtime: !flag(args, "fast")?.unwrap_or(false),
                });
            }
            (None, None, true) => {
                overrides.source = Some(SourceConfig::Demo {
                    width,
                    height,
                    fps,
                    beat_hz: 0.8_f32.min(fps as f32 / 4.0),
                })
            }
            (None, None, false) => {}
            _ => bail!("Give at most one of device, video and demo"),
        }
        let limit = count(args, "frames")?;
        let wait = number(args, "wait_seconds")?
            .unwrap_or(5.0)
            .clamp(0.0, 60.0);
        let label = self.start(path, saved, overrides, lock, limit, video.is_some())?;
        self.settle(&label, wait);
        Ok(self.status(&label).into())
    }
    fn start(
        &mut self,
        path: PathBuf,
        saved: Config,
        overrides: Overrides,
        lock: ConfigLock,
        limit: Option<u64>,
        video_markers: bool,
    ) -> Result<String> {
        let config = overrides.apply(&saved);
        config.validate()?;
        let label = config.instance.clone();
        ensure!(
            !self.rigs.contains_key(&label),
            "A rig labelled {label:?} is already running; give this one another label"
        );
        let options = StartOptions {
            limit,
            video_markers,
            ..Default::default()
        };
        let engine = Engine::start_with(config.clone(), path.clone(), options)?;
        self.rigs.insert(
            label.clone(),
            Rig {
                engine,
                path,
                overrides,
                config,
                started: Instant::now(),
                _lock: lock,
            },
        );
        Ok(label)
    }
    /// Wait until a rig shows its first frame or stops, at most `seconds`.
    fn settle(&self, label: &str, seconds: f64) {
        let deadline = Instant::now() + Duration::from_secs_f64(seconds);
        while let Some(rig) = self.rigs.get(label) {
            let state = rig.engine.state();
            if state.latest.is_some() || state.finished || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    fn launch_rigs(&mut self, args: &Map<String, Value>) -> Result<Output> {
        let manifest = self.config_path(&text(args, "manifest")?.unwrap_or("rigs".into()));
        let demo = flag(args, "demo")?.unwrap_or(false);
        let record = flag(args, "record")?.unwrap_or(false);
        let limit = count(args, "frames")?;
        let rigs = RigSet::load(&manifest)?.resolve(&manifest, demo)?;
        ensure!(
            !rigs.is_empty(),
            "{} has no enabled rigs",
            manifest.display()
        );
        let mut locks = Vec::new();
        for rig in &rigs {
            locks.push(ConfigLock::acquire(&rig.path)?);
        }
        let mut started = Vec::new();
        for (rig, lock) in rigs.into_iter().zip(locks) {
            let saved = Config::load(&rig.path)?;
            let mut overrides = rig.overrides.clone();
            overrides.record |= record || rig.record;
            if demo {
                let (width, height, fps) = saved.source.dimensions();
                overrides.source = Some(SourceConfig::Demo {
                    width,
                    height,
                    fps,
                    beat_hz: 0.8_f32.min(fps as f32 / 4.0),
                });
            }
            let video = matches!(overrides.source, Some(SourceConfig::Video { .. }));
            match self.start(rig.path, saved, overrides, lock, limit, video) {
                Ok(label) => started.push(label),
                Err(e) => {
                    for label in started {
                        if let Some(rig) = self.rigs.remove(&label) {
                            rig.engine.wait();
                        }
                    }
                    return Err(e);
                }
            }
        }
        for label in &started {
            self.settle(label, 5.0);
        }
        let rigs: Vec<Value> = started.iter().map(|l| self.status(l)).collect();
        Ok(json!({"manifest": manifest, "rigs": rigs}).into())
    }
    fn status(&self, label: &str) -> Value {
        let Some(rig) = self.rigs.get(label) else {
            return Value::Null;
        };
        let state = rig.engine.state();
        let m = state.latest.as_ref().map(|s| &s.measurement);
        json!({
            "rig": label,
            "config": rig.path,
            "source": state.source,
            "running": !rig.engine.is_finished(),
            "message": state.message,
            "error": state.error,
            "frames": state.frames,
            "seconds": (rig.started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            "fps": m.map(|m| (m.capture_fps * 10.0).round() / 10.0),
            "processing_us": m.map(|m| m.processing_us.round()),
            "angles_deg": m.map(angles),
            "own_markers": state.example.is_some(),
            "recording": {
                "active": state.recording.active,
                "frames_written": state.recording.written,
                "megabytes": (state.recording.bytes as f64 / 1e4).round() / 100.0,
                "directory": state.recording.directory,
                "error": state.recording.error,
            },
            "outputs": {
                "zmq": rig.config.output.zmq_enabled.then_some(&rig.config.output.zmq_bind),
                "udp_target": rig.config.output.enabled.then_some(&rig.config.output.target),
                "udp_dropped": state.udp_dropped,
                "zmq_dropped": state.zmq_dropped,
            },
        })
    }
    fn rig_status(&self, args: &Map<String, Value>) -> Result<Output> {
        if args.contains_key("rig") {
            let (label, _) = self.rig(args)?;
            return Ok(self.status(label).into());
        }
        let rigs: Vec<Value> = self.rigs.keys().map(|l| self.status(l)).collect();
        Ok(json!({"rigs": rigs}).into())
    }
    /// Every frame of a running rig for `seconds`, summarised.
    fn measure(&self, args: &Map<String, Value>) -> Result<Output> {
        let (label, rig) = self.rig(args)?;
        let seconds = number(args, "seconds")?.unwrap_or(5.0);
        ensure!(
            seconds > 0.0 && seconds <= 600.0,
            "seconds must be more than 0 and at most 600"
        );
        let receiver = rig.engine.subscribe(65_536)?;
        let path = text(args, "csv")?;
        let mut csv = path
            .as_ref()
            .map(|p| {
                std::fs::File::create(p)
                    .map(std::io::BufWriter::new)
                    .with_context(|| format!("Create {p}"))
            })
            .transpose()?;
        if let Some(out) = &mut csv {
            writeln!(
                out,
                "sequence,timestamp_ns,left_deg,right_deg,left_minus_right_deg,head_deg,abdomen_deg,processing_us"
            )?;
        }
        let deadline = Instant::now() + Duration::from_secs_f64(seconds);
        let mut stats: [analyze::Accumulator; 5] = Default::default();
        let (mut frames, mut skipped, mut previous) = (0u64, 0u64, None::<u64>);
        let (mut first, mut last) = (None, 0u64);
        let mut processing = 0.0_f64;
        let deg = |a: Option<f32>| {
            a.map(|a| format!("{:.3}", a.to_degrees()))
                .unwrap_or_default()
        };
        while let Some(wait) = deadline.checked_duration_since(Instant::now()) {
            let Ok(m) = receiver.recv_timeout(wait) else {
                if rig.engine.is_finished() {
                    break;
                }
                continue;
            };
            if let Some(p) = previous {
                skipped += m.sequence.saturating_sub(p + 1);
            }
            previous = Some(m.sequence);
            first.get_or_insert(m.timestamp_ns);
            last = m.timestamp_ns;
            frames += 1;
            processing += m.processing_us as f64;
            for (s, v) in stats.iter_mut().zip([
                m.left.angle_rad,
                m.right.angle_rad,
                m.left_minus_right_rad,
                m.head.angle_rad,
                m.abdomen.angle_rad,
            ]) {
                s.add(v);
            }
            if let Some(out) = &mut csv {
                writeln!(
                    out,
                    "{},{},{},{},{},{},{},{:.1}",
                    m.sequence,
                    m.timestamp_ns,
                    deg(m.left.angle_rad),
                    deg(m.right.angle_rad),
                    deg(m.left_minus_right_rad),
                    deg(m.head.angle_rad),
                    deg(m.abdomen.angle_rad),
                    m.processing_us
                )?;
            }
        }
        if let Some(out) = &mut csv {
            out.flush()?;
        }
        ensure!(
            frames > 0,
            "No frames arrived in {seconds} s; check rig_status"
        );
        let span = last.saturating_sub(first.unwrap_or(last)) as f64 / 1e9;
        let [left, right, left_minus_right, head, abdomen] = stats.map(|s| s.stats());
        Ok(json!({
            "rig": label,
            "frames": frames,
            "skipped_frames": skipped,
            "seconds": (span * 1000.0).round() / 1000.0,
            "fps": if span > 0.0 { ((frames - 1) as f64 / span * 10.0).round() / 10.0 } else { 0.0 },
            "processing_us": (processing / frames as f64).round(),
            "angles_deg": {
                "left": left, "right": right, "left_minus_right": left_minus_right,
                "head": head, "abdomen": abdomen,
            },
            "csv": path,
        })
        .into())
    }
    fn stop_rig(&mut self, args: &Map<String, Value>) -> Result<Output> {
        let labels: Vec<String> = if args.contains_key("rig") {
            vec![self.rig(args)?.0.clone()]
        } else {
            self.rigs.keys().cloned().collect()
        };
        let mut stopped = Vec::new();
        for label in labels {
            let Some(rig) = self.rigs.get(&label) else {
                continue;
            };
            rig.engine.stop();
            let deadline = Instant::now() + Duration::from_secs(20);
            while !rig.engine.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            let status = self.status(&label);
            if let Some(rig) = self.rigs.remove(&label) {
                rig.engine.wait();
            }
            stopped.push(status);
        }
        Ok(json!({"stopped": stopped}).into())
    }
    pub fn stop_all(&mut self) {
        for (_, rig) in std::mem::take(&mut self.rigs) {
            rig.engine.wait();
        }
    }
    fn record(&self, args: &Map<String, Value>) -> Result<Output> {
        let (label, rig) = self.rig(args)?;
        let enabled = flag(args, "enabled")?.context("enabled is required")?;
        rig.engine.record(enabled)?;
        std::thread::sleep(Duration::from_millis(150));
        Ok(self.status(label).into())
    }
    fn save_background(&self, args: &Map<String, Value>) -> Result<Output> {
        let (label, rig) = self.rig(args)?;
        rig.engine.save_background()?;
        let path = rig
            .config
            .resolve(&rig.path, &rig.config.tracking.background_path);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut message = String::new();
        while Instant::now() < deadline {
            message = rig.engine.state().message;
            if [
                "Background saved",
                "Save background",
                "Background capture is off",
            ]
            .iter()
            .any(|m| message.starts_with(m))
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        ensure!(message.starts_with("Background saved"), "{message}");
        Ok(json!({"rig": label, "background": path, "message": message}).into())
    }
    fn snapshot(&self, args: &Map<String, Value>) -> Result<Output> {
        let width = count(args, "width")?.unwrap_or(640).clamp(64, 4096) as u32;
        let (frame, measurement, config, origin) = if args.contains_key("rig") {
            let (label, rig) = self.rig(args)?;
            let state: State = rig.engine.state();
            let latest = state
                .latest
                .clone()
                .context("No frame yet; check rig_status")?;
            let config = state.example.clone().unwrap_or_else(|| rig.config.clone());
            (
                (*latest.frame).clone(),
                latest.measurement.clone(),
                config,
                json!({"rig": label, "frames": state.frames}),
            )
        } else {
            let video = PathBuf::from(
                text(args, "video")?.context("give rig (a running rig) or video (a file)")?,
            );
            let (config, path) = self.optional_config(args)?;
            let options = TrackOptions {
                video_markers: config.is_none(),
                ..Default::default()
            };
            let (config, own) = analyze::video_config(&video, config.as_ref(), &options)?;
            let index = count(args, "frame")?.unwrap_or(0);
            let (frame, measurement) = frame_of(&config, &path, index)?;
            (
                frame,
                measurement,
                config,
                json!({"video": video, "frame": index, "own_markers": own}),
            )
        };
        let image = overlay::render(&frame, &config, Some(&measurement))
            .fit_width(width)
            .to_jpeg(85)?;
        let mut value = origin;
        value["angles_deg"] = angles(&measurement);
        value["edges_deg"] = json!({
            "left": measurement.left.angles_rad.iter().map(|a| round(a.to_degrees())).collect::<Vec<_>>(),
            "right": measurement.right.angles_rad.iter().map(|a| round(a.to_degrees())).collect::<Vec<_>>(),
        });
        value["image"] = json!({
            "width": frame.width, "height": frame.height,
            "legend": "Magenta dot head, cyan dot tail; red sector left wing, green right wing; bold line = reported edge, thin = other edges found.",
        });
        Ok(Output {
            value,
            image: Some(image),
        })
    }
    /// A config given by name or path, and the path that resolves its files.
    fn optional_config(&self, args: &Map<String, Value>) -> Result<(Option<Config>, PathBuf)> {
        match text(args, "config")? {
            Some(name) => {
                let path = self.config_path(&name);
                Ok((Some(Config::load(&path)?), path))
            }
            None => Ok((None, self.directory.join("unused.toml"))),
        }
    }
    fn track_video(&self, args: &Map<String, Value>) -> Result<Output> {
        let video = PathBuf::from(text(args, "video")?.context("video is required")?);
        let (config, path) = self.optional_config(args)?;
        let method = match text(args, "method")?.as_deref() {
            None => None,
            Some("robust" | "wing_blur") => Some(Method::WingBlur),
            Some("classic" | "edge") => Some(Method::Edge),
            Some(other) => bail!("method is robust or classic, not {other:?}"),
        };
        let options = TrackOptions {
            frames: count(args, "frames")?,
            method,
            csv: text(args, "csv")?.map(PathBuf::from),
            jsonl: text(args, "jsonl")?.map(PathBuf::from),
            video_markers: flag(args, "video_markers")?.unwrap_or(config.is_none()),
        };
        let summary = analyze::track(&video, config.as_ref(), &path, &options)?;
        Ok(serde_json::to_value(summary)?.into())
    }
    fn clip_video(&self, args: &Map<String, Value>) -> Result<Output> {
        let input = PathBuf::from(text(args, "input")?.context("input is required")?);
        let output = PathBuf::from(text(args, "output")?.context("output is required")?);
        let format = match text(args, "format")?.as_deref() {
            None | Some("mjpeg") => VideoFormat::Mjpeg,
            Some("png") => VideoFormat::Png,
            Some("raw") => VideoFormat::Raw,
            Some(other) => bail!("format is mjpeg, png or raw, not {other:?}"),
        };
        let quality = count(args, "quality")?.unwrap_or(90);
        ensure!((1..=100).contains(&quality), "quality must be 1–100");
        let options = ClipOptions {
            start: number(args, "start")?.unwrap_or(0.0),
            duration: number(args, "duration")?,
            format,
            quality: quality as u8,
            every: count(args, "every")?.unwrap_or(1).min(u32::MAX as u64) as u32,
        };
        let s = clip::clip(&input, &output, &options)?;
        Ok(json!({
            "output": output, "frames": s.frames, "width": s.width, "height": s.height,
            "fps": s.fps, "megabytes": (s.bytes as f64 / 1e4).round() / 100.0,
        })
        .into())
    }
    fn benchmark(&self, args: &Map<String, Value>) -> Result<Output> {
        let (config, _) = self.optional_config(args)?;
        let frames = count(args, "frames")?.unwrap_or(2000).clamp(1, 1_000_000);
        let result = analyze::benchmark(&config.unwrap_or_default(), frames)?;
        Ok(serde_json::to_value(result)?.into())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop_all();
    }
}
/// Standard base64 with padding, for image content.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * k) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}
fn round(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}
fn angles(m: &Measurement) -> Value {
    let deg = |a: Option<f32>| a.map(|a| round(a.to_degrees()));
    json!({
        "left": deg(m.left.angle_rad),
        "right": deg(m.right.angle_rad),
        "left_minus_right": deg(m.left_minus_right_rad),
        "left_plus_right": deg(m.left_plus_right_rad),
        "head": deg(m.head.angle_rad),
        "abdomen": deg(m.abdomen.angle_rad),
    })
}
fn source_summary(source: &SourceConfig) -> String {
    let (w, h, fps) = source.dimensions();
    let what = match source {
        SourceConfig::Camera { device, .. } => format!("camera {device}"),
        SourceConfig::Video { path, .. } => format!("video {path}"),
        SourceConfig::Network { url, .. } => format!("stream {url}"),
        SourceConfig::Demo { .. } => "simulated fly".into(),
    };
    format!("{what}, {w}×{h} @ {fps} fps")
}
/// Track one frame of the config's video source.
fn frame_of(
    config: &Config,
    path: &Path,
    index: u64,
) -> Result<(crate::frame::GrayFrame, Measurement)> {
    let child: source::ChildControl = Default::default();
    let mut opened = source::open(config, path, child)?.source;
    let stop = std::sync::atomic::AtomicBool::new(false);
    let mut frame = None;
    for _ in 0..=index {
        frame = Some(
            opened
                .next_frame(&stop)?
                .with_context(|| format!("The video has fewer than {} frames", index + 1))?,
        );
    }
    let frame = frame.expect("at least one frame was read");
    let background = if config.tracking.subtract_background {
        let file = config.resolve(path, &config.tracking.background_path);
        Some(
            crate::frame::GrayFrame::load(&file)
                .with_context(|| format!("Background {}", file.display()))?,
        )
    } else {
        None
    };
    let (w, h, _) = config.source.dimensions();
    let measurement =
        crate::tracker::Tracker::new(config, w, h).process(&frame, background.as_ref(), 0)?;
    Ok((frame, measurement))
}
/// RFC 7386 JSON merge patch: objects merge, anything else replaces.
fn merge(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(key);
                } else {
                    merge(target.entry(key.clone()).or_insert(Value::Null), value);
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}
fn text(args: &Map<String, Value>, key: &str) -> Result<Option<String>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(other) => bail!("{key} must be a string, not {other}"),
    }
}
fn flag(args: &Map<String, Value>, key: &str) -> Result<Option<bool>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => bail!("{key} must be true or false, not {other}"),
    }
}
fn number(args: &Map<String, Value>, key: &str) -> Result<Option<f64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|v| v.is_finite())
            .map(Some)
            .with_context(|| format!("{key} must be a number")),
    }
}
fn count(args: &Map<String, Value>, key: &str) -> Result<Option<u64>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .map(Some)
            .with_context(|| format!("{key} must be a whole number ≥ 0")),
    }
}
fn port(args: &Map<String, Value>, key: &str) -> Result<Option<u16>> {
    count(args, key)?
        .map(|p| {
            u16::try_from(p)
                .ok()
                .filter(|p| *p > 0)
                .context(format!("{key} must be 1–65535"))
        })
        .transpose()
}

/// Every tool, with a JSON Schema for its arguments.
pub fn tools() -> Vec<Value> {
    let string = |d: &str| json!({"type": "string", "description": d});
    let boolean = |d: &str| json!({"type": "boolean", "description": d});
    let integer = |d: &str| json!({"type": "integer", "minimum": 0, "description": d});
    let number = |d: &str| json!({"type": "number", "minimum": 0, "description": d});
    let rig = string("A running rig's label, as rig_status lists it.");
    let config = string(
        "A config name in the config folder (camera-a, bogong-moth) or a path to a .toml file.",
    );
    let tool = |name: &str,
                title: &str,
                description: &str,
                properties: Value,
                required: &[&str],
                read_only: bool,
                destructive: bool| {
        json!({
            "name": name,
            "title": title,
            "description": description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            },
            "annotations": {
                "title": title,
                "readOnlyHint": read_only,
                "destructiveHint": destructive,
                "idempotentHint": read_only,
                "openWorldHint": false,
            },
        })
    };
    vec![
        tool(
            "list_cameras",
            "List cameras",
            "Cameras this computer can open, with the numbers to use as `device`.",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "list_configs",
            "List rig configs",
            "Every rig config (.toml) in the config folder with its camera or video, outputs and whether it is running, plus rig manifests (rigs.toml).",
            json!({}),
            &[],
            true,
            false,
        ),
        tool(
            "get_config",
            "Read a config",
            "A rig config as JSON (every field) and as its commented TOML file, which explains each setting.",
            json!({"config": config}),
            &["config"],
            true,
            false,
        ),
        tool(
            "update_config",
            "Change a config",
            "Change fields of a rig config and save it (comments in the file are kept). `changes` is a JSON merge patch on the get_config JSON, for example {\"tracking\": {\"left\": {\"spike_width_deg\": 6}}} or {\"head\": {\"x\": 320, \"y\": 170}}. Invalid values are rejected and nothing is written. A rig running this config picks up marker and detector changes at once; source and output changes need a restart.",
            json!({"config": config, "changes": {"type": "object", "description": "The fields to change, nested as in get_config."},
                   "create": boolean("Create the config from the defaults if it does not exist (default false).")}),
            &["config", "changes"],
            false,
            true,
        ),
        tool(
            "start_rig",
            "Start a rig",
            "Start tracking headless with a config and publish its outputs (Kinefly ZMQ, UDP). Give a camera `device`, a `video` file or `demo` to override the config's source for this run only; the other overrides apply to this run only too. Returns the rig's status once it shows its first frame (or fails).",
            json!({"config": string("A config name or path. Default: the next free rig config (camera-a, camera-b, …)."),
                   "device": string("Camera for this run: auto, a number from list_cameras, a device path or part of the name."),
                   "video": string("Track this video file for this run (AVI natively; other formats need FFmpeg). Uses the video's own markers if it has them."),
                   "demo": boolean("Track the simulated fly for this run."),
                   "label": string("Label (instance name) for this run; also the rig's id here."),
                   "zmq_port": integer("Publish Kinefly ZMQ on this TCP port for this run."),
                   "udp_port": integer("Send UDP measurements from this local port for this run."),
                   "udp_target": string("Send UDP measurements to this IP:port for this run."),
                   "record": boolean("Record video from the start."),
                   "fast": boolean("With a video: as fast as possible instead of at its frame rate."),
                   "loop": boolean("With a video: play it in a loop."),
                   "frames": integer("Stop after this many frames."),
                   "wait_seconds": number("How long to wait for the first frame (default 5).")}),
            &[],
            false,
            false,
        ),
        tool(
            "launch_rigs",
            "Start every rig in a manifest",
            "Start every enabled rig in a rig manifest (rigs.toml): each with its own label, camera or video and ports. All or none: if one cannot start, the others are stopped again.",
            json!({"manifest": string("Manifest name or path (default rigs)."),
                   "demo": boolean("Use the simulated fly for every rig (to test ports and labels)."),
                   "record": boolean("Record every rig."),
                   "frames": integer("Stop each rig after this many frames.")}),
            &[],
            false,
            false,
        ),
        tool(
            "rig_status",
            "Rig status and live angles",
            "Status of one running rig (or all): frames, frame rate, tracking time, the latest wing angles in degrees (left, right, L−R, head, abdomen), recording and outputs.",
            json!({"rig": rig.clone()}),
            &[],
            true,
            false,
        ),
        tool(
            "measure",
            "Measure a rig for a while",
            "Collect every frame of a running rig for some seconds and summarise each angle in degrees (frames measured, mean, spread, minimum, maximum), for example the mean L−R of a trial. skipped_frames counts gaps in the camera's frame numbers: exposures the camera dropped, or that could not be delivered. Optionally writes every frame to a CSV. Blocks for `seconds`.",
            json!({"rig": rig.clone(),
                   "seconds": number("How long to measure (default 5, at most 600)."),
                   "csv": string("Also write every frame here (degrees).")}),
            &["rig"],
            true,
            false,
        ),
        tool(
            "stop_rig",
            "Stop a rig",
            "Stop a rig (or every rig if none is named), finishing any recording, and return its final status.",
            json!({"rig": rig.clone()}),
            &[],
            false,
            false,
        ),
        tool(
            "record",
            "Start or stop recording",
            "Start or stop recording a running rig's camera to its recording folder (MJPEG AVI by default).",
            json!({"rig": rig.clone(), "enabled": boolean("true starts, false stops.")}),
            &["rig", "enabled"],
            false,
            false,
        ),
        tool(
            "save_background",
            "Save the background",
            "Save the rig's next frame as its background image, for background subtraction. Capture an empty scene first.",
            json!({"rig": rig.clone()}),
            &["rig"],
            false,
            true,
        ),
        tool(
            "snapshot",
            "Look at the tracking",
            "An image of a frame with the tracking drawn on it (body axis, sectors, detected edges) and its angles. Use `rig` for the latest live frame, or `video` (with `frame` and optionally `config`) for any frame of a file. Check this after moving markers.",
            json!({"rig": rig,
                   "video": string("A video file, instead of a running rig."),
                   "frame": integer("With video: the frame number (default 0)."),
                   "config": string("With video: the config whose markers to use (default: the video's own, or the defaults)."),
                   "width": integer("Image width in pixels (default 640).")}),
            &[],
            true,
            false,
        ),
        tool(
            "track_video",
            "Track a video file",
            "Track every frame of a video as fast as possible and summarise it: frames per second, tracking time, and mean, spread and range of each angle. Optionally writes a CSV (one row per frame) and the full measurements as JSON lines.",
            json!({"video": string("The video file."),
                   "config": string("Config whose markers and settings to use (default: the video's own markers, or the defaults)."),
                   "method": {"type": "string", "enum": ["robust", "classic"], "description": "Override the wing detector: robust (ignores legs and antennae) or classic (Kinefly's strongest edge)."},
                   "frames": integer("Stop after this many frames."),
                   "csv": string("Write per-frame angles here."),
                   "jsonl": string("Write full measurements here."),
                   "video_markers": boolean("Use the video's own markers (a .toml beside it) even when a config is given.")}),
            &["video"],
            false,
            false,
        ),
        tool(
            "clip_video",
            "Cut a video",
            "Cut part of a video into a new, usually much smaller AVI. Never overwrites.",
            json!({"input": string("The video to cut (AVI natively; other formats need FFmpeg)."),
                   "output": string("The new AVI file."),
                   "start": number("Seconds to skip."),
                   "duration": number("Seconds to keep (default: to the end)."),
                   "format": {"type": "string", "enum": ["mjpeg", "png", "raw"], "description": "mjpeg (small, default), png (lossless) or raw."},
                   "quality": integer("MJPEG quality 1–100 (default 90)."),
                   "every": integer("Keep every Nth frame.")}),
            &["input", "output"],
            false,
            false,
        ),
        tool(
            "benchmark",
            "Measure tracking speed",
            "Tracking time per frame for a config on a synthetic frame (no camera needed).",
            json!({"config": string("Config to measure (default: the defaults, 640×480)."),
                   "frames": integer("Frames to time (default 2000).")}),
            &[],
            true,
            false,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_matches_rfc_4648_vectors() {
        for (plain, encoded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), encoded);
        }
    }
    #[test]
    fn merge_patch_follows_rfc_7386() {
        let mut v = json!({"a": {"b": 1, "c": 2}, "d": [1, 2]});
        merge(
            &mut v,
            &json!({"a": {"b": 5, "c": null}, "d": [3], "e": "x"}),
        );
        assert_eq!(v, json!({"a": {"b": 5}, "d": [3], "e": "x"}));
    }
    #[test]
    fn every_tool_has_a_closed_schema_and_a_description() {
        for tool in tools() {
            assert!(tool["description"].as_str().unwrap().len() > 20, "{tool}");
            assert_eq!(tool["inputSchema"]["type"], "object");
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
            for required in tool["inputSchema"]["required"].as_array().unwrap() {
                assert!(
                    tool["inputSchema"]["properties"]
                        .get(required.as_str().unwrap())
                        .is_some()
                );
            }
        }
    }
}
