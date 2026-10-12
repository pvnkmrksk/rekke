use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use rekke::{
    avi::VideoFormat,
    config::{Config, Overrides, SourceConfig, camera_source},
    engine::{Engine, StartOptions},
    lock::ConfigLock,
    source,
    tracker::Tracker,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Parser)]
#[command(
    version,
    about = "Track insect wings from cameras or video files.",
    long_about = "Track insect wings from cameras or video files.\n\n\
        Run without arguments to open the first camera (or a sample video if none is connected).\n\
        Examples:\n  \
        rekke                                   open the GUI\n  \
        rekke recording.avi                     track a video file\n  \
        rekke --device 1 --label arena-2        second camera, custom label\n  \
        rekke --rigs rigs.toml                  every rig in a manifest, one window each\n  \
        rekke --rigs rigs.toml --headless --record\n  \
        rekke --list-cameras\n  \
        rekke clip big.avi sample.avi --start 60 --duration 30",
    args_conflicts_with_subcommands = true
)]
struct Args {
    /// A video file to track (AVI natively; other formats need FFmpeg).
    #[arg(value_name = "VIDEO", conflicts_with_all = ["rigs", "device"])]
    input: Option<PathBuf>,
    /// This instance's configuration (TOML). Edits in the GUI are saved here.
    #[arg(short, long, conflicts_with = "rigs")]
    config: Option<PathBuf>,
    /// Launch every enabled rig in this manifest, each as its own process.
    #[arg(long, num_args = 0..=1, default_missing_value = "auto", conflicts_with_all = ["init", "benchmark", "screenshot", "list_cameras", "video", "device", "label"])]
    rigs: Option<PathBuf>,
    /// Use this camera for this run: auto, a number from --list-cameras, a
    /// device path or part of the camera name. Not saved to the config.
    #[arg(long, conflicts_with = "video")]
    device: Option<String>,
    /// Track this video file for this run (same as the VIDEO argument).
    #[arg(long, value_name = "FILE")]
    video: Option<PathBuf>,
    /// With a video: process as fast as possible instead of at its frame rate.
    #[arg(long)]
    fast: bool,
    /// With a video: play it in a loop.
    #[arg(long = "loop")]
    repeat: bool,
    /// Window title, instance name and recording prefix for this run.
    #[arg(long)]
    label: Option<String>,
    /// Publish Kinefly-format ZMQ on this TCP port for this run.
    #[arg(long, value_name = "PORT")]
    zmq_port: Option<u16>,
    /// Send full UDP measurements from this local port for this run.
    #[arg(long, value_name = "PORT")]
    udp_port: Option<u16>,
    /// Send full UDP measurements to this IP:port for this run.
    #[arg(long, value_name = "IP:PORT")]
    udp_target: Option<String>,
    /// Record video to this instance's recording directory.
    #[arg(long)]
    record: bool,
    #[arg(long, hide = true)]
    supervised: bool,
    /// Create a sample configuration at --config and exit (never overwrites).
    #[arg(long)]
    init: bool,
    /// Run acquisition and output without a GUI.
    #[arg(long)]
    headless: bool,
    /// Stop after this many processed frames.
    #[arg(long)]
    frames: Option<u64>,
    /// List cameras with the numbers to use for `device`, then exit.
    #[arg(long)]
    list_cameras: bool,
    /// Use the simulated fly instead of the camera for this run.
    #[arg(long, conflicts_with_all = ["device", "video"])]
    demo: bool,
    /// Benchmark processing of a fixed synthetic frame; does not acquire a camera.
    #[arg(long)]
    benchmark: bool,
    /// Capture the GUI to a PNG after two seconds, then exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    /// Open GUI panels at start (settings, plots); used for documentation.
    #[arg(long, hide = true, value_delimiter = ',')]
    show: Vec<String>,
    #[command(subcommand)]
    command: Option<Tool>,
}
#[derive(Subcommand)]
enum Tool {
    /// Cut part of a video into a new, usually much smaller, AVI.
    ///
    /// Reads AVI natively (including Rekke/Wingtrace recordings); other
    /// formats need FFmpeg. Never overwrites an existing file.
    Clip {
        input: PathBuf,
        output: PathBuf,
        /// Seconds to skip from the start.
        #[arg(long, default_value_t = 0.0)]
        start: f64,
        /// Seconds to keep (default: until the end).
        #[arg(long)]
        duration: Option<f64>,
        /// mjpeg (small), png (lossless) or raw (lossless, large).
        #[arg(long, value_enum, default_value_t = FormatArg::Mjpeg)]
        format: FormatArg,
        /// MJPEG quality 1–100.
        #[arg(long, default_value_t = 90)]
        quality: u8,
        /// Keep every Nth frame.
        #[arg(long, default_value_t = 1)]
        every: u32,
    },
    /// Render a synthetic tethered-fly video with known wing angles.
    #[command(hide = true)]
    Synth {
        output: PathBuf,
        #[arg(long, default_value_t = 30.0)]
        seconds: f64,
        #[arg(long, default_value_t = 30)]
        fps: u32,
        #[arg(long, default_value_t = 82)]
        quality: u8,
        /// Sensor noise, gray levels.
        #[arg(long, default_value_t = 1.6)]
        noise: f32,
    },
}
#[derive(Clone, Copy, clap::ValueEnum)]
enum FormatArg {
    Mjpeg,
    Png,
    Raw,
}
impl From<FormatArg> for VideoFormat {
    fn from(f: FormatArg) -> Self {
        match f {
            FormatArg::Mjpeg => Self::Mjpeg,
            FormatArg::Png => Self::Png,
            FormatArg::Raw => Self::Raw,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("rekke: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args = Args::parse();
    ensure!(
        args.frames.is_none_or(|n| n > 0),
        "--frames must be greater than zero"
    );
    match args.command {
        Some(Tool::Clip {
            ref input,
            ref output,
            start,
            duration,
            format,
            quality,
            every,
        }) => {
            ensure!((1..=100).contains(&quality), "--quality must be 1–100");
            let summary = rekke::clip::clip(
                input,
                output,
                &rekke::clip::ClipOptions {
                    start,
                    duration,
                    format: format.into(),
                    quality,
                    every,
                },
            )?;
            println!(
                "Wrote {} frames ({}×{} @ {:.2} fps, {:.1} MB) to {}",
                summary.frames,
                summary.width,
                summary.height,
                summary.fps,
                summary.bytes as f64 / 1e6,
                output.display()
            );
            return Ok(());
        }
        Some(Tool::Synth {
            ref output,
            seconds,
            fps,
            quality,
            noise,
        }) => {
            let options = rekke::synth::SceneOptions {
                noise,
                ..Default::default()
            };
            let summary =
                rekke::clip::synthesize(output, seconds, fps, 640, 480, quality, options)?;
            println!(
                "Wrote {} synthetic frames ({:.1} MB) to {}",
                summary.frames,
                summary.bytes as f64 / 1e6,
                output.display()
            );
            return Ok(());
        }
        None => {}
    }
    if args.list_cameras {
        let cameras = source::list_cameras()?;
        if cameras.is_empty() {
            println!("No cameras found.");
        } else {
            println!("Use the number (or the path/name) as source.device:");
            for camera in cameras {
                println!("  {camera}");
            }
        }
        return Ok(());
    }
    if let Some(manifest) = &args.rigs {
        let automatic;
        let manifest = if manifest == Path::new("auto") {
            automatic = default_config()?
                .parent()
                .unwrap_or(Path::new("."))
                .join("rigs.toml");
            &automatic
        } else {
            manifest
        };
        return rekke::rigs::launch(
            manifest,
            rekke::rigs::LaunchOptions {
                headless: args.headless,
                record: args.record,
                demo: args.demo,
                frames: args.frames,
            },
        );
    }
    if args.init {
        let path = args.config.clone().unwrap_or_else(|| "rekke.toml".into());
        ensure!(!path.exists(), "Config already exists: {}", path.display());
        Config::default().save(&path)?;
        println!(
            "Created {}. Run rekke --config {}",
            path.display(),
            path.display()
        );
        return Ok(());
    }
    // One running instance per config. Without --config, a second window
    // takes the next free rig config instead of the same camera and ports.
    let (config_path, lock) = if let Some(path) = args.config.clone() {
        let lock = (!args.benchmark)
            .then(|| ConfigLock::acquire(&path))
            .transpose()?;
        (path, lock)
    } else if args.benchmark {
        (default_config()?, None)
    } else {
        let (path, lock) = rekke::installation::free_config(&default_config()?)?;
        (path, Some(lock))
    };
    let saved = if args.config.is_none() && !config_path.exists() {
        Config::default()
    } else {
        Config::load(&config_path).with_context(|| format!("Config {}", config_path.display()))?
    };
    let config_path = if config_path.exists() {
        config_path.canonicalize()?
    } else {
        std::env::current_dir()?.join(config_path)
    };
    let mut overrides = Overrides {
        label: args.label.clone(),
        zmq_port: args.zmq_port,
        udp_port: args.udp_port,
        udp_target: args.udp_target.clone(),
        record: args.record,
        ..Default::default()
    };
    if let Some(device) = &args.device {
        overrides.source = Some(camera_source(&saved, device));
    }
    if let Some(video) = args.video.as_ref().or(args.input.as_ref()) {
        let video = video
            .canonicalize()
            .with_context(|| format!("Video {}", video.display()))?;
        let (width, height, fps) = saved.source.dimensions();
        overrides.source = Some(SourceConfig::Video {
            path: video.to_string_lossy().into_owned(),
            width,
            height,
            fps,
            repeat: args.repeat,
            realtime: !args.fast,
        });
        overrides.note = Some(format!(
            "Tracking {}",
            video.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    if args.demo {
        let (width, height, fps) = saved.source.dimensions();
        overrides.source = Some(SourceConfig::Demo {
            width,
            height,
            fps,
            beat_hz: 0.8_f32.min(fps as f32 / 4.0),
        });
    }
    let config = overrides.apply(&saved);
    config.validate()?;
    if args.benchmark {
        return benchmark(config, args.frames.unwrap_or(2000));
    }
    let stop = Arc::new(AtomicBool::new(false));
    if args.supervised {
        let flag = stop.clone();
        std::thread::spawn(move || {
            use std::io::BufRead;
            let mut line = String::new();
            // EOF also stops an orphaned rig if its launcher disappears.
            let _ = std::io::stdin().lock().read_line(&mut line);
            flag.store(true, Ordering::Relaxed);
        });
    }
    let flag = stop.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::Relaxed))?;
    if args.headless {
        let engine = Engine::start(config, config_path, args.frames)?;
        while !engine.is_finished() {
            if stop.load(Ordering::Relaxed) {
                engine.stop();
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let state = engine.state();
        engine.wait();
        drop(lock);
        if let Some(error) = state.error {
            anyhow::bail!(error);
        }
        println!(
            "Processed {} frames from {}; {} local UDP drops; {} ZMQ queue drops; video {}/{} frames written",
            state.frames,
            state.source,
            state.udp_dropped,
            state.zmq_dropped,
            state.recording.written,
            state.recording.accepted
        );
        if let Some(error) = state.recording.error {
            anyhow::bail!(error);
        }
        return Ok(());
    }
    #[cfg(feature = "gui")]
    {
        let options = StartOptions {
            limit: args.frames,
            // A rig launched for an experiment must show its camera error,
            // never a sample that could be mistaken for live data.
            fallback: !args.supervised,
            degrade_output: !args.supervised,
        };
        let result = rekke::gui::launch(rekke::gui::Launch {
            saved,
            overrides,
            path: config_path,
            options,
            screenshot: args.screenshot,
            panels: args.show.clone(),
            shutdown: stop,
        })
        .map_err(|e| anyhow::anyhow!("GUI: {e}"));
        drop(lock);
        result
    }
    #[cfg(not(feature = "gui"))]
    {
        let _ = (saved, overrides, lock, StartOptions::default());
        anyhow::bail!("Built without GUI support; use --headless")
    }
}
fn benchmark(config: Config, count: u64) -> Result<()> {
    ensure!(count > 0, "Benchmark frame count must be positive");
    let (w, h, fps) = config.source.dimensions();
    let demo = source::DemoSource::new(w, h, fps, 1.0);
    let raw = demo.render(0.031, 0);
    let bg = demo.blank_frame();
    let tracker = Tracker::new(&config, w, h);
    for _ in 0..20 {
        std::hint::black_box(tracker.process(&raw, Some(&bg), 0)?);
    }
    let start = std::time::Instant::now();
    for _ in 0..count {
        std::hint::black_box(tracker.process(&raw, Some(&bg), 0)?);
    }
    let seconds = start.elapsed().as_secs_f64();
    println!(
        "{count} frames, {w}×{h}, {} angular × {} radial samples × {} enabled sectors",
        config.tracking.angle_bins,
        config.tracking.radial_samples,
        config.sectors().iter().filter(|s| s.enabled).count()
    );
    println!(
        "Processing only: {:.1} µs/frame, {:.0} frames/s ({seconds:.3}s total)",
        seconds * 1e6 / count as f64,
        count as f64 / seconds
    );
    println!(
        "Includes foreground preparation and tracking; excludes acquisition, GUI, serialization and output."
    );
    Ok(())
}

fn default_config() -> Result<PathBuf> {
    config_path_for(
        &std::env::current_dir()?,
        std::env::current_exe().ok().as_deref(),
        rekke::installation::user_directory().as_deref(),
    )
}
fn config_path_for(cwd: &Path, exe: Option<&Path>, app_data: Option<&Path>) -> Result<PathBuf> {
    rekke::installation::select(cwd, exe, app_data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "rekke-install-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn cli_parses_videos_overrides_and_tools() {
        let a = Args::try_parse_from(["rekke", "fly.avi", "--fast", "--label", "arena"]).unwrap();
        assert_eq!(a.input, Some(PathBuf::from("fly.avi")));
        assert!(a.fast);
        assert_eq!(a.label.as_deref(), Some("arena"));
        let a = Args::try_parse_from(["rekke", "--device", "1", "--zmq-port", "9900"]).unwrap();
        assert_eq!(a.device.as_deref(), Some("1"));
        assert_eq!(a.zmq_port, Some(9900));
        let a = Args::try_parse_from(["rekke", "clip", "in.avi", "out.avi", "--duration", "30"])
            .unwrap();
        assert!(matches!(a.command, Some(Tool::Clip { duration: Some(d), .. }) if d == 30.0));
        assert!(Args::try_parse_from(["rekke", "--rigs", "r.toml", "--device", "1"]).is_err());
        assert!(Args::try_parse_from(["rekke", "--device", "1", "--video", "a.avi"]).is_err());
        let a = Args::try_parse_from(["rekke", "--rigs"]).unwrap();
        assert_eq!(a.rigs, Some(PathBuf::from("auto")));
    }
    #[test]
    fn isolated_mac_bundle_creates_a_writable_camera_config() {
        let t = Scratch::new();
        let cwd = t.0.join("cwd");
        let exe = t.0.join("isolated/Rekke.app/Contents/MacOS/rekke");
        let data = t.0.join("user-data");
        let path = config_path_for(&cwd, Some(&exe), Some(&data)).unwrap();
        assert_eq!(path, data.join("configs/camera-a.toml"));
        let mut c = Config::load(&path).unwrap();
        assert!(matches!(&c.source, SourceConfig::Camera { device, .. } if device == "auto"));
        assert!(!c.tracking.asymmetric);
        assert_eq!(c.tracking.left.threshold, 0.0);
        c.tracking.left.outer_radius = 170.0;
        c.save(&path).unwrap();
        assert_eq!(
            Config::load(&path).unwrap().tracking.left.outer_radius,
            170.0
        );
        assert_eq!(
            c.resolve(&path, &c.tracking.background_path).parent(),
            Some(data.join("configs").as_path())
        );
    }
    #[test]
    fn user_config_is_preserved_across_mac_bundle_launches() {
        let t = Scratch::new();
        let exe = t.0.join("isolated/Rekke.app/Contents/MacOS/rekke");
        let data = t.0.join("user-data");
        let path = config_path_for(&t.0, Some(&exe), Some(&data)).unwrap();
        let mut c = Config::load(&path).unwrap();
        c.instance = "custom-rig".into();
        c.output.bind = "127.0.0.1:5527".into();
        c.tracking.asymmetric = true;
        c.save(&path).unwrap();
        let saved = fs::read(&path).unwrap();
        assert_eq!(
            config_path_for(&t.0, Some(&exe), Some(&data)).unwrap(),
            path
        );
        assert_eq!(fs::read(&path).unwrap(), saved);
    }
    #[test]
    fn portable_and_local_configs_keep_precedence_over_mac_fallback() {
        let t = Scratch::new();
        let package = t.0.join("installed/Rekke");
        let exe = package.join("rekke");
        let bundled = package.join("configs/camera-a.toml");
        let data = t.0.join("user-data");
        Config::default().save(&bundled).unwrap();
        assert_eq!(
            config_path_for(&t.0, Some(&exe), Some(&data)).unwrap(),
            bundled
        );
        let local = t.0.join("rekke.toml");
        Config::default().save(&local).unwrap();
        assert_eq!(
            config_path_for(&t.0, Some(&exe), Some(&data)).unwrap(),
            local
        );
        assert!(!data.exists());
    }
    #[test]
    fn plain_cli_does_not_create_an_implicit_user_config() {
        let t = Scratch::new();
        let exe = t.0.join("bin/rekke");
        let data = t.0.join("user-data");
        let path = config_path_for(&t.0, Some(&exe), Some(&data)).unwrap();
        assert_eq!(path, t.0.join("rekke.toml"));
        assert!(!path.exists());
        assert!(!data.exists());
    }
}
