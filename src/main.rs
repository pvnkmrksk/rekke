use anyhow::{Context, Result, ensure};
use clap::Parser;
use rekke::{
    config::{Config, SourceConfig},
    engine::Engine,
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
    about = "Insect wing tracking with independent rigs, ZMQ output and lossless recording"
)]
struct Args {
    #[arg(short, long, conflicts_with = "rigs")]
    config: Option<PathBuf>,
    /// Launch every enabled rig in this manifest, each as an independent process.
    #[arg(long, num_args = 0..=1, default_missing_value = "auto", conflicts_with_all = ["config", "init", "benchmark", "screenshot", "list_cameras"])]
    rigs: Option<PathBuf>,
    /// Record lossless grayscale video to this instance's recording directory.
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
    /// Stop after this many processed frames (useful for validation/replay).
    #[arg(long)]
    frames: Option<u64>,
    /// Enumerate OS camera devices and exit.
    #[arg(long)]
    list_cameras: bool,
    /// Override input with the simulated camera, keeping geometry and output.
    #[arg(long)]
    demo: bool,
    /// Benchmark processing of a fixed synthetic frame; does not acquire a camera.
    #[arg(long)]
    benchmark: bool,
    /// Capture the GUI to a PNG after two seconds, then exit.
    #[arg(long)]
    screenshot: Option<PathBuf>,
}
fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(
        args.frames.is_none_or(|n| n > 0),
        "--frames must be greater than zero"
    );
    if args.list_cameras {
        for camera in source::list_cameras()? {
            println!("{camera}");
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
    let config_path = if let Some(path) = args.config.clone() {
        path
    } else if args.init {
        PathBuf::from("rekke.toml")
    } else {
        default_config()?
    };
    if args.init {
        ensure!(
            !config_path.exists(),
            "Config already exists: {}",
            config_path.display()
        );
        Config::default().save(&config_path)?;
        println!(
            "Created {}. Run rekke --config {}",
            config_path.display(),
            config_path.display()
        );
        return Ok(());
    }
    let mut config = if args.config.is_none() && !config_path.exists() {
        Config::default()
    } else {
        Config::load(&config_path).context("Read configuration; use --init to create one")?
    };
    let config_path = if config_path.exists() {
        config_path.canonicalize()?
    } else {
        std::env::current_dir()?.join(config_path)
    };
    if args.demo {
        let (w, h, fps) = config.source.dimensions();
        config.source = SourceConfig::Demo {
            width: w,
            height: h,
            fps,
            beat_hz: 8.0_f32.min(fps as f32 / 4.0),
        };
    }
    if args.record {
        config.recording.enabled = true;
    }
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
        if let Some(error) = state.error {
            anyhow::bail!(error);
        }
        println!(
            "Processed {} frames; {} local UDP drops; {} ZMQ queue drops; video {}/{} frames written",
            state.frames,
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
        rekke::gui::launch(config, config_path, args.frames, args.screenshot, stop)
            .map_err(|e| anyhow::anyhow!("GUI: {e}"))
    }
    #[cfg(not(feature = "gui"))]
    {
        anyhow::bail!("Built without GUI support; use --headless")
    }
}
fn benchmark(config: Config, count: u64) -> Result<()> {
    ensure!(count > 0, "Benchmark frame count must be positive");
    let (w, h, fps) = config.source.dimensions();
    let demo = source::DemoSource::new(w, h, fps, 8.0);
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
        "Includes grayscale foreground preparation and tracking; excludes acquisition, GUI, serialization and output."
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
