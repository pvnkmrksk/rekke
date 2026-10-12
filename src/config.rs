pub use crate::avi::VideoFormat;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn angle_to(self, other: Self) -> f32 {
        (other.y - self.y).atan2(other.x - self.x)
    }
    pub fn distance(self, other: Self) -> f32 {
        (other.x - self.x).hypot(other.y - self.y)
    }
    pub fn at(self, radius: f32, angle: f32) -> Self {
        Self {
            x: self.x + radius * angle.cos(),
            y: self.y + radius * angle.sin(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SourceConfig {
    Demo {
        width: u32,
        height: u32,
        fps: u32,
        beat_hz: f32,
    },
    Camera {
        device: String,
        width: u32,
        height: u32,
        fps: u32,
    },
    /// AVI (Y800/GREY, 8/24/32-bit, MJPEG, PNG) plays natively; other
    /// containers and codecs use FFmpeg if it is installed.
    Video {
        path: String,
        width: u32,
        height: u32,
        fps: u32,
        #[serde(default)]
        repeat: bool,
        /// Play at the recorded frame rate. False processes as fast as possible.
        #[serde(default = "default_true")]
        realtime: bool,
    },
    Network {
        url: String,
        width: u32,
        height: u32,
        fps: u32,
    },
}
impl SourceConfig {
    pub fn dimensions(&self) -> (u32, u32, u32) {
        match *self {
            Self::Demo {
                width, height, fps, ..
            }
            | Self::Camera {
                width, height, fps, ..
            }
            | Self::Video {
                width, height, fps, ..
            }
            | Self::Network {
                width, height, fps, ..
            } => (width, height, fps),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Demo { .. } => "Simulated camera",
            Self::Camera { .. } => "USB camera",
            Self::Video { .. } => "Video replay",
            Self::Network { .. } => "Network stream",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Strongest gradient of the radially averaged profile (classic Kinefly).
    Edge,
    /// Robust wing edge: contrast clipping, removal of thin angular pulses
    /// (legs, antennae) and the median gradient across radial bands.
    #[serde(alias = "robust")]
    WingBlur,
    Centroid,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrimaryEdge {
    #[default]
    Strongest,
    TowardHead,
    TowardTail,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sector {
    pub enabled: bool,
    pub hinge: Point,
    /// Image coordinates: zero points right, positive angles run clockwise.
    pub start_deg: f32,
    pub span_deg: f32,
    pub inner_radius: f32,
    pub outer_radius: f32,
    pub method: Method,
    pub threshold: f32,
    pub max_edges: usize,
    #[serde(default)]
    pub primary_edge: PrimaryEdge,
    #[serde(default = "default_contrast_cap")]
    pub contrast_cap: f32,
    /// Remove angular bright pulses narrower than this before scoring WingBlur edges.
    #[serde(default = "default_spike_width")]
    pub spike_width_deg: f32,
}
fn default_true() -> bool {
    true
}
fn default_spike_width() -> f32 {
    5.0
}
fn default_contrast_cap() -> f32 {
    0.12
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackingConfig {
    /// False couples geometry edits to the body frame and mirrors both wings.
    #[serde(default)]
    pub asymmetric: bool,
    pub angle_bins: usize,
    pub radial_samples: usize,
    /// Used without background subtraction; true for dark insect on light image.
    pub dark_on_light: bool,
    pub subtract_background: bool,
    pub background_path: String,
    pub left: Sector,
    pub right: Sector,
    pub head: Sector,
    pub abdomen: Sector,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    pub enabled: bool,
    /// A unique sender port for this instance; conflicts are errors.
    pub bind: String,
    pub target: String,
    /// Empty disables recording. Paths resolve relative to the config file.
    pub jsonl_path: String,
    #[serde(default)]
    pub zmq_enabled: bool,
    #[serde(default = "default_zmq_bind")]
    pub zmq_bind: String,
}
fn default_zmq_bind() -> String {
    "tcp://*:9871".into()
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RecordingConfig {
    pub enabled: bool,
    pub directory: String,
    pub queue_frames: usize,
    pub segment_mb: u32,
    /// "mjpeg" (small, default), "png" (lossless) or "raw" (lossless, largest).
    pub format: VideoFormat,
    /// MJPEG quality 1–100. 90 is visually lossless for wing tracking.
    pub quality: u8,
}
impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: "recordings".into(),
            queue_frames: 32,
            segment_mb: 1024,
            format: VideoFormat::Mjpeg,
            quality: 90,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub instance: String,
    pub source: SourceConfig,
    pub head: Point,
    pub tail: Point,
    pub tracking: TrackingConfig,
    pub output: OutputConfig,
    pub preview_fps: u32,
    #[serde(default)]
    pub recording: RecordingConfig,
    #[serde(default = "default_autosave")]
    pub autosave: bool,
}
fn default_autosave() -> bool {
    true
}
impl Default for Config {
    fn default() -> Self {
        let sector = |x, y, start, span, inner, outer, method| Sector {
            enabled: true,
            hinge: Point { x, y },
            start_deg: start,
            span_deg: span,
            inner_radius: inner,
            outer_radius: outer,
            method,
            threshold: 0.0,
            max_edges: 2,
            primary_edge: PrimaryEdge::Strongest,
            contrast_cap: default_contrast_cap(),
            spike_width_deg: default_spike_width(),
        };
        Self {
            instance: "rig-a".into(),
            source: SourceConfig::Camera {
                device: "auto".into(),
                width: 640,
                height: 480,
                fps: 30,
            },
            head: Point { x: 320.0, y: 175.0 },
            tail: Point { x: 320.0, y: 330.0 },
            tracking: TrackingConfig {
                asymmetric: false,
                angle_bins: 240,
                radial_samples: 72,
                dark_on_light: true,
                subtract_background: false,
                background_path: "rig-a.background.png".into(),
                // −45° (toward the tail) to +85° (toward the head) around each
                // wing, covering a typical leading edge of +30° to +80°.
                left: sector(296.0, 240.0, 135.0, 130.0, 55.0, 185.0, Method::WingBlur),
                right: sector(344.0, 240.0, -85.0, 130.0, 55.0, 185.0, Method::WingBlur),
                head: Sector {
                    enabled: false,
                    ..sector(320.0, 205.0, -125.0, 70.0, 12.0, 62.0, Method::Centroid)
                },
                abdomen: Sector {
                    enabled: false,
                    ..sector(320.0, 265.0, 55.0, 70.0, 15.0, 88.0, Method::Centroid)
                },
            },
            output: OutputConfig {
                enabled: true,
                bind: "127.0.0.1:5501".into(),
                target: "127.0.0.1:5601".into(),
                jsonl_path: String::new(),
                zmq_enabled: true,
                zmq_bind: default_zmq_bind(),
            },
            preview_fps: 40,
            recording: RecordingConfig::default(),
            autosave: true,
        }
    }
}
impl Config {
    pub fn demo() -> Self {
        Self {
            source: SourceConfig::Demo {
                width: 640,
                height: 480,
                fps: 60,
                beat_hz: 0.8,
            },
            output: OutputConfig {
                zmq_enabled: false,
                ..Self::default().output
            },
            ..Self::default()
        }
    }
    pub fn load(path: &Path) -> Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("Read {}", path.display()))?;
        let config: Self = toml::from_str(&contents).context("Parse configuration TOML")?;
        config.validate()?;
        Ok(config)
    }
    /// Validate and atomically write. Comments and layout of an existing
    /// file are kept; only changed values are replaced.
    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        let fresh = toml::to_string_pretty(self)?;
        let contents = fs::read_to_string(path)
            .ok()
            .and_then(|old| old.parse::<toml_edit::DocumentMut>().ok())
            .and_then(|mut document| {
                let new = fresh.parse::<toml_edit::DocumentMut>().ok()?;
                merge_table(document.as_table_mut(), new.as_table());
                Some(document.to_string())
            })
            .unwrap_or(fresh);
        atomic_write(path, contents.as_bytes()).with_context(|| format!("Save {}", path.display()))
    }
    pub fn resolve(&self, config_path: &Path, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            config_path.parent().unwrap_or(Path::new(".")).join(p)
        }
    }
    pub fn sectors(&self) -> [&Sector; 4] {
        [
            &self.tracking.left,
            &self.tracking.right,
            &self.tracking.head,
            &self.tracking.abdomen,
        ]
    }
    pub fn sector_mut(&mut self, index: usize) -> &mut Sector {
        match index {
            0 => &mut self.tracking.left,
            1 => &mut self.tracking.right,
            2 => &mut self.tracking.head,
            _ => &mut self.tracking.abdomen,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.instance.trim().is_empty() && self.instance.len() <= 128,
            "instance must contain 1–128 characters"
        );
        let (w, h, fps) = self.source.dimensions();
        ensure!(
            (32..=8192).contains(&w)
                && (32..=8192).contains(&h)
                && (w as u64 * h as u64) <= 33_554_432,
            "Invalid source dimensions (max 32 megapixels)"
        );
        ensure!((1..=10000).contains(&fps), "Source fps must be 1–10000");
        ensure!(
            (1..=120).contains(&self.preview_fps),
            "preview_fps must be 1–120"
        );
        if let SourceConfig::Demo { beat_hz, .. } = self.source {
            ensure!(
                beat_hz.is_finite() && beat_hz > 0.0 && beat_hz < fps as f32 / 2.0,
                "Demo beat_hz must be positive and below fps/2"
            );
        }
        match &self.source {
            SourceConfig::Camera { device, .. } => {
                ensure!(!device.is_empty(), "Camera device is empty")
            }
            SourceConfig::Video { path, .. } => ensure!(!path.is_empty(), "Video path is empty"),
            SourceConfig::Network { url, .. } => ensure!(!url.is_empty(), "Network URL is empty"),
            _ => {}
        }
        for p in [self.head, self.tail] {
            validate_point(p, w, h)?;
        }
        ensure!(
            self.head.distance(self.tail) > 1.0,
            "Head and tail anchors must be distinct"
        );
        ensure!(
            self.tracking.left.hinge.distance(self.tracking.right.hinge) > 1.0,
            "Wing hinges must be distinct"
        );
        let body = Point {
            x: self.head.x - self.tail.x,
            y: self.head.y - self.tail.y,
        };
        let wings = Point {
            x: self.tracking.right.hinge.x - self.tracking.left.hinge.x,
            y: self.tracking.right.hinge.y - self.tracking.left.hinge.y,
        };
        ensure!(
            (body.x * wings.y - body.y * wings.x).abs() > 1.0,
            "Body axis and wing-hinge axis must not be collinear"
        );
        ensure!(
            (16..=2048).contains(&self.tracking.angle_bins),
            "angle_bins must be 16–2048"
        );
        ensure!(
            (4..=1024).contains(&self.tracking.radial_samples),
            "radial_samples must be 4–1024"
        );
        ensure!(
            self.tracking.angle_bins * self.tracking.radial_samples <= 524288,
            "Polar grid is too large (max 524288 samples per sector)"
        );
        ensure!(
            !self.tracking.background_path.is_empty(),
            "background_path is empty"
        );
        for (name, s) in ["left", "right", "head", "abdomen"]
            .into_iter()
            .zip(self.sectors())
        {
            validate_point(s.hinge, w, h).with_context(|| name.to_string())?;
            ensure!(
                s.start_deg.is_finite() && s.start_deg.abs() <= 3600.0,
                "{name}: invalid start angle"
            );
            ensure!(
                s.span_deg.is_finite() && (5.0..=355.0).contains(&s.span_deg),
                "{name}: span must be 5–355 degrees"
            );
            ensure!(
                s.inner_radius.is_finite()
                    && s.outer_radius.is_finite()
                    && s.inner_radius >= 0.0
                    && s.outer_radius > s.inner_radius + 1.0
                    && s.outer_radius <= ((w as f32).hypot(h as f32)),
                "{name}: invalid radii"
            );
            ensure!(
                s.threshold.is_finite() && (0.0..=1.0).contains(&s.threshold),
                "{name}: threshold must be 0–1"
            );
            ensure!(
                s.contrast_cap.is_finite() && s.contrast_cap > 0.0 && s.contrast_cap <= 1.0,
                "{name}: contrast_cap must be >0 and ≤1"
            );
            ensure!(
                (1..=8).contains(&s.max_edges),
                "{name}: max_edges must be 1–8"
            );
            ensure!(
                s.spike_width_deg.is_finite() && (0.0..=45.0).contains(&s.spike_width_deg),
                "{name}: spike_width_deg must be 0–45"
            );
        }
        if self.output.enabled {
            let bind: std::net::SocketAddr = self
                .output
                .bind
                .parse()
                .context("output.bind must be an IP:port")?;
            let target: std::net::SocketAddr = self
                .output
                .target
                .parse()
                .context("output.target must be an IP:port")?;
            if bind.port() == 0 || target.port() == 0 {
                bail!("Output ports must be nonzero");
            }
            ensure!(
                bind.is_ipv4() == target.is_ipv4(),
                "bind and target must use the same IP version"
            );
        }
        if self.output.zmq_enabled {
            let endpoint: zeromq::Endpoint = self
                .output
                .zmq_bind
                .parse()
                .context("Invalid output.zmq_bind")?;
            ensure!(
                matches!(endpoint, zeromq::Endpoint::Tcp(_, port) if port > 0),
                "ZMQ bind must be tcp://host:nonzero-port"
            );
        }
        ensure!(
            !self.recording.directory.trim().is_empty(),
            "recording.directory is empty"
        );
        ensure!(
            (1..=256).contains(&self.recording.queue_frames)
                && self.recording.queue_frames as u64 * w as u64 * h as u64 <= 268_435_456,
            "Recording queue must be 1–256 frames and at most 256 MiB"
        );
        ensure!(
            (16..=1900).contains(&self.recording.segment_mb),
            "Recording segment_mb must be 16–1900"
        );
        ensure!(
            (1..=100).contains(&self.recording.quality),
            "Recording quality must be 1–100"
        );
        Ok(())
    }
}
/// Settings supplied at launch (command line or rig manifest) that apply to
/// this run only. The GUI saves the config without them, so a temporary
/// video, camera or port never overwrites the saved setup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overrides {
    /// Window title, instance name and recording prefix.
    pub label: Option<String>,
    pub source: Option<SourceConfig>,
    pub zmq_port: Option<u16>,
    pub udp_port: Option<u16>,
    pub udp_target: Option<String>,
    pub record: bool,
    /// Why the source was replaced, shown in the GUI.
    pub note: Option<String>,
}
fn replace_port(address: &str, port: u16) -> String {
    match address.rsplit_once(':') {
        Some((host, _)) => format!("{host}:{port}"),
        None => format!("{address}:{port}"),
    }
}
impl Overrides {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
    pub fn apply(&self, config: &Config) -> Config {
        let mut c = config.clone();
        if let Some(label) = &self.label {
            c.instance = label.clone();
        }
        if let Some(source) = &self.source {
            c.source = source.clone();
        }
        if let Some(port) = self.zmq_port {
            c.output.zmq_enabled = true;
            c.output.zmq_bind = replace_port(&c.output.zmq_bind, port);
        }
        if let Some(port) = self.udp_port {
            c.output.enabled = true;
            c.output.bind = replace_port(&c.output.bind, port);
        }
        if let Some(target) = &self.udp_target {
            c.output.enabled = true;
            c.output.target = target.clone();
        }
        if self.record {
            c.recording.enabled = true;
        }
        c
    }
    /// The config to save after GUI edits: overridden fields keep the values
    /// from `saved` (the file), everything else comes from `edited`.
    pub fn persistable(&self, edited: &Config, saved: &Config) -> Config {
        let mut c = edited.clone();
        if self.label.is_some() {
            c.instance = saved.instance.clone();
        }
        if self.source.is_some() {
            c.source = saved.source.clone();
        }
        if self.zmq_port.is_some() {
            c.output.zmq_enabled = saved.output.zmq_enabled;
            c.output.zmq_bind = saved.output.zmq_bind.clone();
        }
        if self.udp_port.is_some() || self.udp_target.is_some() {
            c.output.enabled = saved.output.enabled;
            c.output.bind = saved.output.bind.clone();
            c.output.target = saved.output.target.clone();
        }
        if self.record {
            c.recording.enabled = saved.recording.enabled;
        }
        c
    }
}
/// A camera source keeping the config's size and rate where possible.
pub fn camera_source(config: &Config, device: &str) -> SourceConfig {
    let (width, height, fps) = match &config.source {
        SourceConfig::Camera {
            width, height, fps, ..
        } => (*width, *height, *fps),
        _ => (640, 480, 30),
    };
    SourceConfig::Camera {
        device: device.to_owned(),
        width,
        height,
        fps,
    }
}
/// Update `old` to hold exactly `new`'s values, keeping comments, key order
/// and formatting wherever a key survives.
fn merge_table(old: &mut toml_edit::Table, new: &toml_edit::Table) {
    let stale: Vec<String> = old
        .iter()
        .map(|(key, _)| key.to_owned())
        .filter(|key| !new.contains_key(key))
        .collect();
    for key in stale {
        old.remove(&key);
    }
    for (key, item) in new.iter() {
        match (old.get_mut(key), item) {
            (Some(toml_edit::Item::Table(o)), toml_edit::Item::Table(n)) => merge_table(o, n),
            (Some(toml_edit::Item::Value(o)), toml_edit::Item::Value(n)) => {
                let decor = o.decor().clone();
                *o = n.clone();
                *o.decor_mut() = decor;
            }
            (Some(slot), _) => *slot = item.clone(),
            (None, _) => {
                old.insert(key, item.clone());
            }
        }
    }
}
/// Replace only after a complete write; a crash cannot leave a half-written TOML/PNG.
pub fn atomic_write(path: &Path, contents: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(contents)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
fn validate_point(p: Point, w: u32, h: u32) -> Result<()> {
    ensure!(
        p.x.is_finite()
            && p.y.is_finite()
            && p.x >= 0.0
            && p.y >= 0.0
            && p.x < w as f32
            && p.y < h as f32,
        "Point lies outside configured image dimensions"
    );
    Ok(())
}
