//! Video recording on a dedicated bounded writer. MJPEG (default), lossless
//! PNG or raw Y800 frames in AVI segments, plus exact per-frame timestamps.
use crate::{
    avi::{self, AviWriter, VideoFormat},
    config::Config,
    frame::{GrayFrame, unix_ns},
};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
};

#[derive(Clone, Default, Debug)]
pub struct RecordingState {
    pub active: bool,
    pub accepted: u64,
    pub written: u64,
    /// Bytes written to video segments so far.
    pub bytes: u64,
    pub directory: Option<PathBuf>,
    pub error: Option<String>,
}
pub struct Recorder {
    sender: Option<SyncSender<Arc<GrayFrame>>>,
    thread: Option<JoinHandle<()>>,
    shared: Arc<Mutex<RecordingState>>,
}
/// A filesystem-safe version of an instance label.
pub fn safe_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if safe.is_empty() { "rig".into() } else { safe }
}
impl Recorder {
    pub fn start(config: &Config, config_path: &Path) -> Result<Self> {
        config.validate()?;
        let root = config.resolve(config_path, &config.recording.directory);
        let directory = root.join(format!(
            "{}-{}-{}",
            safe_name(&config.instance),
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
        let writer = VideoWriter::new(
            directory.clone(),
            width,
            height,
            fps,
            config.recording.segment_mb as u64 * 1024 * 1024,
            config.recording.format,
            config.recording.quality,
        )?;
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
                let result = write_frames(writer, receiver, &progress);
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
                        "Video queue is full: the disk or encoder cannot keep up. Recording stopped; tracking continues. Use format = \"mjpeg\", lower FPS/resolution or faster storage."
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

/// Encoders run in parallel only when frames queue up; an idle or light
/// load encodes on the writer thread. Order on disk always matches arrival.
fn write_frames(
    mut writer: VideoWriter,
    receiver: Receiver<Arc<GrayFrame>>,
    progress: &Mutex<RecordingState>,
) -> Result<()> {
    let threads = if writer.format == VideoFormat::Raw {
        1
    } else {
        thread::available_parallelism()
            .map(|n| (n.get() / 2).clamp(1, 6))
            .unwrap_or(2)
    };
    let mut buffers: Vec<Vec<u8>> = Vec::new();
    let mut batch: Vec<Arc<GrayFrame>> = Vec::new();
    while let Ok(first) = receiver.recv() {
        batch.clear();
        batch.push(first);
        while batch.len() < threads * 2
            && let Ok(next) = receiver.try_recv()
        {
            batch.push(next);
        }
        if writer.format == VideoFormat::Raw {
            for frame in &batch {
                writer.write(frame, &frame.pixels)?;
            }
        } else {
            buffers.resize_with(batch.len(), Vec::new);
            let (format, quality) = (writer.format, writer.quality);
            let encode = |frame: &GrayFrame, out: &mut Vec<u8>| {
                // Live recording favours encoding speed over the last few percent.
                avi::encode(
                    format,
                    quality,
                    false,
                    frame.width,
                    frame.height,
                    &frame.pixels,
                    out,
                )
            };
            if batch.len() == 1 || threads == 1 {
                for (frame, out) in batch.iter().zip(&mut buffers) {
                    encode(frame, out)?;
                }
            } else {
                let per = batch.len().div_ceil(threads);
                thread::scope(|scope| -> Result<()> {
                    let workers: Vec<_> = batch
                        .chunks(per)
                        .zip(buffers.chunks_mut(per))
                        .map(|(frames, outs)| {
                            scope.spawn(move || -> Result<()> {
                                for (frame, out) in frames.iter().zip(outs) {
                                    encode(frame, out)?;
                                }
                                Ok(())
                            })
                        })
                        .collect();
                    for worker in workers {
                        worker
                            .join()
                            .map_err(|_| anyhow::anyhow!("Video encoder panicked"))??;
                    }
                    Ok(())
                })?;
            }
            for (frame, data) in batch.iter().zip(&buffers) {
                writer.write(frame, data)?;
            }
        }
        let mut state = progress.lock().unwrap();
        state.written += batch.len() as u64;
        state.bytes = writer.bytes();
    }
    writer.finish()
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
    format: VideoFormat,
    quality: u8,
    completed_bytes: u64,
    avi: AviWriter,
    timestamps: BufWriter<File>,
}
impl VideoWriter {
    fn new(
        directory: PathBuf,
        width: u32,
        height: u32,
        fps: u32,
        limit: u64,
        format: VideoFormat,
        quality: u8,
    ) -> Result<Self> {
        let avi = AviWriter::create(
            &directory.join("video-0000.avi"),
            width,
            height,
            fps,
            1,
            format,
        )?;
        let timestamps = BufWriter::new(avi::create_new(&directory.join("frames.jsonl"))?);
        Ok(Self {
            directory,
            width,
            height,
            fps,
            limit,
            segment: 0,
            format,
            quality,
            completed_bytes: 0,
            avi,
            timestamps,
        })
    }
    fn bytes(&self) -> u64 {
        self.completed_bytes + self.avi.size_after(0)
    }
    fn write(&mut self, frame: &GrayFrame, data: &[u8]) -> Result<()> {
        anyhow::ensure!(
            frame.width == self.width && frame.height == self.height,
            "Video frame size changed"
        );
        if self.avi.frames() > 0 && self.avi.size_after(data.len()) > self.limit {
            self.completed_bytes += self.avi.size_after(0);
            self.avi.finish()?;
            self.segment += 1;
            self.avi = AviWriter::create(
                &self
                    .directory
                    .join(format!("video-{:04}.avi", self.segment)),
                self.width,
                self.height,
                self.fps,
                1,
                self.format,
            )?;
        }
        let stamp = FrameStamp {
            segment: self.segment,
            frame_in_segment: self.avi.frames(),
            sequence: frame.sequence,
            timestamp_ns: frame.timestamp_ns,
        };
        self.avi.write(data)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::avi::AviReader;
    fn record(format: VideoFormat, segment_mb: u32, frames: u64) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().to_path_buf();
        let mut config = Config::demo();
        config.recording.directory = "video".into();
        config.recording.segment_mb = segment_mb;
        config.recording.queue_frames = 128;
        config.recording.format = format;
        let mut recorder = Recorder::start(&config, &dir.join("rig.toml")).unwrap();
        for sequence in 0..frames {
            let pixels = (0..640 * 480)
                .map(|i| ((i % 640) / 3 + sequence as usize) as u8)
                .collect();
            recorder
                .submit(Arc::new(
                    GrayFrame::new(640, 480, pixels, 42 + sequence, sequence).unwrap(),
                ))
                .unwrap();
        }
        recorder.finish().unwrap();
        let state = recorder.state();
        assert_eq!(state.accepted, frames);
        assert_eq!(state.written, frames);
        assert!(!state.active);
        assert!(state.error.is_none());
        assert!(state.bytes > 0);
        let directory = state.directory.unwrap();
        let stamps = fs::read_to_string(directory.join("frames.jsonl")).unwrap();
        assert_eq!(stamps.lines().count() as u64, frames);
        for (sequence, row) in stamps.lines().enumerate() {
            let value: serde_json::Value = serde_json::from_str(row).unwrap();
            assert_eq!(value["sequence"], sequence as u64);
        }
        (temp, directory)
    }
    #[test]
    fn raw_recorder_drains_every_accepted_frame_and_splits_segments() {
        let (_temp, directory) = record(VideoFormat::Raw, 16, 60);
        assert!(directory.join("video-0000.avi").is_file());
        assert!(directory.join("video-0001.avi").is_file());
        let mut reader = AviReader::open(&directory.join("video-0000.avi")).unwrap();
        let (mut c, mut p) = (Vec::new(), Vec::new());
        assert!(reader.next_frame(&mut c, &mut p).unwrap());
        assert_eq!(p[3], 1);
    }
    #[test]
    fn mjpeg_recording_is_much_smaller_ordered_and_replayable() {
        let (_temp, directory) = record(VideoFormat::Mjpeg, 1024, 40);
        let file = directory.join("video-0000.avi");
        let size = fs::metadata(&file).unwrap().len();
        assert!(size < 40 * 640 * 480 / 8, "MJPEG {size} bytes");
        let mut reader = AviReader::open(&file).unwrap();
        assert_eq!(reader.info().frames, 40);
        let (mut c, mut p) = (Vec::new(), Vec::new());
        for sequence in 0..40usize {
            assert!(reader.next_frame(&mut c, &mut p).unwrap());
            // Ordered: the per-frame offset encodes the sequence number.
            let expected = (300 / 3 + sequence) as i32;
            assert!((p[300] as i32 - expected).abs() <= 3, "frame {sequence}");
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
