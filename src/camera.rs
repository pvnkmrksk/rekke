//! Native USB/webcam capture through Nokhwa, with dependable device selection.
//!
//! `device` in a config may be:
//! - `"auto"`: the first camera that opens (busy cameras are skipped);
//! - a number: the camera's position in `rekke --list-cameras` (0 = first);
//! - a path such as `/dev/video2` or `/dev/v4l/by-id/...` (Linux, stable);
//! - any other text: a case-insensitive part of the camera name or its ID.
//!
//! On Linux each USB camera also creates metadata-only `/dev/video*` nodes and
//! the kernel's directory order is arbitrary. Rekke lists only nodes that can
//! capture video, sorted by number, so `auto` and numbers mean real cameras.
use crate::source::FrameSource;
use anyhow::{Context, Result, bail, ensure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    Auto,
    Index(usize),
    Path(String),
    Name(String),
}
pub fn parse(device: &str) -> Selector {
    let device = device.trim();
    if device.is_empty() || device.eq_ignore_ascii_case("auto") {
        Selector::Auto
    } else if let Ok(n) = device.parse::<usize>() {
        Selector::Index(n)
    } else if device.starts_with('/') {
        Selector::Path(device.to_owned())
    } else {
        Selector::Name(device.to_owned())
    }
}
/// A camera as shown by `--list-cameras`; `position` is what `device = "N"` means.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraEntry {
    pub position: usize,
    pub name: String,
    /// Device path (Linux) or the OS's unique ID, if known.
    pub id: String,
    /// The backend's own index for this camera.
    pub backend_index: u32,
}
impl std::fmt::Display for CameraEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.id.is_empty() {
            write!(f, "{}: {}", self.position, self.name)
        } else {
            write!(f, "{}: {} ({})", self.position, self.name, self.id)
        }
    }
}
/// Which listed cameras a selector may use, in the order to try them.
/// `Err` means the selector names something that is not in the list.
pub fn candidates(selector: &Selector, cameras: &[CameraEntry]) -> Result<Vec<CameraEntry>> {
    match selector {
        Selector::Auto => {
            ensure!(!cameras.is_empty(), "No camera found");
            Ok(cameras.to_vec())
        }
        Selector::Index(n) => cameras.get(*n).cloned().map(|c| vec![c]).with_context(|| {
            format!(
                "Camera {n} not found; {} camera(s) available (numbering starts at 0)",
                cameras.len()
            )
        }),
        Selector::Path(path) => {
            let resolved = std::fs::canonicalize(path)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| path.clone());
            let found: Vec<_> = cameras
                .iter()
                .filter(|c| c.id == *path || c.id == resolved)
                .cloned()
                .collect();
            ensure!(!found.is_empty(), "{path} is not a video capture camera");
            Ok(found)
        }
        Selector::Name(name) => {
            let lower = name.to_lowercase();
            let found: Vec<_> = cameras
                .iter()
                .filter(|c| c.id == *name || c.name.to_lowercase().contains(&lower))
                .cloned()
                .collect();
            ensure!(!found.is_empty(), "No camera named like \"{name}\"");
            Ok(found)
        }
    }
}
#[cfg(feature = "camera")]
fn describe(cameras: &[CameraEntry]) -> String {
    if cameras.is_empty() {
        "No cameras detected".into()
    } else {
        format!(
            "Available: {}",
            cameras
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )
    }
}

