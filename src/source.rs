//! Image sources: cameras, video files, network streams and a simulated fly.
use crate::{
    avi::{self, AviReader},
    config::{Config, SourceConfig},
    frame::{GrayFrame, unix_ns},
    synth::{Scene, SceneOptions},
};
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub trait FrameSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> Result<Option<GrayFrame>>;
}
pub type ChildControl = Arc<Mutex<Option<Child>>>;
/// An open source and a short human-readable description of it.
pub struct Opened {
    pub source: Box<dyn FrameSource>,
    pub description: String,
}
pub fn open(config: &Config, path: &Path, child: ChildControl) -> Result<Opened> {
    match &config.source {
        SourceConfig::Demo {
            width,
            height,
            fps,
            beat_hz,
        } => Ok(Opened {
            source: Box::new(DemoSource::new(*width, *height, *fps, *beat_hz)),
            description: "Simulated fly".into(),
        }),
        SourceConfig::Camera {
            device,
            width,
            height,
            fps,
        } => {
            let (source, description) = crate::camera::open(device, *width, *height, *fps)?;
            Ok(Opened {
                source,
                description,
            })
        }
        SourceConfig::Video {
            path: video,
            width,
            height,
            repeat,
            realtime,
            ..
        } => {
            let mut input = config.resolve(path, video);
            // Seeded configs point at the bundled example wherever it is installed.
            if !input.is_file()
                && input.file_name().is_some_and(|n| n == EXAMPLE)
                && let Some(sample) = sample_video()
            {
                input = sample;
            }
            ensure!(
                input.is_file(),
                "Video file does not exist: {}",
                input.display()
            );
            open_video(&input, *width, *height, *repeat, *realtime, child)
        }
        SourceConfig::Network {
            url, width, height, ..
        } => Ok(Opened {
            source: Box::new(FfmpegSource::new(
                vec![
                    "-rw_timeout".into(),
                    "5000000".into(),
                    "-i".into(),
                    url.clone(),
                ],
                *width,
                *height,
                child,
            )?),
            description: format!("Stream {url}"),
        }),
    }
}
fn open_video(
    input: &Path,
    width: u32,
    height: u32,
    repeat: bool,
    realtime: bool,
    child: ChildControl,
) -> Result<Opened> {
    let name = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let native = if avi::is_avi(input) {
        Some(AviSource::open(input, width, height, repeat, realtime))
    } else {
        None
    };
    match native {
        Some(Ok(source)) => {
            let info = source.reader.info();
            let description = format!(
                "Video {name} · {}×{} {} @ {:.2} fps{}",
                info.width,
                info.height,
                info.fourcc,
                info.fps,
                if realtime {
                    ""
                } else {
                    " · as fast as possible"
                }
            );
            Ok(Opened {
                source: Box::new(source),
                description,
            })
        }
        other => {
            let mut args = Vec::new();
            if realtime {
                args.push("-re".to_string());
            }
            if repeat {
                args.extend(["-stream_loop".into(), "-1".into()]);
            }
            args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
            let source = FfmpegSource::new(args, width, height, child).map_err(|error| {
                match other {
                    Some(Err(native)) => anyhow::anyhow!(
                        "{native:#}. FFmpeg fallback also failed: {error:#}"
                    ),
                    _ => error.context(
                        "This video format needs FFmpeg. Install FFmpeg, or use an AVI recorded by Rekkefly",
                    ),
                }
            })?;
            Ok(Opened {
                source: Box::new(source),
                description: format!("Video {name} via FFmpeg"),
            })
        }
    }
}

