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
    assert!(
        state.source.starts_with("Sample video") || state.source == "Simulated fly",
        "{}",
        state.source
    );
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
