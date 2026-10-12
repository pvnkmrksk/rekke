use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use rekkefly::{
    avi::VideoFormat,
    config::{Config, Overrides, SourceConfig, camera_source},
    engine::{Engine, StartOptions},
    lock::ConfigLock,
    source,
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
    about = "Rekkefly, the new Kinefly: track insect wings from cameras or video files.",
    long_about = "Rekkefly, the new Kinefly: track insect wings from cameras or video files.\n\n\
        Run without arguments to open the first camera (or a sample video if none is connected).\n\
        Examples:\n  \
        rekkefly                                   open the GUI\n  \
        rekkefly recording.avi                     track a video file\n  \
        rekkefly --device 1 --label arena-2        second camera, custom label\n  \
        rekkefly --rigs rigs.toml                  every rig in a manifest, one window each\n  \
        rekkefly --rigs rigs.toml --headless --record\n  \
        rekkefly --list-cameras\n  \
        rekkefly track recording.avi --csv angles.csv\n  \
        rekkefly clip big.avi sample.avi --start 60 --duration 30\n  \
        rekkefly mcp                               serve AI agents (Model Context Protocol)",
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
    /// Print machine-readable JSON (with --list-cameras, --benchmark, --headless).
    #[arg(long)]
    json: bool,
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
    /// Reads AVI natively (including Rekkefly/Wingtrace recordings); other
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
    /// Track a whole video as fast as possible and summarise the wing angles.
    ///
    /// Without --config the video's own markers are used (a .toml beside it,
    /// or the Bogong moth example's), else the defaults.
    Track {
        video: PathBuf,
        /// Markers and detector settings to use (never written).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Override the wing detector for both wings.
        #[arg(long, value_enum)]
        method: Option<MethodArg>,
        /// Write per-frame angles (degrees) and tracking time here.
        #[arg(long)]
        csv: Option<PathBuf>,
        /// Write every full measurement as JSON lines here.
        #[arg(long)]
        jsonl: Option<PathBuf>,
        /// Stop after this many frames.
        #[arg(long)]
        frames: Option<u64>,
        /// Print the summary as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Serve the Model Context Protocol on stdin/stdout, for AI agents and
    /// automation: cameras, configs, rigs, live angles, snapshots, videos.
    ///
    /// Add it to an MCP client, for example:  claude mcp add rekkefly -- rekkefly mcp
    Mcp {
        /// Folder of rig configs (default: the folder Rekkefly itself uses).
        #[arg(long)]
        config_dir: Option<PathBuf>,
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
enum MethodArg {
    /// Ignores legs and antennae (default).
    Robust,
    /// Kinefly's strongest edge.
    Classic,
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
        eprintln!("rekkefly: {error:#}");
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
            let summary = rekkefly::clip::clip(
                input,
                output,
                &rekkefly::clip::ClipOptions {
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
        Some(Tool::Track {
            ref video,
            ref config,
            method,
            ref csv,
            ref jsonl,
            frames,
            json,
        }) => {
            let loaded = config
                .as_deref()
                .map(|p| Config::load(p).with_context(|| format!("Config {}", p.display())))
                .transpose()?;
            let options = rekkefly::analyze::TrackOptions {
                frames,
                method: method.map(|m| match m {
                    MethodArg::Robust => rekkefly::config::Method::WingBlur,
                    MethodArg::Classic => rekkefly::config::Method::Edge,
                }),
                csv: csv.clone(),
                jsonl: jsonl.clone(),
                video_markers: config.is_none(),
            };
            let base = config.clone().unwrap_or_else(|| "unused.toml".into());
            let s = rekkefly::analyze::track(video, loaded.as_ref(), &base, &options)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&s)?);
                return Ok(());
            }
            let name = |m: rekkefly::config::Method| match m {
                rekkefly::config::Method::WingBlur => "robust",
                rekkefly::config::Method::Edge => "classic",
                rekkefly::config::Method::Centroid => "centroid",
            };
            println!(
                "Tracked {} frames of {} ({}×{}, {} detector{}) in {:.2} s: {:.0} frames/s, {:.0} µs tracking per frame (99% under {:.0} µs)",
                s.frames,
                video.file_name().unwrap_or_default().to_string_lossy(),
                s.width,
                s.height,
                name(s.method[0]),
                if s.video_markers {
                    ", the video's own markers"
                } else {
                    ""
                },
                s.timing.seconds,
                s.timing.frames_per_second,
                s.timing.mean_us,
                s.timing.p99_us
            );
            for (label, a) in [
                ("left", &s.left),
                ("right", &s.right),
                ("L−R", &s.left_minus_right),
                ("head", &s.head),
                ("abdomen", &s.abdomen),
            ] {
                if let (Some(mean), Some(sd), Some(min), Some(max)) = (a.mean, a.sd, a.min, a.max) {
                    println!(
                        "  {label:<8} mean {mean:+7.1}°  sd {sd:5.1}°  range {min:+6.1}° … {max:+6.1}°  ({} frames)",
                        a.valid
                    );
                }
            }
            for path in [&s.csv, &s.jsonl].into_iter().flatten() {
                println!("Wrote {}", path.display());
            }
            return Ok(());
        }
        Some(Tool::Mcp { ref config_dir }) => {
            let directory = match config_dir {
                Some(dir) => dir.clone(),
                None => default_config()?
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| ".".into()),
            };
            eprintln!(
                "rekkefly {}: MCP server on stdio, configs in {}",
                env!("CARGO_PKG_VERSION"),
                directory.display()
            );
            let stdin = std::io::stdin();
            return rekkefly::mcp::serve(stdin.lock(), std::io::stdout(), directory);
        }
        Some(Tool::Synth {
            ref output,
            seconds,
            fps,
            quality,
            noise,
        }) => {
            let options = rekkefly::synth::SceneOptions {
                noise,
                ..Default::default()
            };
            let summary =
                rekkefly::clip::synthesize(output, seconds, fps, 640, 480, quality, options)?;
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
        if args.json {
            println!("{}", serde_json::json!({ "cameras": cameras }));
        } else if cameras.is_empty() {
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
        return rekkefly::rigs::launch(
            manifest,
            rekkefly::rigs::LaunchOptions {
                headless: args.headless,
                record: args.record,
                demo: args.demo,
                frames: args.frames,
            },
        );
    }
    if args.init {
        let path = args
            .config
            .clone()
            .unwrap_or_else(|| "rekkefly.toml".into());
        ensure!(!path.exists(), "Config already exists: {}", path.display());
        Config::default().save(&path)?;
        println!(
            "Created {}. Run rekkefly --config {}",
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
        let (path, lock) = rekkefly::installation::free_config(&default_config()?)?;
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
        let b = rekkefly::analyze::benchmark(&config, args.frames.unwrap_or(2000))?;
        if args.json {
            println!("{}", serde_json::to_string_pretty(&b)?);
            return Ok(());
        }
        println!(
            "{} frames, {}×{}, {} angular × {} radial samples × {} enabled sectors",
            b.frames, b.width, b.height, b.angle_bins, b.radial_samples, b.sectors
        );
        println!(
            "Processing only: {:.1} µs/frame, {:.0} frames/s",
            b.mean_us, b.frames_per_second
        );
        println!(
            "Includes foreground preparation and tracking; excludes acquisition, GUI, serialization and output."
        );
        return Ok(());
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
        let options = StartOptions {
            limit: args.frames,
            video_markers: matches!(overrides.source, Some(SourceConfig::Video { .. })),
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let engine = Engine::start_with(config, config_path, options)?;
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
        let seconds = started.elapsed().as_secs_f64();
        if args.json {
            println!(
                "{}",
                serde_json::json!({
                    "frames": state.frames,
                    "source": state.source,
                    "seconds": seconds,
                    "frames_per_second": state.frames as f64 / seconds.max(1e-9),
                    "udp_dropped": state.udp_dropped,
                    "zmq_dropped": state.zmq_dropped,
                    "recording": {
                        "written": state.recording.written,
                        "accepted": state.recording.accepted,
                        "directory": state.recording.directory,
                        "error": state.recording.error,
                    },
                })
            );
        } else {
            println!(
                "Processed {} frames from {} in {:.2} s ({:.0} frames/s); {} local UDP drops; {} ZMQ queue drops; video {}/{} frames written",
                state.frames,
                state.source,
                seconds,
                state.frames as f64 / seconds.max(1e-9),
                state.udp_dropped,
                state.zmq_dropped,
                state.recording.written,
                state.recording.accepted
            );
        }
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
            ..Default::default()
        };
        let result = rekkefly::gui::launch(rekkefly::gui::Launch {
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
fn default_config() -> Result<PathBuf> {
    config_path_for(
        &std::env::current_dir()?,
        std::env::current_exe().ok().as_deref(),
        rekkefly::installation::user_directory().as_deref(),
    )
}
fn config_path_for(cwd: &Path, exe: Option<&Path>, app_data: Option<&Path>) -> Result<PathBuf> {
    rekkefly::installation::select(cwd, exe, app_data)
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
                "rekkefly-install-{}-{}",
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
        let a =
            Args::try_parse_from(["rekkefly", "fly.avi", "--fast", "--label", "arena"]).unwrap();
        assert_eq!(a.input, Some(PathBuf::from("fly.avi")));
        assert!(a.fast);
        assert_eq!(a.label.as_deref(), Some("arena"));
        let a = Args::try_parse_from(["rekkefly", "--device", "1", "--zmq-port", "9900"]).unwrap();
        assert_eq!(a.device.as_deref(), Some("1"));
        assert_eq!(a.zmq_port, Some(9900));
        let a = Args::try_parse_from(["rekkefly", "clip", "in.avi", "out.avi", "--duration", "30"])
            .unwrap();
        assert!(matches!(a.command, Some(Tool::Clip { duration: Some(d), .. }) if d == 30.0));
        assert!(Args::try_parse_from(["rekkefly", "--rigs", "r.toml", "--device", "1"]).is_err());
        assert!(Args::try_parse_from(["rekkefly", "--device", "1", "--video", "a.avi"]).is_err());
        let a = Args::try_parse_from(["rekkefly", "--rigs"]).unwrap();
        assert_eq!(a.rigs, Some(PathBuf::from("auto")));
    }
    #[test]
    fn isolated_mac_bundle_creates_a_writable_camera_config() {
        let t = Scratch::new();
        let cwd = t.0.join("cwd");
        let exe = t.0.join("isolated/Rekkefly.app/Contents/MacOS/rekkefly");
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
        let exe = t.0.join("isolated/Rekkefly.app/Contents/MacOS/rekkefly");
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
        let package = t.0.join("installed/Rekkefly");
        let exe = package.join("rekkefly");
        let bundled = package.join("configs/camera-a.toml");
        let data = t.0.join("user-data");
        Config::default().save(&bundled).unwrap();
        assert_eq!(
            config_path_for(&t.0, Some(&exe), Some(&data)).unwrap(),
            bundled
        );
        let local = t.0.join("rekkefly.toml");
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
        let exe = t.0.join("bin/rekkefly");
        let data = t.0.join("user-data");
        let path = config_path_for(&t.0, Some(&exe), Some(&data)).unwrap();
        assert_eq!(path, t.0.join("rekkefly.toml"));
        assert!(!path.exists());
        assert!(!data.exists());
    }
}