/// The bundled example video: a tethered Bogong moth.
pub const EXAMPLE: &str = "bogong-moth.avi";
/// The example's markers and sectors, used when it plays instead of a camera.
const EXAMPLE_CONFIG: &str = include_str!("../configs/bogong-moth.toml");
/// Where the sample video may live, most specific first. A `sample.avi` in
/// the user's Rekkefly folder replaces the example shipped with the app.
pub fn sample_candidates(exe: Option<&Path>, user: Option<&Path>, cwd: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(path) =
        std::env::var_os("REKKEFLY_SAMPLE").or_else(|| std::env::var_os("REKKE_SAMPLE"))
    {
        paths.push(PathBuf::from(path));
    }
    if let Some(user) = user {
        paths.push(user.join("samples/sample.avi"));
    }
    if let Some(dir) = exe.and_then(Path::parent) {
        paths.push(dir.join("samples").join(EXAMPLE));
        paths.push(dir.join("../Resources/samples").join(EXAMPLE));
        paths.push(dir.join("../share/rekkefly/samples").join(EXAMPLE));
        paths.push(dir.join("../../samples").join(EXAMPLE));
    }
    paths.push(cwd.join("samples").join(EXAMPLE));
    paths
}
/// The config whose markers fit a sample video: `<name>.toml` beside it, or
/// the built-in Bogong moth example for the bundled clip.
pub fn sample_config(video: &Path) -> Option<Config> {
    let beside = video.with_extension("toml");
    if beside.is_file() {
        return Config::load(&beside).ok();
    }
    if video.file_name().is_some_and(|n| n == EXAMPLE) {
        return toml::from_str(EXAMPLE_CONFIG).ok();
    }
    None
}
pub fn sample_video() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok();
    let cwd = std::env::current_dir().unwrap_or_default();
    sample_candidates(
        exe.as_deref(),
        crate::installation::user_directory().as_deref(),
        &cwd,
    )
    .into_iter()
    .find(|p| p.is_file())
    .and_then(|p| p.canonicalize().ok())
}

/// A synthetic tethered fly with stroke envelopes, antennae and legs.
pub struct DemoSource {
    scene: Scene,
    fps: u32,
    start: Instant,
    sequence: u64,
}
impl DemoSource {
    pub fn new(width: u32, height: u32, fps: u32, beat: f32) -> Self {
        let options = SceneOptions {
            modulation_hz: beat.clamp(0.05, 20.0),
            ..SceneOptions::default()
        };
        Self {
            scene: Scene::new(width, height, options),
            fps,
            start: Instant::now(),
            sequence: 0,
        }
    }
    /// The empty, evenly lit scene: a clean background reference.
    pub fn blank_frame(&self) -> GrayFrame {
        self.scene.empty()
    }
    pub fn render(&self, t: f32, sequence: u64) -> GrayFrame {
        self.scene.render(t, sequence)
    }
    pub fn scene(&self) -> &Scene {
        &self.scene
    }
}
impl FrameSource for DemoSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> Result<Option<GrayFrame>> {
        let due = self.start + Duration::from_secs_f64(self.sequence as f64 / self.fps as f64);
        while Instant::now() < due {
            if stop.load(Ordering::Relaxed) {
                return Ok(None);
            }
            std::thread::sleep((due - Instant::now()).min(Duration::from_millis(5)));
        }
        // Skip simulated exposures if overloaded rather than building latency.
        self.sequence = self
            .sequence
            .max((self.start.elapsed().as_secs_f64() * self.fps as f64) as u64);
        let frame = self.render(self.sequence as f32 / self.fps as f32, self.sequence);
        self.sequence += 1;
        Ok(Some(frame))
    }
}

