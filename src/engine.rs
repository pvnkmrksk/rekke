use crate::{
    config::{Config, SourceConfig},
    frame::GrayFrame,
    output::Publisher,
    recording::{Recorder, RecordingState},
    source::{self, AviSource, ChildControl, DemoSource, Opened},
    tracker::{Measurement, Tracker},
};
use anyhow::{Context, Result, ensure};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Snapshot {
    pub frame: Arc<GrayFrame>,
    pub foreground: Option<Arc<GrayFrame>>,
    pub measurement: Measurement,
}
#[derive(Clone, Default)]
pub struct State {
    pub latest: Option<Arc<Snapshot>>,
    pub message: String,
    pub error: Option<String>,
    pub finished: bool,
    pub background_loaded: bool,
    pub frames: u64,
    pub udp_dropped: u64,
    pub zmq_dropped: u64,
    pub recording: RecordingState,
    /// What is being acquired, for example the camera name and mode.
    pub source: String,
    /// Why the configured camera was replaced by the sample video.
    pub fallback: Option<String>,
    /// The configuration tracking the sample: the run's own, with the
    /// sample's markers and sectors (the Bogong moth example's, for the
    /// bundled clip). The GUI shows these and never saves them.
    pub example: Option<Config>,
    /// Outputs are off because they could not start (GUI only).
    pub output_error: Option<String>,
}
/// How an engine reacts to an unavailable camera or output port.
#[derive(Clone, Debug, Default)]
pub struct StartOptions {
    /// Stop after this many processed frames.
    pub limit: Option<u64>,
    /// If the camera cannot open, play the bundled sample video instead
    /// (or the simulated fly). Interactive use only; never for experiments.
    pub fallback: bool,
    /// If an output port is busy, keep tracking with outputs off and report
    /// it, instead of failing. Interactive use only.
    pub degrade_output: bool,
    /// A video chosen for this run brings its own markers and sectors (a
    /// `.toml` beside it, or the Bogong moth example's) instead of using the
    /// config's, which describe a different insect.
    pub video_markers: bool,
}
enum Command {
    Update(Box<Config>),
    Subscribe(mpsc::SyncSender<Measurement>),
    SaveBackground,
    LoadBackground,
    DemoBackground,
    Record(bool),
}
pub struct Engine {
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    sender: mpsc::Sender<Command>,
    child: ChildControl,
    thread: Option<JoinHandle<()>>,
    initial: Config,
}
impl Engine {
    pub fn start(config: Config, path: PathBuf, limit: Option<u64>) -> Result<Self> {
        Self::start_with(
            config,
            path,
            StartOptions {
                limit,
                ..Default::default()
            },
        )
    }
    pub fn start_with(config: Config, path: PathBuf, options: StartOptions) -> Result<Self> {
        config.validate()?;
        let mut config = config;
        let mut example = None;
        if options.video_markers
            && let SourceConfig::Video { path: video, .. } = &config.source
            && let Some(own) = source::sample_config(&config.resolve(&path, video))
                .and_then(|e| config.with_geometry_of(&e))
        {
            config = own.clone();
            example = Some(own);
        }
        let limit = options.limit;
        let mut output_error = None;
        let publisher = match Publisher::new(&config, &path) {
            Ok(p) => p,
            Err(error) if options.degrade_output => {
                // Keep every output that still works; turn off only the failing one(s).
                type Disable = fn(&mut Config);
                let attempts: [(&str, Disable); 4] = [
                    ("ZMQ", |c| c.output.zmq_enabled = false),
                    ("UDP", |c| c.output.enabled = false),
                    ("ZMQ and UDP", |c| {
                        c.output.zmq_enabled = false;
                        c.output.enabled = false;
                    }),
                    ("all outputs", |c| {
                        c.output.zmq_enabled = false;
                        c.output.enabled = false;
                        c.output.jsonl_path.clear();
                    }),
                ];
                let mut fallback = None;
                for (name, disable) in attempts {
                    let mut quiet = config.clone();
                    disable(&mut quiet);
                    if quiet != config
                        && let Ok(p) = Publisher::new(&quiet, &path)
                    {
                        fallback = Some((name, p));
                        break;
                    }
                }
                let Some((name, publisher)) = fallback else {
                    return Err(error);
                };
                output_error = Some(format!("{name} off: {error:#}"));
                publisher
            }
            Err(error) => return Err(error),
        };
        let bg_path = config.resolve(&path, &config.tracking.background_path);
        let background = if bg_path.is_file() {
            Some(GrayFrame::load(&bg_path)?)
        } else {
            None
        };
        ensure!(
            !config.tracking.subtract_background || background.is_some(),
            "No saved background at {}. Capture one in the GUI before enabling subtraction.",
            bg_path.display()
        );
        if let Some(bg) = &background {
            let (w, h, _) = config.source.dimensions();
            ensure!(
                bg.width == w && bg.height == h,
                "Saved background dimensions do not match configured camera"
            );
        }
        let state = Arc::new(Mutex::new(State {
            message: "Opening image source…".into(),
            background_loaded: background.is_some(),
            output_error,
            example,
            ..Default::default()
        }));
        let fallback = options.fallback;
        let stop = Arc::new(AtomicBool::new(false));
        let child: ChildControl = Arc::new(Mutex::new(None));
        let (sender, receiver) = mpsc::channel();
        let shared = state.clone();
        let flag = stop.clone();
        let process = child.clone();
        let initial = config.clone();
        let thread = thread::Builder::new()
            .name(format!("capture-{}", config.instance))
            .spawn(move || {
                let result = run(
                    config,
                    path,
                    publisher,
                    background,
                    receiver,
                    shared.clone(),
                    flag,
                    process,
                    limit,
                    fallback,
                );
                let mut shared = shared.lock().unwrap();
                shared.finished = true;
                if let Err(e) = result {
                    shared.error = Some(format!("{e:#}"));
                    shared.message = "Acquisition stopped".into();
                } else if shared.message != "End of video" {
                    shared.message = "Acquisition stopped".into();
                }
            })?;
        Ok(Self {
            state,
            stop,
            sender,
            child,
            thread: Some(thread),
            initial,
        })
    }
    pub fn state(&self) -> State {
        self.state.lock().unwrap().clone()
    }
    pub fn update(&self, config: &Config) -> Result<()> {
        config.validate()?;
        ensure!(
            config.source == self.initial.source
                && config.output == self.initial.output
                && config.instance == self.initial.instance,
            "Restart acquisition to apply source, instance, or output changes"
        );
        ensure!(
            config.tracking.background_path == self.initial.tracking.background_path,
            "Restart acquisition to apply the background path"
        );
        self.sender
            .send(Command::Update(Box::new(config.clone())))
            .context("Acquisition has stopped")
    }
    /// Every measurement from now on, in order, for automation that needs
    /// all frames rather than the preview. Tracking never waits for the
    /// receiver: if `capacity` measurements queue up, newer ones are
    /// dropped (visible as gaps in `sequence`).
    pub fn subscribe(&self, capacity: usize) -> Result<mpsc::Receiver<Measurement>> {
        let (sender, receiver) = mpsc::sync_channel(capacity.max(1));
        self.sender
            .send(Command::Subscribe(sender))
            .context("Acquisition has stopped")?;
        Ok(receiver)
    }
    pub fn save_background(&self) -> Result<()> {
        self.sender
            .send(Command::SaveBackground)
            .context("Acquisition has stopped")
    }
    pub fn record(&self, enabled: bool) -> Result<()> {
        self.sender
            .send(Command::Record(enabled))
            .context("Acquisition has stopped")
    }
    pub fn load_background(&self) -> Result<()> {
        self.sender
            .send(Command::LoadBackground)
            .context("Acquisition has stopped")
    }
    pub fn demo_background(&self) -> Result<()> {
        self.sender
            .send(Command::DemoBackground)
            .context("Acquisition has stopped")
    }
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(child) = self.child.lock().unwrap().as_mut() {
            let _ = child.kill();
        }
    }
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
    pub fn wait(mut self) {
        self.stop();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    mut config: Config,
    path: PathBuf,
    mut publisher: Publisher,
    mut background: Option<GrayFrame>,
    commands: mpsc::Receiver<Command>,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    child: ChildControl,
    limit: Option<u64>,
    fallback: bool,
) -> Result<()> {
    let (w, h, _) = config.source.dimensions();
    let opened = match source::open(&config, &path, child) {
        Ok(opened) => opened,
        Err(error) if fallback && matches!(config.source, SourceConfig::Camera { .. }) => {
            let reason = format!("{error:#}");
            eprintln!(
                "{}: camera unavailable, showing a sample: {reason}",
                config.instance
            );
            let mut example = None;
            let opened = source::sample_video()
                .and_then(|video| {
                    let s = AviSource::open(&video, w, h, true, true).ok()?;
                    example =
                        source::sample_config(&video).and_then(|e| config.with_geometry_of(&e));
                    let name = video.file_name().unwrap_or_default().to_string_lossy();
                    Some(Opened {
                        source: Box::new(s),
                        description: if name == source::EXAMPLE {
                            "Sample video: Bogong moth example".into()
                        } else {
                            format!("Sample video {name}")
                        },
                    })
                })
                .unwrap_or_else(|| Opened {
                    source: Box::new(DemoSource::new(w, h, 60, 0.8)),
                    description: "Simulated fly".into(),
                });
            if let Some(example) = &example {
                config = example.clone();
            }
            let mut s = state.lock().unwrap();
            s.fallback = Some(reason);
            s.example = example;
            opened
        }
        Err(error) => return Err(error),
    };
    let mut source = opened.source;
    let sample = state.lock().unwrap().fallback.is_some();
    state.lock().unwrap().source = opened.description;
    let mut tracker = Tracker::new(&config, w, h);
    let mut recorder = if config.recording.enabled && !sample {
        Some(Recorder::start(&config, &path)?)
    } else {
        None
    };
    let mut draining: Vec<Recorder> = Vec::new();
    let mut prior_recording_error: Option<String> = None;
    let start = Instant::now();
    let mut last = Instant::now();
    let mut fps = 0.0;
    let mut preview_at = Instant::now() - Duration::from_secs(1);
    let mut frames = 0;
    let mut dropped = 0;
    let mut capture_bg = false;
    let mut subscribers: Vec<mpsc::SyncSender<Measurement>> = Vec::new();
    {
        let mut s = state.lock().unwrap();
        s.message = if s.fallback.is_some() {
            "No camera: showing a sample".into()
        } else {
            "Live".into()
        };
    }
    let processing_result: Result<()> = (|| {
        while !stop.load(Ordering::Relaxed) {
            for command in commands.try_iter() {
                let command = match command {
                    Command::Subscribe(sender) => {
                        subscribers.push(sender);
                        continue;
                    }
                    other => other,
                };
                let result: Result<()> = match command {
                    Command::Subscribe(_) => unreachable!("handled above"),
                    Command::Update(next) => {
                        if next.tracking.subtract_background && background.is_none() {
                            Err(anyhow::anyhow!(
                                "Capture or load a background before enabling subtraction"
                            ))
                        } else {
                            config = *next;
                            tracker.update(&config);
                            Ok(())
                        }
                    }
                    Command::SaveBackground if sample => Err(anyhow::anyhow!(
                        "Background capture is off while the sample plays; connect the camera and Restart"
                    )),
                    Command::SaveBackground => {
                        capture_bg = true;
                        Ok(())
                    }
                    Command::Record(true) if sample => Err(anyhow::anyhow!(
                        "Recording is off while the sample plays; connect the camera and Restart"
                    )),
                    Command::Record(enabled) => {
                        if enabled && recorder.as_ref().is_none_or(|r| !r.accepting()) {
                            if let Some(old) = recorder.take() {
                                draining.push(old);
                            }
                            Recorder::start(&config, &path).map(|r| {
                                recorder = Some(r);
                            })
                        } else {
                            if !enabled && let Some(r) = &mut recorder {
                                r.request_stop();
                            }
                            Ok(())
                        }
                    }
                    Command::LoadBackground => {
                        let p = config.resolve(&path, &config.tracking.background_path);
                        GrayFrame::load(&p).and_then(|bg| {
                            ensure!(
                                bg.width == w && bg.height == h,
                                "Background dimensions must match camera"
                            );
                            background = Some(bg);
                            Ok(())
                        })
                    }
                    Command::DemoBackground => {
                        if let crate::config::SourceConfig::Demo {
                            width,
                            height,
                            fps,
                            beat_hz,
                        } = &config.source
                        {
                            let bg = DemoSource::new(*width, *height, *fps, *beat_hz).blank_frame();
                            let result =
                                bg.save(&config.resolve(&path, &config.tracking.background_path));
                            if result.is_ok() {
                                background = Some(bg);
                            }
                            result
                        } else {
                            Err(anyhow::anyhow!(
                                "Clean simulated background is available only for the demo source"
                            ))
                        }
                    }
                };
                let mut s = state.lock().unwrap();
                s.background_loaded = background.is_some();
                match result {
                    Ok(()) => {
                        if s.fallback.is_none() {
                            s.message = "Live".into();
                        }
                        s.error = None;
                    }
                    Err(e) => s.message = format!("{e:#}"),
                }
            }
            let Some(raw) = source.next_frame(&stop)? else {
                if !stop.load(Ordering::Relaxed) {
                    state.lock().unwrap().message = "End of video".into();
                }
                break;
            };
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if capture_bg {
                match raw.save(&config.resolve(&path, &config.tracking.background_path)) {
                    Ok(()) => {
                        background = Some(raw.clone());
                        let mut s = state.lock().unwrap();
                        s.background_loaded = true;
                        s.message =
                            "Background saved; enable subtraction when the insect is in position"
                                .into();
                    }
                    Err(e) => state.lock().unwrap().message = format!("Save background: {e:#}"),
                }
                capture_bg = false;
            }
            let raw = Arc::new(raw);
            if let Some(r) = &mut recorder
                && r.accepting()
                && let Err(error) = r.submit(raw.clone())
            {
                eprintln!("{}: video recording stopped: {error:#}", config.instance);
                state.lock().unwrap().message = format!("{error:#}");
                // Closing the queue drains/finalizes on the writer. Capture
                // continues while storage catches up; joining happens on exit.
                r.request_stop();
                state.lock().unwrap().recording = r.state();
            }
            let mut index = 0;
            while index < draining.len() {
                if draining[index].is_finished() {
                    let mut old = draining.swap_remove(index);
                    if let Err(error) = old.finish() {
                        prior_recording_error = Some(format!("{error:#}"));
                    }
                } else {
                    index += 1;
                }
            }
            let elapsed = start.elapsed().as_nanos() as u64;
            let now = Instant::now();
            let dt = now.duration_since(last).as_secs_f32();
            last = now;
            if frames > 0 && dt > 0.0 {
                fps = if fps == 0.0 {
                    1.0 / dt
                } else {
                    1.0 / (0.95 / fps + 0.05 * dt)
                };
            }
            let mut measurement = tracker.process(&raw, background.as_ref(), elapsed)?;
            measurement.capture_fps = fps;
            if !publisher.send(&measurement)? {
                dropped += 1;
            }
            if !subscribers.is_empty() {
                subscribers.retain(|s| {
                    !matches!(
                        s.try_send(measurement.clone()),
                        Err(mpsc::TrySendError::Disconnected(_))
                    )
                });
            }
            frames += 1;
            let publish_preview = preview_at.elapsed()
                >= Duration::from_secs_f64(1.0 / config.preview_fps as f64)
                || frames == 1;
            let foreground = if config.tracking.subtract_background && publish_preview {
                background
                    .as_ref()
                    .map(|bg| raw.difference(bg))
                    .transpose()?
                    .map(Arc::new)
            } else {
                None
            };
            if publish_preview {
                let snapshot = Arc::new(Snapshot {
                    frame: raw,
                    foreground,
                    measurement,
                });
                let mut s = state.lock().unwrap();
                s.latest = Some(snapshot);
                s.frames = frames;
                s.udp_dropped = dropped;
                s.zmq_dropped = publisher.zmq_dropped();
                if let Some(r) = &recorder {
                    s.recording = r.state();
                }
                preview_at = Instant::now();
            }
            if limit.is_some_and(|limit| frames >= limit) {
                break;
            }
        }
        Ok(())
    })();
    let mut recording_result = if let Some(r) = &mut recorder {
        r.finish()
    } else {
        Ok(())
    };
    for r in &mut draining {
        if let Err(error) = r.finish() {
            recording_result = Err(error);
        }
    }
    let output_result = publisher.flush();
    let mut s = state.lock().unwrap();
    s.frames = frames;
    s.udp_dropped = dropped;
    s.zmq_dropped = publisher.zmq_dropped();
    if let Some(r) = &recorder {
        s.recording = r.state();
    }
    processing_result?;
    recording_result?;
    if let Some(error) = prior_recording_error {
        anyhow::bail!(error);
    }
    output_result?;
    Ok(())
}
