//! Track a whole video as fast as possible and summarise it: the engine of
//! `rekkefly track` and of the MCP `track_video` tool.
use crate::{
    config::{Config, Method, SourceConfig},
    frame::GrayFrame,
    source,
    tracker::{Measurement, Tracker},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Instant,
};

#[derive(Clone, Debug, Default)]
pub struct TrackOptions {
    /// Stop after this many frames.
    pub frames: Option<u64>,
    /// Use this detector for both wings instead of the config's.
    pub method: Option<Method>,
    /// Write one row per frame: angles in degrees and processing time.
    pub csv: Option<PathBuf>,
    /// Write every full measurement as JSON lines (the UDP format).
    pub jsonl: Option<PathBuf>,
    /// Use the video's own markers (a `.toml` beside it, or the Bogong moth
    /// example's) instead of the config's.
    pub video_markers: bool,
}
/// Angles over a run, in degrees.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct AngleStats {
    /// Frames with a measurement.
    pub valid: u64,
    pub mean: Option<f64>,
    pub sd: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}
#[derive(Default)]
struct Accumulator {
    n: u64,
    sum: f64,
    squares: f64,
    min: f64,
    max: f64,
}
impl Accumulator {
    fn add(&mut self, value: Option<f32>) {
        let Some(v) = value.map(|v| v.to_degrees() as f64) else {
            return;
        };
        if self.n == 0 {
            (self.min, self.max) = (v, v);
        }
        self.n += 1;
        self.sum += v;
        self.squares += v * v;
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }
    fn stats(&self) -> AngleStats {
        if self.n == 0 {
            return AngleStats::default();
        }
        let mean = self.sum / self.n as f64;
        let variance = (self.squares / self.n as f64 - mean * mean).max(0.0);
        let round = |v: f64| (v * 100.0).round() / 100.0;
        AngleStats {
            valid: self.n,
            mean: Some(round(mean)),
            sd: Some(round(variance.sqrt())),
            min: Some(round(self.min)),
            max: Some(round(self.max)),
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Timing {
    /// Frames per second for the whole run, decoding included.
    pub frames_per_second: f64,
    pub seconds: f64,
    /// Tracking time per frame, microseconds.
    pub mean_us: f64,
    pub p99_us: f64,
    pub max_us: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub video: String,
    pub frames: u64,
    pub width: u32,
    pub height: u32,
    /// The detectors used for the left and right wings.
    pub method: [Method; 2],
    /// Whether the video's own markers were used.
    pub video_markers: bool,
    pub timing: Timing,
    pub left: AngleStats,
    pub right: AngleStats,
    pub left_minus_right: AngleStats,
    pub head: AngleStats,
    pub abdomen: AngleStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub csv: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonl: Option<PathBuf>,
}
/// The config that `track` uses for `video`: `config` (or the defaults) with
/// the video as its source, its own markers if asked, and `method`.
pub fn video_config(
    video: &Path,
    config: Option<&Config>,
    options: &TrackOptions,
) -> Result<(Config, bool)> {
    // Relative to here, not to the config's folder.
    let video = &video
        .canonicalize()
        .with_context(|| format!("Video {}", video.display()))?;
    let mut c = config.cloned().unwrap_or_default();
    let (mut width, mut height, mut fps) = c.source.dimensions();
    // Without a config, track at the video's own size and rate.
    if config.is_none()
        && crate::avi::is_avi(video)
        && let Ok(reader) = crate::avi::AviReader::open(video)
    {
        let info = reader.info();
        (width, height) = (info.width, info.height);
        fps = (info.fps.round() as u32).max(1);
    }
    c.source = SourceConfig::Video {
        path: video.to_string_lossy().into_owned(),
        width,
        height,
        fps,
        repeat: false,
        realtime: false,
    };
    if config.is_none() {
        // The default markers, scaled to the video's size.
        c = c.with_geometry_of(&Config::default()).unwrap_or(c);
    }
    let mut own = false;
    if options.video_markers
        && let Some(markers) = source::sample_config(video).and_then(|e| c.with_geometry_of(&e))
    {
        c = markers;
        own = true;
    }
    if let Some(method) = options.method {
        c.tracking.left.method = method;
        c.tracking.right.method = method;
    }
    c.validate()?;
    Ok((c, own))
}
/// Track every frame of `video`. `config_path` resolves the background and
/// relative paths; the config file itself is never written.
pub fn track(
    video: &Path,
    config: Option<&Config>,
    config_path: &Path,
    options: &TrackOptions,
) -> Result<Summary> {
    ensure!(
        video.is_file(),
        "Video file does not exist: {}",
        video.display()
    );
    ensure!(
        options.frames.is_none_or(|n| n > 0),
        "frames must be positive"
    );
    let (config, own) = video_config(video, config, options)?;
    let (w, h, _) = config.source.dimensions();
    let background = if config.tracking.subtract_background {
        let path = config.resolve(config_path, &config.tracking.background_path);
        Some(GrayFrame::load(&path).with_context(|| format!("Background {}", path.display()))?)
    } else {
        None
    };
    let child: source::ChildControl = Arc::new(Mutex::new(None));
    let mut opened = source::open(&config, config_path, child)?.source;
    let tracker = Tracker::new(&config, w, h);
    let mut csv = options
        .csv
        .as_deref()
        .map(|p| {
            std::fs::File::create(p)
                .map(std::io::BufWriter::new)
                .with_context(|| format!("Create {}", p.display()))
        })
        .transpose()?;
    if let Some(out) = &mut csv {
        writeln!(
            out,
            "frame,seconds,left_deg,right_deg,left_minus_right_deg,left_plus_right_deg,head_deg,abdomen_deg,processing_us"
        )?;
    }
    let mut jsonl = options
        .jsonl
        .as_deref()
        .map(|p| {
            std::fs::File::create(p)
                .map(std::io::BufWriter::new)
                .with_context(|| format!("Create {}", p.display()))
        })
        .transpose()?;
    let fps = config.source.dimensions().2.max(1) as f64;
    let stop = AtomicBool::new(false);
    let start = Instant::now();
    let mut stats: [Accumulator; 5] = Default::default();
    let mut times = Vec::new();
    let mut frames = 0u64;
    let degrees = |v: Option<f32>| {
        v.map(|v| format!("{:.3}", v.to_degrees()))
            .unwrap_or_default()
    };
    while options.frames.is_none_or(|n| frames < n) {
        let Some(frame) = opened.next_frame(&stop)? else {
            break;
        };
        let m: Measurement = tracker.process(&frame, background.as_ref(), 0)?;
        for (s, v) in stats.iter_mut().zip([
            m.left.angle_rad,
            m.right.angle_rad,
            m.left_minus_right_rad,
            m.head.angle_rad,
            m.abdomen.angle_rad,
        ]) {
            s.add(v);
        }
        times.push(m.processing_us);
        if let Some(out) = &mut csv {
            writeln!(
                out,
                "{frames},{:.4},{},{},{},{},{},{},{:.1}",
                frames as f64 / fps,
                degrees(m.left.angle_rad),
                degrees(m.right.angle_rad),
                degrees(m.left_minus_right_rad),
                degrees(m.left_plus_right_rad),
                degrees(m.head.angle_rad),
                degrees(m.abdomen.angle_rad),
                m.processing_us
            )?;
        }
        if let Some(out) = &mut jsonl {
            serde_json::to_writer(&mut *out, &m)?;
            out.write_all(b"\n")?;
        }
        frames += 1;
    }
    ensure!(
        frames > 0,
        "No frames could be read from {}",
        video.display()
    );
    if let Some(out) = &mut csv {
        out.flush()?;
    }
    if let Some(out) = &mut jsonl {
        out.flush()?;
    }
    let seconds = start.elapsed().as_secs_f64();
    let mean = times.iter().map(|&t| t as f64).sum::<f64>() / times.len() as f64;
    times.sort_by(f32::total_cmp);
    let p99 = times[((times.len() - 1) as f64 * 0.99).round() as usize] as f64;
    let round = |v: f64| (v * 10.0).round() / 10.0;
    let [left, right, left_minus_right, head, abdomen] = stats.map(|s| s.stats());
    Ok(Summary {
        video: video.to_string_lossy().into_owned(),
        frames,
        width: w,
        height: h,
        method: [config.tracking.left.method, config.tracking.right.method],
        video_markers: own,
        timing: Timing {
            frames_per_second: round(frames as f64 / seconds.max(1e-9)),
            seconds: (seconds * 1000.0).round() / 1000.0,
            mean_us: round(mean),
            p99_us: round(p99),
            max_us: round(*times.last().unwrap() as f64),
        },
        left,
        right,
        left_minus_right,
        head,
        abdomen,
        csv: options.csv.clone(),
        jsonl: options.jsonl.clone(),
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct Benchmark {
    pub frames: u64,
    pub width: u32,
    pub height: u32,
    pub angle_bins: usize,
    pub radial_samples: usize,
    pub sectors: usize,
    /// Tracking time per frame, microseconds.
    pub mean_us: f64,
    pub frames_per_second: f64,
}
/// Time the tracker on a fixed synthetic frame; acquires no camera.
/// Includes foreground preparation and tracking; excludes acquisition,
/// GUI, serialization and output.
pub fn benchmark(config: &Config, count: u64) -> Result<Benchmark> {
    ensure!(count > 0, "Benchmark frame count must be positive");
    let (w, h, fps) = config.source.dimensions();
    let demo = source::DemoSource::new(w, h, fps, 1.0);
    let raw = demo.render(0.031, 0);
    let bg = demo.blank_frame();
    let tracker = Tracker::new(config, w, h);
    for _ in 0..20 {
        std::hint::black_box(tracker.process(&raw, Some(&bg), 0)?);
    }
    let start = Instant::now();
    for _ in 0..count {
        std::hint::black_box(tracker.process(&raw, Some(&bg), 0)?);
    }
    let seconds = start.elapsed().as_secs_f64();
    Ok(Benchmark {
        frames: count,
        width: w,
        height: h,
        angle_bins: config.tracking.angle_bins,
        radial_samples: config.tracking.radial_samples,
        sectors: config.sectors().iter().filter(|s| s.enabled).count(),
        mean_us: (seconds * 1e7 / count as f64).round() / 10.0,
        frames_per_second: (count as f64 / seconds).round(),
    })
}