#[cfg(feature = "camera")]
mod native {
    use super::*;
    use crate::frame::{GrayFrame, unix_ns};
    use nokhwa::{
        Camera,
        pixel_format::RgbFormat,
        utils::{
            CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType,
            Resolution,
        },
    };
    use std::sync::atomic::{AtomicBool, Ordering};
    pub fn initialize() -> Result<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        nokhwa::nokhwa_initialize(move |granted| {
            let _ = tx.send(granted);
        });
        ensure!(
            rx.recv_timeout(std::time::Duration::from_secs(60))
                .context("Camera permission request timed out")?,
            "Camera access denied by the OS. Allow Rekke in the system camera privacy settings."
        );
        Ok(())
    }
    #[cfg(target_os = "linux")]
    pub fn list() -> Result<Vec<CameraEntry>> {
        use v4l::capability::Flags;
        let mut nodes: Vec<(u32, std::path::PathBuf)> = std::fs::read_dir("/dev")
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let n = name.strip_prefix("video")?.parse::<u32>().ok()?;
                        Some((n, e.path()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        nodes.sort();
        let mut cameras = Vec::new();
        let mut denied = Vec::new();
        for (number, path) in nodes {
            match v4l::Device::with_path(&path).and_then(|d| d.query_caps()) {
                Ok(caps) if caps.capabilities.contains(Flags::VIDEO_CAPTURE) => {
                    cameras.push(CameraEntry {
                        position: cameras.len(),
                        name: caps.card,
                        id: path.to_string_lossy().into_owned(),
                        backend_index: number,
                    });
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    denied.push(path.display().to_string())
                }
                Err(_) => {}
            }
        }
        if cameras.is_empty() && !denied.is_empty() {
            bail!(
                "Permission denied for {}. Add your user to the 'video' group (sudo usermod -aG video $USER), then log out and in.",
                denied.join(", ")
            );
        }
        Ok(cameras)
    }
    #[cfg(not(target_os = "linux"))]
    pub fn list() -> Result<Vec<CameraEntry>> {
        initialize()?;
        Ok(nokhwa::query(nokhwa::utils::ApiBackend::Auto)
            .context("Enumerate cameras")?
            .iter()
            .enumerate()
            .map(|(position, c)| CameraEntry {
                position,
                name: c.human_name(),
                id: c.misc(),
                backend_index: match c.index() {
                    CameraIndex::Index(i) => *i,
                    CameraIndex::String(_) => position as u32,
                },
            })
            .collect())
    }
    fn preference(format: FrameFormat) -> u32 {
        // Luma-native formats avoid color decoding; MJPEG saves USB bandwidth.
        match format {
            FrameFormat::GRAY => 0,
            FrameFormat::YUYV => 1,
            FrameFormat::MJPEG => 2,
            FrameFormat::NV12 => 3,
            _ => 4,
        }
    }
    /// Rank supported modes: exact size and rate first, then the nearest size
    /// (preferring larger, which is downscaled), then the nearest rate.
    pub fn rank(formats: &[CameraFormat], width: u32, height: u32, fps: u32) -> Vec<CameraFormat> {
        let mut ranked = formats.to_vec();
        ranked.sort_by_key(|f| {
            let (w, h) = (f.resolution().width(), f.resolution().height());
            let size = (w as i64 - width as i64).abs() + (h as i64 - height as i64).abs();
            let smaller = (w < width || h < height) as i64;
            let rate = (f.frame_rate() as i64 - fps as i64).abs();
            let slower = (f.frame_rate() < fps) as i64;
            (
                size > 0,
                smaller,
                size,
                slower,
                rate,
                preference(f.format()),
            )
        });
        ranked.dedup();
        ranked
    }
    fn open_one(
        entry: &CameraEntry,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<(Camera, CameraFormat)> {
        let index = CameraIndex::Index(entry.backend_index);
        let mut camera = Camera::new(
            index.clone(),
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::None),
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        let formats = camera.compatible_camera_formats().unwrap_or_default();
        let mut ranked = rank(&formats, width, height, fps);
        if ranked.is_empty() {
            // Some backends cannot enumerate modes: request common formats exactly.
            ranked = [FrameFormat::YUYV, FrameFormat::MJPEG, FrameFormat::NV12]
                .map(|f| CameraFormat::new(Resolution::new(width, height), f, fps))
                .to_vec();
        }
        let mut errors = Vec::new();
        for format in ranked.into_iter().take(4) {
            let request = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Exact(format));
            let result = camera
                .set_camera_requset(request)
                .and_then(|_| camera.open_stream());
            match result {
                Ok(()) => {
                    let actual = camera.camera_format();
                    return Ok((camera, actual));
                }
                Err(e) => errors.push(format!("{format}: {e}")),
            }
        }
        bail!("{}", errors.join("; "))
    }
    pub fn open(
        device: &str,
        width: u32,
        height: u32,
        fps: u32,
    ) -> Result<(Box<dyn FrameSource>, String)> {
        let selector = parse(device);
        // A missing device path fails at once, without enumerating cameras
        // (which can wait on an OS permission prompt).
        if let Selector::Path(path) = &selector {
            ensure!(
                std::path::Path::new(path).exists(),
                "Camera device {path} does not exist (unplugged?)"
            );
        }
        let cameras = list()?;
        let selected = candidates(&selector, &cameras).map_err(|e| {
            if cameras.is_empty() {
                anyhow::anyhow!("No camera found")
            } else {
                anyhow::anyhow!("{e}. {}", describe(&cameras))
            }
        })?;
        let mut failures = Vec::new();
        for entry in &selected {
            match open_one(entry, width, height, fps) {
                Ok((camera, format)) => {
                    let label = format!(
                        "{} · {}×{} {:?} @ {} fps",
                        if entry.id.is_empty() {
                            entry.name.clone()
                        } else {
                            format!("{} ({})", entry.name, entry.id)
                        },
                        format.resolution().width(),
                        format.resolution().height(),
                        format.format(),
                        format.frame_rate()
                    );
                    return Ok((
                        Box::new(NativeCamera {
                            camera,
                            width,
                            height,
                            sequence: 0,
                            luma: Vec::new(),
                        }),
                        label,
                    ));
                }
                Err(e) => failures.push(format!("{entry}: {e:#}")),
            }
        }
        bail!(
            "Cannot open camera \"{device}\" (it may be in use by another program or Rekke window). {}",
            failures.join(" | ")
        )
    }
    struct NativeCamera {
        camera: Camera,
        width: u32,
        height: u32,
        sequence: u64,
        luma: Vec<u8>,
    }
    impl NativeCamera {
        /// Direct luma from V4L2 buffers, whose format labels are reliable.
        #[cfg(target_os = "linux")]
        fn fast_luma(&mut self, buffer: &nokhwa::Buffer) -> Option<(u32, u32)> {
            let (w, h) = (buffer.resolution().width(), buffer.resolution().height());
            let n = w as usize * h as usize;
            let data = buffer.buffer();
            self.luma.clear();
            match buffer.source_frame_format() {
                FrameFormat::GRAY if data.len() >= n => self.luma.extend_from_slice(&data[..n]),
                FrameFormat::NV12 if data.len() >= n * 3 / 2 => {
                    self.luma.extend_from_slice(&data[..n])
                }
                FrameFormat::YUYV if data.len() == n * 2 => {
                    self.luma.extend(data.iter().step_by(2).copied())
                }
                FrameFormat::MJPEG => {
                    let options = zune_core::options::DecoderOptions::default()
                        .jpeg_set_out_colorspace(zune_core::colorspace::ColorSpace::Luma);
                    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(
                        std::io::Cursor::new(data),
                        options,
                    );
                    let pixels = decoder.decode().ok()?;
                    let (dw, dh) = decoder.dimensions()?;
                    if pixels.len() != dw * dh {
                        return None;
                    }
                    self.luma = pixels;
                    return Some((dw as u32, dh as u32));
                }
                _ => return None,
            }
            Some((w, h))
        }
        #[cfg(not(target_os = "linux"))]
        fn fast_luma(&mut self, _: &nokhwa::Buffer) -> Option<(u32, u32)> {
            None
        }
    }
    impl FrameSource for NativeCamera {
        fn next_frame(&mut self, stop: &AtomicBool) -> Result<Option<GrayFrame>> {
            if stop.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let buffer = self.camera.frame().context("Read camera frame")?;
            let time = unix_ns();
            let (w, h) = match self.fast_luma(&buffer) {
                Some(size) => size,
                None => {
                    let rgb = buffer
                        .decode_image::<RgbFormat>()
                        .context("Decode camera pixels")?;
                    let size = (rgb.width(), rgb.height());
                    self.luma.clear();
                    self.luma
                        .extend(rgb.as_raw().as_chunks::<3>().0.iter().map(|p| {
                            ((p[0] as u32 * 77 + p[1] as u32 * 150 + p[2] as u32 * 29) >> 8) as u8
                        }));
                    size
                }
            };
            // Keep the configured coordinate system if the camera negotiated
            // another size, without distorting angles. FPS is measured from arrivals.
            let pixels = crate::frame::fit(
                std::mem::take(&mut self.luma),
                (w, h),
                self.width,
                self.height,
            )?;
            let frame = GrayFrame::new(self.width, self.height, pixels, time, self.sequence)?;
            self.sequence += 1;
            Ok(Some(frame))
        }
    }
    impl Drop for NativeCamera {
        fn drop(&mut self) {
            let _ = self.camera.stop_stream();
        }
    }
}

/// Open a camera; returns the source and a human-readable description.
pub fn open(
    device: &str,
    width: u32,
    height: u32,
    fps: u32,
) -> Result<(Box<dyn FrameSource>, String)> {
    #[cfg(feature = "camera")]
    {
        native::open(device, width, height, fps)
    }
    #[cfg(not(feature = "camera"))]
    {
        let _ = (device, width, height, fps);
        bail!("Built without native camera support; enable the camera feature")
    }
}
pub fn list() -> Result<Vec<CameraEntry>> {
    #[cfg(feature = "camera")]
    {
        native::list()
    }
    #[cfg(not(feature = "camera"))]
    {
        bail!("Built without camera support")
    }
}
pub fn initialize() -> Result<()> {
    #[cfg(feature = "camera")]
    {
        native::initialize()
    }
    #[cfg(not(feature = "camera"))]
    {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cams() -> Vec<CameraEntry> {
        [
            ("Front", "/dev/video0", 0),
            ("HD Pro Webcam C920", "/dev/video2", 2),
        ]
        .iter()
        .enumerate()
        .map(|(position, (name, id, n))| CameraEntry {
            position,
            name: (*name).into(),
            id: (*id).into(),
            backend_index: *n,
        })
        .collect()
    }
    #[test]
    fn selectors_parse_auto_numbers_paths_and_names() {
        assert_eq!(parse("auto"), Selector::Auto);
        assert_eq!(parse(" AUTO "), Selector::Auto);
        assert_eq!(parse(""), Selector::Auto);
        assert_eq!(parse("1"), Selector::Index(1));
        assert_eq!(parse("/dev/video2"), Selector::Path("/dev/video2".into()));
        assert_eq!(parse("C920"), Selector::Name("C920".into()));
    }
    #[test]
    fn numbers_are_positions_among_capture_cameras_not_dev_nodes() {
        // /dev/video1 is the metadata node of the first camera and is not
        // listed, so "1" is the second real camera at /dev/video2.
        let c = candidates(&Selector::Index(1), &cams()).unwrap();
        assert_eq!(c[0].backend_index, 2);
        let auto = candidates(&Selector::Auto, &cams()).unwrap();
        assert_eq!(auto.len(), 2);
        assert_eq!(auto[0].id, "/dev/video0");
        let error = candidates(&Selector::Index(2), &cams()).unwrap_err();
        assert!(error.to_string().contains("2 camera(s)"));
        assert!(candidates(&Selector::Auto, &[]).is_err());
    }
    #[test]
    fn names_and_paths_select_specific_cameras() {
        let c = candidates(&Selector::Name("c920".into()), &cams()).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].position, 1);
        let p = candidates(&Selector::Path("/dev/video2".into()), &cams()).unwrap();
        assert_eq!(p[0].position, 1);
        assert!(candidates(&Selector::Path("/dev/video1".into()), &cams()).is_err());
        assert!(candidates(&Selector::Name("Logitech Brio".into()), &cams()).is_err());
    }
    #[cfg(feature = "camera")]
    #[test]
    fn mode_ranking_prefers_exact_then_larger_then_faster() {
        use nokhwa::utils::{CameraFormat, FrameFormat, Resolution};
        let f = |w, h, fmt, fps| CameraFormat::new(Resolution::new(w, h), fmt, fps);
        let modes = [
            f(1280, 720, FrameFormat::MJPEG, 30),
            f(640, 480, FrameFormat::MJPEG, 30),
            f(640, 480, FrameFormat::YUYV, 30),
            f(320, 240, FrameFormat::YUYV, 30),
        ];
        let ranked = native::rank(&modes, 640, 480, 30);
        assert_eq!(ranked[0], f(640, 480, FrameFormat::YUYV, 30));
        assert_eq!(ranked[1], f(640, 480, FrameFormat::MJPEG, 30));
        let ranked = native::rank(&modes, 800, 600, 60);
        assert_eq!(ranked[0].resolution(), Resolution::new(1280, 720));
    }
}