/// Native AVI playback: no FFmpeg, paced at the file's frame rate or
/// as fast as possible for offline analysis.
pub struct AviSource {
    reader: AviReader<BufReader<File>>,
    width: u32,
    height: u32,
    repeat: bool,
    realtime: bool,
    interval: Duration,
    start: Instant,
    played: u64,
    sequence: u64,
    chunk: Vec<u8>,
    pixels: Vec<u8>,
}
impl AviSource {
    pub fn open(
        path: &Path,
        width: u32,
        height: u32,
        repeat: bool,
        realtime: bool,
    ) -> Result<Self> {
        let reader = AviReader::open(path)?;
        let fps = reader.info().fps;
        let interval = if fps.is_finite() && fps > 0.0 {
            Duration::from_secs_f64(1.0 / fps)
        } else {
            Duration::from_millis(33)
        };
        Ok(Self {
            reader,
            width,
            height,
            repeat,
            realtime,
            interval,
            start: Instant::now(),
            played: 0,
            sequence: 0,
            chunk: Vec::new(),
            pixels: Vec::new(),
        })
    }
}
impl FrameSource for AviSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> Result<Option<GrayFrame>> {
        if stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        if !self.reader.next_frame(&mut self.chunk, &mut self.pixels)? {
            if !self.repeat || self.sequence == 0 {
                return Ok(None);
            }
            self.reader.rewind();
            self.start = Instant::now();
            self.played = 0;
            if !self.reader.next_frame(&mut self.chunk, &mut self.pixels)? {
                return Ok(None);
            }
        }
        if self.realtime {
            let now = Instant::now();
            let due = self.start + self.interval.mul_f64(self.played as f64);
            if now > due + Duration::from_millis(500) {
                // Fell far behind (slow disk or debugger): resume pacing from now.
                self.start = now - self.interval.mul_f64(self.played as f64);
            }
            while Instant::now() < due {
                if stop.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                std::thread::sleep((due - Instant::now()).min(Duration::from_millis(5)));
            }
        }
        let info = self.reader.info();
        let pixels = crate::frame::fit(
            self.pixels.clone(),
            (info.width, info.height),
            self.width,
            self.height,
        )?;
        let frame = GrayFrame::new(self.width, self.height, pixels, unix_ns(), self.sequence)?;
        self.sequence += 1;
        self.played += 1;
        Ok(Some(frame))
    }
}

/// The FFmpeg executable: REKKEFLY_FFMPEG, one beside Rekkefly, or `ffmpeg` on PATH.
pub fn ffmpeg_executable() -> PathBuf {
    let adjacent = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join(if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        });
    std::env::var_os("REKKEFLY_FFMPEG")
        .or_else(|| std::env::var_os("REKKE_FFMPEG"))
        .or_else(|| std::env::var_os("WINGTRACE_FFMPEG"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if adjacent.is_file() {
                adjacent
            } else {
                "ffmpeg".into()
            }
        })
}
pub(crate) struct FfmpegSource {
    stdout: ChildStdout,
    control: ChildControl,
    width: u32,
    height: u32,
    sequence: u64,
}
impl FfmpegSource {
    pub(crate) fn new(
        args: Vec<String>,
        width: u32,
        height: u32,
        control: ChildControl,
    ) -> Result<Self> {
        let mut command = Command::new(ffmpeg_executable());
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(args)
            .args([
                "-an",
                "-sn",
                "-dn",
                "-vf",
                // Keep the aspect ratio: distortion would bias every angle.
                &format!(
                    "scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=gray"
                ),
                "-threads",
                "1",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().context(
            "Start FFmpeg: install ffmpeg or put it beside Rekkefly for this video/stream",
        )?;
        let stdout = child
            .stdout
            .take()
            .context("FFmpeg did not provide stdout")?;
        *control.lock().unwrap() = Some(child);
        Ok(Self {
            stdout,
            control,
            width,
            height,
            sequence: 0,
        })
    }
}
impl FrameSource for FfmpegSource {
    fn next_frame(&mut self, stop: &AtomicBool) -> Result<Option<GrayFrame>> {
        if stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let mut pixels = vec![0; self.width as usize * self.height as usize];
        match self.stdout.read_exact(&mut pixels) {
            Ok(()) => {}
            Err(_) if stop.load(Ordering::Relaxed) => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                let mut control = self.control.lock().unwrap();
                if let Some(child) = control.as_mut() {
                    let status = child.wait()?;
                    ensure!(
                        status.success(),
                        "FFmpeg exited with {status}; see terminal output for the camera/stream error"
                    );
                }
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }
        let frame = GrayFrame::new(self.width, self.height, pixels, unix_ns(), self.sequence)?;
        self.sequence += 1;
        Ok(Some(frame))
    }
}
impl Drop for FfmpegSource {
    fn drop(&mut self) {
        if let Some(mut child) = self.control.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub fn initialize_camera() -> Result<()> {
    crate::camera::initialize()
}
pub fn list_cameras() -> Result<Vec<String>> {
    Ok(crate::camera::list()?
        .iter()
        .map(ToString::to_string)
        .collect())
}
