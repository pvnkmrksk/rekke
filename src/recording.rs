//! Lossless 8-bit grayscale AVI, with bounded asynchronous disk I/O. No encoder.
use crate::{
    config::Config,
    frame::{GrayFrame, unix_ns},
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Default, Debug)]
pub struct RecordingState {
    pub active: bool,
    pub accepted: u64,
    pub written: u64,
    pub directory: Option<PathBuf>,
    pub error: Option<String>,
}
pub struct Recorder {
    sender: Option<SyncSender<Arc<GrayFrame>>>,
    thread: Option<JoinHandle<()>>,
    shared: Arc<Mutex<RecordingState>>,
}
impl Recorder {
    pub fn start(config: &Config, config_path: &Path) -> Result<Self> {
        config.validate()?;
        let root = config.resolve(config_path, &config.recording.directory);
        let safe_name: String = config
            .instance
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let directory = root.join(format!(
            "{}-{}-{}",
            safe_name,
            unix_ns(),
            std::process::id()
        ));
        fs::create_dir_all(&directory).context("Create video recording directory")?;
        config.save(&directory.join("config.toml"))?;
        let background = config.resolve(config_path, &config.tracking.background_path);
        if background.is_file() {
            fs::copy(background, directory.join("background.png"))?;
        }
        let (width, height, fps) = config.source.dimensions();
        let limit = config.recording.segment_mb as u64 * 1024 * 1024;
        let mut writer = VideoWriter::new(directory.clone(), width, height, fps, limit)?;
        let (sender, receiver) =
            mpsc::sync_channel::<Arc<GrayFrame>>(config.recording.queue_frames);
        let shared = Arc::new(Mutex::new(RecordingState {
            active: true,
            directory: Some(directory),
            ..Default::default()
        }));
        let progress = shared.clone();
        let thread = thread::Builder::new()
            .name(format!("video-{}", config.instance))
            .spawn(move || {
                let result: Result<()> = (|| {
                    for frame in receiver {
                        writer.write(&frame)?;
                        progress.lock().unwrap().written += 1;
                    }
                    writer.finish()
                })();
                let mut state = progress.lock().unwrap();
                state.active = false;
                if let Err(error) = result {
                    state.error = Some(format!("Video recording: {error:#}"));
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
            shared,
        })
    }
    pub fn state(&self) -> RecordingState {
        self.shared.lock().unwrap().clone()
    }
    pub fn accepting(&self) -> bool {
        self.sender.is_some()
    }
    pub fn request_stop(&mut self) {
        self.sender.take();
        self.shared.lock().unwrap().active = false;
    }
    pub fn is_finished(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
    pub fn submit(&mut self, frame: Arc<GrayFrame>) -> Result<()> {
        if let Some(error) = self.state().error {
            bail!(error);
        }
        match self
            .sender
            .as_ref()
            .context("Recording is stopped")?
            .try_send(frame)
        {
            Ok(()) => {
                self.shared.lock().unwrap().accepted += 1;
                Ok(())
            }
            Err(error) => {
                let message = match error {
                    TrySendError::Full(_) => {
                        "Video queue is full: disk cannot keep up. Recording stopped; tracking continues. Reduce FPS/resolution or use faster storage."
                    }
                    TrySendError::Disconnected(_) => {
                        "Video writer stopped; inspect the recording error."
                    }
                };
                self.shared.lock().unwrap().error = Some(message.into());
                self.sender.take();
                bail!(message);
            }
        }
    }
    pub fn finish(&mut self) -> Result<()> {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("Video writer panicked"))?;
        }
        if let Some(error) = self.state().error {
            bail!(error);
        }
        Ok(())
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[derive(Serialize)]
struct FrameStamp {
    segment: u32,
    frame_in_segment: u32,
    sequence: u64,
    timestamp_ns: u64,
}
struct VideoWriter {
    directory: PathBuf,
    width: u32,
    height: u32,
    fps: u32,
    limit: u64,
    segment: u32,
    avi: AviWriter,
    timestamps: BufWriter<File>,
}
impl VideoWriter {
    fn new(directory: PathBuf, width: u32, height: u32, fps: u32, limit: u64) -> Result<Self> {
        let avi = AviWriter::new(&directory.join("video-0000.avi"), width, height, fps)?;
        let timestamps = BufWriter::new(create(&directory.join("frames.jsonl"))?);
        Ok(Self {
            directory,
            width,
            height,
            fps,
            limit,
            segment: 0,
            avi,
            timestamps,
        })
    }
    fn write(&mut self, frame: &GrayFrame) -> Result<()> {
        if self.avi.frames() > 0 && self.avi.estimated_size_after_frame() > self.limit {
            self.avi.finish()?;
            self.segment += 1;
            self.avi = AviWriter::new(
                &self
                    .directory
                    .join(format!("video-{:04}.avi", self.segment)),
                self.width,
                self.height,
                self.fps,
            )?;
        }
        let stamp = FrameStamp {
            segment: self.segment,
            frame_in_segment: self.avi.frames(),
            sequence: frame.sequence,
            timestamp_ns: frame.timestamp_ns,
        };
        self.avi.write(frame)?;
        serde_json::to_writer(&mut self.timestamps, &stamp)?;
        self.timestamps.write_all(b"\n")?;
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        self.avi.finish()?;
        self.timestamps.flush()?;
        Ok(())
    }
}
impl Drop for VideoWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
fn create(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("Create {}", path.display()))
}
fn u32le(out: &mut impl Write, value: u32) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}
fn u16le(out: &mut impl Write, value: u16) -> Result<()> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}
fn start_chunk(out: &mut (impl Write + Seek), tag: &[u8; 4]) -> Result<u64> {
    out.write_all(tag)?;
    let p = out.stream_position()?;
    u32le(out, 0)?;
    Ok(p)
}
fn patch(out: &mut (impl Write + Seek), position: u64, value: u32) -> Result<()> {
    let end = out.stream_position()?;
    out.seek(SeekFrom::Start(position))?;
    u32le(out, value)?;
    out.seek(SeekFrom::Start(end))?;
    Ok(())
}
fn end_chunk(out: &mut (impl Write + Seek), position: u64) -> Result<()> {
    let size = out.stream_position()? - position - 4;
    ensure!(size <= u32::MAX as u64, "AVI chunk exceeds 4 GiB");
    patch(out, position, size as u32)
}

