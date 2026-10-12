use crate::{
    config::{Config, Point, SourceConfig},
    frame::{GrayFrame, unix_ns},
    tracker::wrap_angle,
};
use anyhow::{Context, Result, ensure};
use std::{
    io::Read,
    path::Path,
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
pub fn open(config: &Config, path: &Path, child: ChildControl) -> Result<Box<dyn FrameSource>> {
    match &config.source {
        SourceConfig::Demo {
            width,
            height,
            fps,
            beat_hz,
        } => Ok(Box::new(DemoSource::new(*width, *height, *fps, *beat_hz))),
        SourceConfig::Camera {
            device,
            width,
            height,
            fps,
        } => open_camera(device, *width, *height, *fps),
        SourceConfig::Video {
            path: video,
            width,
            height,
            repeat,
            ..
        } => {
            let input = config.resolve(path, video);
            ensure!(
                input.is_file(),
                "Video file does not exist: {}",
                input.display()
            );
            let mut args = vec!["-re".to_string()];
            if *repeat {
                args.extend(["-stream_loop".into(), "-1".into()]);
            }
            args.extend(["-i".into(), input.to_string_lossy().into_owned()]);
            Ok(Box::new(FfmpegSource::new(args, *width, *height, child)?))
        }
        SourceConfig::Network {
            url, width, height, ..
        } => Ok(Box::new(FfmpegSource::new(
            vec![
                "-rw_timeout".into(),
                "5000000".into(),
                "-i".into(),
                url.clone(),
            ],
            *width,
            *height,
            child,
        )?)),
    }
}

pub struct DemoSource {
    width: u32,
    height: u32,
    fps: u32,
    beat: f32,
    start: Instant,
    sequence: u64,
    blank: Vec<u8>,
}
impl DemoSource {
    pub fn new(width: u32, height: u32, fps: u32, beat: f32) -> Self {
        let mut blank = Vec::with_capacity(width as usize * height as usize);
        for y in 0..height {
            for x in 0..width {
                blank.push(
                    (215.0
                        + 10.0 * x as f32 / width as f32
                        + 6.0 * ((y as f32 / height as f32) * 5.0).sin()) as u8,
                );
            }
        }
        Self {
            width,
            height,
            fps,
            beat,
            start: Instant::now(),
            sequence: 0,
            blank,
        }
    }
    pub fn blank_frame(&self) -> GrayFrame {
        GrayFrame {
            width: self.width,
            height: self.height,
            pixels: self.blank.clone(),
            timestamp_ns: unix_ns(),
            sequence: 0,
        }
    }
    pub fn render(&self, t: f32, sequence: u64) -> GrayFrame {
        let mut frame = self.blank_frame();
        frame.sequence = sequence;
        let sx = self.width as f32 / 640.0;
        let sy = self.height as f32 / 480.0;
        let phase = t * self.beat * std::f32::consts::TAU;
        let left = std::f32::consts::PI - 0.72 + 0.27 * phase.sin();
        let right = 0.72 - 0.24 * (phase + 0.12).sin();
        for y in 0..self.height {
            for x in 0..self.width {
                let p = Point {
                    x: x as f32 / sx,
                    y: y as f32 / sy,
                };
                let wing = |hinge: Point, a: f32| {
                    let r = hinge.distance(p);
                    let delta = wrap_angle(hinge.angle_to(p) - a);
                    let radial =
                        ((r - 20.0) / 20.0).clamp(0.0, 1.0) * ((165.0 - r) / 25.0).clamp(0.0, 1.0);
                    radial * (-0.5 * (delta / 0.19).powi(2)).exp()
                };
                let idx = y as usize * self.width as usize + x as usize;
                let blur = wing(Point { x: 296.0, y: 240.0 }, left)
                    .max(wing(Point { x: 344.0, y: 240.0 }, right));
                frame.pixels[idx] = frame.pixels[idx].saturating_sub((14.0 * blur).round() as u8);
                let body = ((p.x - 320.0) / 24.0).powi(2) + ((p.y - 242.0) / 35.0).powi(2) < 1.0;
                let abdomen = ((p.x - 320.0 - 6.0 * (phase * 0.3).sin()) / 18.0).powi(2)
                    + ((p.y - 299.0) / 39.0).powi(2)
                    < 1.0;
                let head = ((p.x - 320.0 - 3.0 * (phase * 0.4).sin()) / 19.0).powi(2)
                    + ((p.y - 179.0) / 20.0).powi(2)
                    < 1.0;
                if body || abdomen || head {
                    frame.pixels[idx] = 28;
                }
            }
        }
        frame
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

#[cfg(feature = "camera")]
fn open_camera(device: &str, width: u32, height: u32, fps: u32) -> Result<Box<dyn FrameSource>> {
    initialize_camera()?;
    use nokhwa::{
        Camera,
        pixel_format::RgbFormat,
        utils::{
            CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
            Resolution,
        },
    };
    let automatic = device.eq_ignore_ascii_case("auto");
    let index = if automatic {
        nokhwa::query(nokhwa::utils::ApiBackend::Auto)
            .context("Enumerate cameras")?
            .first()
            .ok_or_else(|| {
                anyhow::anyhow!("No camera found. Connect a webcam, then Restart in Settings.")
            })?
            .index()
            .clone()
    } else {
        device
            .parse::<u32>()
            .map(CameraIndex::Index)
            .unwrap_or_else(|_| CameraIndex::String(device.to_owned()))
    };
    #[cfg(target_os = "linux")]
    let index = if device.starts_with('/') {
        // Nokhwa's V4L2 adapter accepts numerical indices, not device paths.
        // Resolve stable /dev/v4l/by-id aliases before extracting /dev/videoN.
        let path = std::fs::canonicalize(device)
            .with_context(|| format!("Resolve camera device {device}"))?;
        let number = path
            .to_str()
            .and_then(|p| p.strip_prefix("/dev/video"))
            .and_then(|n| n.parse::<u32>().ok())
            .ok_or_else(|| anyhow::anyhow!("Camera path must resolve to /dev/videoN"))?;
        CameraIndex::Index(number)
    } else {
        index
    };
    let mut camera = None;
    let mut errors = Vec::new();
    // Automatic selection tries the requested mode, then negotiates the nearest
    // supported mode. Explicit device configurations retain exact acquisition.
    for exact in [true, false] {
        if !exact && !automatic {
            break;
        }
        for format in [
            FrameFormat::MJPEG,
            FrameFormat::YUYV,
            FrameFormat::NV12,
            FrameFormat::RAWRGB,
            FrameFormat::RAWBGR,
            FrameFormat::GRAY,
        ] {
            let mode = CameraFormat::new(Resolution::new(width, height), format, fps);
            let request = RequestedFormat::new::<RgbFormat>(if exact {
                RequestedFormatType::Exact(mode)
            } else {
                RequestedFormatType::Closest(mode)
            });
            match Camera::new(index.clone(), request) {
                Ok(mut candidate) => match candidate.open_stream() {
                    Ok(()) => {
                        camera = Some(candidate);
                        break;
                    }
                    Err(e) => errors.push(e.to_string()),
                },
                Err(e) => errors.push(e.to_string()),
            }
        }
        if camera.is_some() {
            break;
        }
    }
    let camera=camera.ok_or_else(||anyhow::anyhow!("Cannot open camera {device} at {width}×{height} @ {fps} fps. Select a supported mode. {}",errors.join("; ")))?;
    Ok(Box::new(NativeCamera {
        camera,
        width,
        height,
        sequence: 0,
    }))
}
#[cfg(not(feature = "camera"))]
fn open_camera(_: &str, _: u32, _: u32, _: u32) -> Result<Box<dyn FrameSource>> {
    anyhow::bail!("Built without native camera support; enable the camera feature")
}
#[cfg(feature = "camera")]
struct NativeCamera {
    camera: nokhwa::Camera,
    width: u32,
    height: u32,
    sequence: u64,
}
#[cfg(feature = "camera")]
impl FrameSource for NativeCamera {
    fn next_frame(&mut self, stop: &AtomicBool) -> Result<Option<GrayFrame>> {
        if stop.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let buffer = self.camera.frame().context("Read camera frame")?;
        let time = unix_ns();
        let rgb = buffer
            .decode_image::<nokhwa::pixel_format::RgbFormat>()
            .context("Decode camera pixels")?;
        // Preserve the configured coordinate system if automatic negotiation
        // chose another camera resolution. Capture FPS is measured from arrivals.
        let rgb = if rgb.width() != self.width || rgb.height() != self.height {
            image::imageops::resize(
                &rgb,
                self.width,
                self.height,
                image::imageops::FilterType::Triangle,
            )
        } else {
            rgb
        };
        let (w, h) = (rgb.width(), rgb.height());
        let pixels = rgb
            .into_raw()
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8)
            .collect();
        let frame = GrayFrame::new(w, h, pixels, time, self.sequence)?;
        self.sequence += 1;
        Ok(Some(frame))
    }
}
#[cfg(feature = "camera")]
impl Drop for NativeCamera {
    fn drop(&mut self) {
        let _ = self.camera.stop_stream();
    }
}

pub fn initialize_camera() -> Result<()> {
    #[cfg(feature = "camera")]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        nokhwa::nokhwa_initialize(move |granted| {
            let _ = tx.send(granted);
        });
        ensure!(
            rx.recv_timeout(Duration::from_secs(60))
                .context("Camera permission request timed out")?,
            "Camera access denied by OS"
        );
    }
    Ok(())
}
pub fn list_cameras() -> Result<Vec<String>> {
    #[cfg(feature = "camera")]
    {
        initialize_camera()?;
        Ok(nokhwa::query(nokhwa::utils::ApiBackend::Auto)?
            .iter()
            .map(|c| format!("{}: {}", c.index(), c.human_name()))
            .collect())
    }
    #[cfg(not(feature = "camera"))]
    {
        anyhow::bail!("Built without camera support")
    }
}

struct FfmpegSource {
    stdout: ChildStdout,
    control: ChildControl,
    width: u32,
    height: u32,
    sequence: u64,
}
impl FfmpegSource {
    fn new(args: Vec<String>, width: u32, height: u32, control: ChildControl) -> Result<Self> {
        let adjacent = std::env::current_exe()?
            .parent()
            .unwrap_or(Path::new("."))
            .join(if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            });
        let executable = std::env::var_os("REKKE_FFMPEG")
            .or_else(|| std::env::var_os("WINGTRACE_FFMPEG"))
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                if adjacent.is_file() {
                    adjacent
                } else {
                    "ffmpeg".into()
                }
            });
        let mut command = Command::new(executable);
        command
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(args)
            .args([
                "-an",
                "-sn",
                "-dn",
                "-vf",
                &format!("scale={width}:{height}"),
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
            "Start FFmpeg: install ffmpeg or put it beside Rekke for video/network input",
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
