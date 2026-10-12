//! Video playback, the no-camera fallback, degraded outputs and run-only
//! overrides: the behaviours that keep a double-clicked app working.
use rekkefly::{
    avi::{AviWriter, VideoFormat},
    config::{Config, Overrides, SourceConfig, camera_source},
    engine::{Engine, StartOptions},
    source::{AviSource, FrameSource},
};
use std::{
    path::PathBuf,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn root() -> &'static std::path::Path {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
}
fn write_video(path: &std::path::Path, width: u32, height: u32, frames: u8, fps: u32) {
    let mut writer = AviWriter::create(path, width, height, fps, 1, VideoFormat::Raw).unwrap();
    for k in 0..frames {
        writer
            .write(&vec![k * 10; (width * height) as usize])
            .unwrap();
    }
    writer.finish().unwrap();
}
fn wait(
    engine: &Engine,
    until: impl Fn(&rekkefly::engine::State) -> bool,
) -> rekkefly::engine::State {
    let start = Instant::now();
    loop {
        let state = engine.state();
        if until(&state) || start.elapsed() > Duration::from_secs(20) {
            return state;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn video_plays_fast_or_paced_loops_and_fits_any_size_without_distortion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("clip.avi");
    write_video(&path, 64, 32, 5, 10);
    let stop = AtomicBool::new(false);
    // As fast as possible, letterboxed from 2:1 into 4:3.
    let mut fast = AviSource::open(&path, 64, 48, false, false).unwrap();
    let start = Instant::now();
    let mut frames = Vec::new();
    while let Some(frame) = fast.next_frame(&stop).unwrap() {
        frames.push(frame);
    }
    assert_eq!(frames.len(), 5);
    assert!(start.elapsed() < Duration::from_millis(300));
    let last = &frames[4];
    assert_eq!((last.width, last.height), (64, 48));
    assert_eq!(last.pixels[24 * 64 + 32], 40, "image centred");
    assert_eq!(last.pixels[0], 40, "padding matches the border");
    // Paced at 10 fps, looping past the end with increasing sequence numbers.
    let mut paced = AviSource::open(&path, 64, 32, true, true).unwrap();
    let start = Instant::now();
    let mut last = 0;
    for _ in 0..7 {
        last = paced.next_frame(&stop).unwrap().unwrap().sequence;
    }
    assert_eq!(last, 6);
    assert!(
        start.elapsed() >= Duration::from_millis(400),
        "paced playback"
    );
}
#[test]
fn engine_tracks_a_native_video_and_stops_at_its_end() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fly.avi");
    rekkefly::clip::synthesize(&path, 1.0, 20, 640, 480, 85, Default::default()).unwrap();
    let mut config = Config::default();
    config.output.enabled = false;
    config.output.zmq_enabled = false;
    config.source = SourceConfig::Video {
        path: path.to_string_lossy().into_owned(),
        width: 640,
        height: 480,
        fps: 20,
        repeat: false,
        realtime: false,
    };
    let engine = Engine::start(config, dir.path().join("rig.toml"), None).unwrap();
    let state = wait(&engine, |s| s.finished);
    assert!(state.error.is_none(), "{:?}", state.error);
    assert_eq!(state.frames, 20);
    assert!(state.source.contains("fly.avi"), "{}", state.source);
    let m = &state.latest.unwrap().measurement;
    assert!(m.left.angle_rad.is_some() && m.right.angle_rad.is_some());
}
#[test]
fn missing_camera_falls_back_to_a_sample_only_when_allowed() {
    let mut config = Config::default();
    config.output.enabled = false;
    config.output.zmq_enabled = false;
    // A missing device path fails without asking the OS for camera access,
    // so this test never waits on a permission prompt on CI.
    config.source = camera_source(&config, "/nonexistent/rekkefly-test-camera");
    let config_source = config.source.clone();
    let path = PathBuf::from("unused-fallback.toml");
    // Experiments (headless, rigs) must fail loudly instead.
    let strict = Engine::start(config.clone(), path.clone(), Some(3)).unwrap();
    let state = wait(&strict, |s| s.finished);
    assert!(state.error.is_some());
    assert!(state.fallback.is_none());
    let engine = Engine::start_with(
        config,
        path,
        StartOptions {
            fallback: true,
            ..Default::default()
        },
    )
    .unwrap();
    let state = wait(&engine, |s| s.frames >= 5);
    assert!(state.error.is_none(), "{:?}", state.error);
    assert!(state.fallback.is_some());
    // The bundled example plays with the moth's own markers and sectors.
    assert_eq!(state.source, "Sample video: Bogong moth example");
    let example = state.example.as_ref().expect("the example's markers");
    let moth = Config::load(&root().join("configs/bogong-moth.toml")).unwrap();
    assert_eq!(example.head, moth.head);
    assert_eq!(example.tracking.left, moth.tracking.left);
    assert_eq!(
        example.source, config_source,
        "the run's own source and outputs"
    );
    let m = &state.latest.as_ref().unwrap().measurement;
    assert!(m.left.angle_rad.is_some() && m.right.angle_rad.is_some());
    // The sample must never become this rig's saved background or a recording.
    engine.save_background().unwrap();
    let state = wait(&engine, |s| s.message.contains("Background capture is off"));
    assert!(
        state.message.contains("Background capture is off"),
        "{}",
        state.message
    );
    engine.record(true).unwrap();
    let state = wait(&engine, |s| s.message.contains("Recording is off"));
    assert!(!state.recording.active);
    engine.wait();
}
#[test]
fn a_busy_output_port_turns_outputs_off_in_the_gui_but_fails_headless() {
    let busy = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut config = Config::demo();
    config.output.bind = busy.local_addr().unwrap().to_string();
    let path = PathBuf::from("unused-busy.toml");
    assert!(Engine::start(config.clone(), path.clone(), Some(2)).is_err());
    let engine = Engine::start_with(
        config,
        path,
        StartOptions {
            limit: Some(3),
            degrade_output: true,
            ..Default::default()
        },
    )
    .unwrap();
    let state = wait(&engine, |s| s.finished);
    assert!(state.error.is_none(), "{:?}", state.error);
    assert_eq!(state.frames, 3);
    let error = state.output_error.unwrap();
    assert!(error.contains("unique sender port"), "{error}");
    assert!(
        error.starts_with("UDP off"),
        "only the failing output: {error}"
    );
}
#[test]
fn run_only_overrides_apply_but_are_never_saved() {
    let saved = Config::default();
    let overrides = Overrides {
        label: Some("arena 2".into()),
        source: Some(camera_source(&saved, "2")),
        zmq_port: Some(9902),
        udp_port: Some(5602),
        udp_target: Some("10.0.0.5:6000".into()),
        record: true,
        note: None,
    };
    let mut running = overrides.apply(&saved);
    assert_eq!(running.instance, "arena 2");
    assert!(matches!(&running.source, SourceConfig::Camera { device, .. } if device == "2"));
    assert_eq!(running.output.zmq_bind, "tcp://*:9902");
    assert_eq!(running.output.bind, "127.0.0.1:5602");
    assert_eq!(running.output.target, "10.0.0.5:6000");
    assert!(running.recording.enabled);
    running.validate().unwrap();
    // A marker drag in the GUI is saved; the run-only values are not.
    running.head.x = 333.0;
    let persisted = overrides.persistable(&running, &saved);
    assert_eq!(persisted.head.x, 333.0);
    assert_eq!(persisted.instance, saved.instance);
    assert_eq!(persisted.source, saved.source);
    assert_eq!(persisted.output, saved.output);
    assert_eq!(persisted.recording, saved.recording);
    assert_eq!(Overrides::default().persistable(&running, &saved), running);
    // A camera chosen for a video config keeps that config's image size,
    // because the markers are in its pixel coordinates.
    let mut video = saved.clone();
    video.source = SourceConfig::Video {
        path: "a.avi".into(),
        width: 1280,
        height: 1024,
        fps: 50,
        repeat: false,
        realtime: true,
    };
    assert_eq!(camera_source(&video, "0").dimensions(), (1280, 1024, 50));
}
#[test]
fn example_markers_scale_into_any_frame_size_like_the_video_does() {
    let moth = Config::load(&root().join("configs/bogong-moth.toml")).unwrap();
    let mut rig = Config {
        source: SourceConfig::Camera {
            device: "0".into(),
            width: 1280,
            height: 1024,
            fps: 100,
        },
        ..Config::default()
    };
    rig.tracking.subtract_background = true;
    let example = rig.with_geometry_of(&moth).unwrap();
    // 640×480 fits 1280×1024 at 2×, centred with 32 px bars above and below.
    assert_eq!(
        (example.head.x, example.head.y),
        (2.0 * moth.head.x, 2.0 * moth.head.y + 32.0)
    );
    let (a, b) = (&example.tracking.left, &moth.tracking.left);
    assert_eq!(a.hinge.x, 2.0 * b.hinge.x);
    assert_eq!(a.outer_radius, 2.0 * b.outer_radius);
    assert_eq!(a.start_deg, b.start_deg);
    assert_eq!(example.tracking.dark_on_light, moth.tracking.dark_on_light);
    // Never the rig's background, never subtracted, same outputs and source.
    assert!(!example.tracking.subtract_background);
    assert_eq!(
        example.tracking.background_path,
        rig.tracking.background_path
    );
    assert_eq!(
        (&example.source, &example.output),
        (&rig.source, &rig.output)
    );
    // The bundled clip finds its markers; a sidecar config wins for any clip.
    let bundled = rekkefly::source::sample_config(&root().join("samples/bogong-moth.avi"));
    assert_eq!(bundled.unwrap().head, moth.head);
    let dir = tempfile::tempdir().unwrap();
    let mine = dir.path().join("sample.avi");
    assert!(rekkefly::source::sample_config(&mine).is_none());
    let mut sidecar = Config::default();
    sidecar.head.x = 300.0;
    sidecar.save(&dir.path().join("sample.toml")).unwrap();
    assert_eq!(
        rekkefly::source::sample_config(&mine).unwrap().head.x,
        300.0
    );
}
#[test]
fn a_video_opened_for_one_run_brings_its_own_markers() {
    let mut config = Config::default();
    config.output.enabled = false;
    config.output.zmq_enabled = false;
    config.source = SourceConfig::Video {
        path: root()
            .join("samples/bogong-moth.avi")
            .to_string_lossy()
            .into_owned(),
        width: 640,
        height: 480,
        fps: 30,
        repeat: false,
        realtime: false,
    };
    let path = PathBuf::from("unused-markers.toml");
    let moth = Config::load(&root().join("configs/bogong-moth.toml")).unwrap();
    for video_markers in [true, false] {
        let options = StartOptions {
            limit: Some(3),
            video_markers,
            ..Default::default()
        };
        let engine = Engine::start_with(config.clone(), path.clone(), options).unwrap();
        let state = wait(&engine, |s| s.finished);
        assert!(state.error.is_none(), "{:?}", state.error);
        assert_eq!(
            state.example.map(|e| e.head),
            video_markers.then_some(moth.head),
            "the config's own markers unless asked"
        );
    }
}