/// RIFF AVI with Y800 grayscale pixels and idx1. Segmenting stays below
/// the classic AVI 2 GiB limit. Header/index work happens only when finishing.
struct AviWriter {
    out: BufWriter<File>,
    width: u32,
    height: u32,
    image_bytes: u32,
    total_frames: u64,
    stream_frames: u64,
    movi_size: u64,
    movi_base: u64,
    position: u64,
    index: Vec<(u32, u32)>,
    finished: bool,
}
impl AviWriter {
    fn new(path: &Path, width: u32, height: u32, fps: u32) -> Result<Self> {
        let image_bytes = width * height;
        let mut out = BufWriter::with_capacity(1024 * 1024, create(path)?);
        out.write_all(b"RIFF")?;
        u32le(&mut out, 0)?;
        out.write_all(b"AVI ")?;
        let hdrl = start_chunk(&mut out, b"LIST")?;
        out.write_all(b"hdrl")?;
        out.write_all(b"avih")?;
        u32le(&mut out, 56)?;
        u32le(&mut out, (1_000_000 / fps).max(1))?;
        u32le(&mut out, image_bytes.saturating_mul(fps))?;
        u32le(&mut out, 0)?;
        u32le(&mut out, 0x10)?;
        let total_frames = out.stream_position()?;
        u32le(&mut out, 0)?;
        u32le(&mut out, 0)?;
        u32le(&mut out, 1)?;
        u32le(&mut out, image_bytes)?;
        u32le(&mut out, width)?;
        u32le(&mut out, height)?;
        for _ in 0..4 {
            u32le(&mut out, 0)?;
        }
        let strl = start_chunk(&mut out, b"LIST")?;
        out.write_all(b"strl")?;
        out.write_all(b"strh")?;
        u32le(&mut out, 56)?;
        out.write_all(b"vidsY800")?;
        u32le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u32le(&mut out, 0)?;
        u32le(&mut out, 1)?;
        u32le(&mut out, fps)?;
        u32le(&mut out, 0)?;
        let stream_frames = out.stream_position()?;
        u32le(&mut out, 0)?;
        u32le(&mut out, image_bytes)?;
        u32le(&mut out, u32::MAX)?;
        u32le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u16le(&mut out, 0)?;
        u16le(&mut out, width as u16)?;
        u16le(&mut out, height as u16)?;
        out.write_all(b"strf")?;
        u32le(&mut out, 40)?;
        u32le(&mut out, 40)?;
        u32le(&mut out, width)?;
        u32le(&mut out, height)?;
        u16le(&mut out, 1)?;
        u16le(&mut out, 8)?;
        u32le(&mut out, u32::from_le_bytes(*b"Y800"))?;
        u32le(&mut out, image_bytes)?;
        for value in [0, 0, 0, 0] {
            u32le(&mut out, value)?;
        }
        end_chunk(&mut out, strl)?;
        end_chunk(&mut out, hdrl)?;
        let movi_size = start_chunk(&mut out, b"LIST")?;
        let movi_base = out.stream_position()?;
        out.write_all(b"movi")?;
        let position = out.stream_position()?;
        Ok(Self {
            out,
            width,
            height,
            image_bytes,
            total_frames,
            stream_frames,
            movi_size,
            movi_base,
            position,
            index: Vec::new(),
            finished: false,
        })
    }
    fn frames(&self) -> u32 {
        self.index.len() as u32
    }
    fn estimated_size_after_frame(&self) -> u64 {
        self.position
            + self.image_bytes as u64
            + (self.image_bytes % 2) as u64
            + 8
            + 8
            + (self.index.len() as u64 + 1) * 16
    }
    fn write(&mut self, frame: &GrayFrame) -> Result<()> {
        ensure!(
            !self.finished && frame.width == self.width && frame.height == self.height,
            "Video frame size changed"
        );
        let offset = (self.position - self.movi_base) as u32;
        self.out.write_all(b"00dc")?;
        u32le(&mut self.out, self.image_bytes)?;
        // Y800 is packed top-down luma. One contiguous write avoids palette
        // conversion/rounding on replay and per-row copies in the writer.
        self.out.write_all(&frame.pixels)?;
        if self.image_bytes % 2 == 1 {
            self.out.write_all(&[0])?;
        }
        self.index.push((offset, self.image_bytes));
        self.position += 8 + self.image_bytes as u64 + (self.image_bytes % 2) as u64;
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        if self.finished {
            return Ok(());
        }
        let frames = self.frames();
        end_chunk(&mut self.out, self.movi_size)?;
        self.out.write_all(b"idx1")?;
        u32le(&mut self.out, frames * 16)?;
        for &(offset, size) in &self.index {
            self.out.write_all(b"00dc")?;
            u32le(&mut self.out, 0x10)?;
            u32le(&mut self.out, offset)?;
            u32le(&mut self.out, size)?;
        }
        patch(&mut self.out, self.total_frames, frames)?;
        patch(&mut self.out, self.stream_frames, frames)?;
        end_chunk(&mut self.out, 4)?;
        self.out.flush()?;
        self.finished = true;
        Ok(())
    }
}
impl Drop for AviWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn avi_preserves_exact_grayscale_pixels_odd_chunk_padding_and_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.avi");
        let pixels: Vec<u8> = (0..33 * 33).map(|i| (i % 251) as u8).collect();
        let frame = GrayFrame::new(33, 33, pixels.clone(), 42, 7).unwrap();
        let mut writer = AviWriter::new(&path, 33, 33, 120).unwrap();
        writer.write(&frame).unwrap();
        writer.write(&frame).unwrap();
        writer.finish().unwrap();
        let bytes = fs::read(path).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8,
            bytes.len()
        );
        let movi = bytes.windows(4).position(|b| b == b"movi").unwrap();
        assert_eq!(&bytes[movi + 4..movi + 8], b"00dc");
        assert_eq!(&bytes[movi + 12..movi + 12 + pixels.len()], &pixels);
        assert_eq!(bytes[movi + 12 + pixels.len()], 0);
        let second = movi + 12 + pixels.len() + 1;
        assert_eq!(&bytes[second..second + 4], b"00dc");
        assert_eq!(&bytes[second + 8..second + 8 + pixels.len()], &pixels);
        let strh = bytes.windows(4).position(|b| b == b"strh").unwrap();
        assert_eq!(&bytes[strh + 8..strh + 16], b"vidsY800");
        let idx = bytes.windows(4).position(|b| b == b"idx1").unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[idx + 16..idx + 20].try_into().unwrap()),
            4
        );
        assert_eq!(u32::from_le_bytes(bytes[48..52].try_into().unwrap()), 2);
    }
    #[test]
    fn recorder_drains_every_accepted_frame_and_splits_segments() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::demo();
        config.recording.directory = "video".into();
        config.recording.segment_mb = 16;
        config.recording.queue_frames = 128;
        let mut recorder = Recorder::start(&config, &dir.path().join("rig.toml")).unwrap();
        for sequence in 0..60 {
            recorder
                .submit(Arc::new(
                    GrayFrame::new(
                        640,
                        480,
                        vec![sequence as u8; 640 * 480],
                        42 + sequence,
                        sequence,
                    )
                    .unwrap(),
                ))
                .unwrap();
        }
        recorder.finish().unwrap();
        let state = recorder.state();
        assert_eq!(state.accepted, 60);
        assert_eq!(state.written, 60);
        assert!(!state.active);
        assert!(state.error.is_none());
        let directory = state.directory.unwrap();
        assert!(directory.join("video-0000.avi").is_file());
        assert!(directory.join("video-0001.avi").is_file());
        let stamps = fs::read_to_string(directory.join("frames.jsonl")).unwrap();
        assert_eq!(stamps.lines().count(), 60);
        for (sequence, row) in stamps.lines().enumerate() {
            let value: serde_json::Value = serde_json::from_str(row).unwrap();
            assert_eq!(value["sequence"], sequence as u64);
        }
    }
    #[test]
    fn full_recording_queue_is_an_explicit_failure_without_unbounded_memory() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let mut recorder = Recorder {
            sender: Some(sender),
            thread: None,
            shared: Arc::new(Mutex::new(RecordingState {
                active: true,
                ..Default::default()
            })),
        };
        let frame = Arc::new(GrayFrame::new(32, 32, vec![0; 1024], 1, 1).unwrap());
        recorder.submit(frame.clone()).unwrap();
        assert!(
            recorder
                .submit(frame)
                .unwrap_err()
                .to_string()
                .contains("queue is full")
        );
        assert_eq!(recorder.state().accepted, 1);
        assert!(recorder.state().error.is_some());
    }
}
