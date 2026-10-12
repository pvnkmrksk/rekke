use rekke::{config::Config, engine::Engine, tracker::Measurement};
use std::{net::UdpSocket, path::PathBuf, time::Duration};
fn sender_address() -> String {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.local_addr().unwrap().to_string()
}
#[test]
fn simultaneous_instances_send_independent_configured_streams() {
    let a = UdpSocket::bind("127.0.0.1:0").unwrap();
    let b = UdpSocket::bind("127.0.0.1:0").unwrap();
    a.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    b.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    let mut ca = Config::demo();
    ca.output.bind = sender_address();
    ca.output.target = a.local_addr().unwrap().to_string();
    ca.instance = "a".into();
    let mut cb = ca.clone();
    cb.instance = "b".into();
    cb.output.bind = sender_address();
    cb.output.target = b.local_addr().unwrap().to_string();
    assert_ne!(ca.output.bind, cb.output.bind);
    let ea = Engine::start(ca.clone(), PathBuf::from("unused-a.toml"), Some(4)).unwrap();
    let eb = Engine::start(cb.clone(), PathBuf::from("unused-b.toml"), Some(4)).unwrap();
    let mut buffer = [0; 8192];
    let (n, peer) = a.recv_from(&mut buffer).unwrap();
    let ma: Measurement = serde_json::from_slice(&buffer[..n]).unwrap();
    assert_eq!(ma.instance, "a");
    assert_eq!(peer.to_string(), ca.output.bind);
    let (n, peer) = b.recv_from(&mut buffer).unwrap();
    let mb: Measurement = serde_json::from_slice(&buffer[..n]).unwrap();
    assert_eq!(mb.instance, "b");
    assert_eq!(peer.to_string(), cb.output.bind);
    ea.wait();
    eb.wait();
}
#[test]
fn conflicting_sender_port_fails_with_a_clear_error() {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut config = Config::demo();
    config.output.bind = socket.local_addr().unwrap().to_string();
    let error = match Engine::start(config, PathBuf::from("unused.toml"), Some(1)) {
        Ok(_) => panic!("Port conflict was accepted"),
        Err(e) => e,
    };
    assert!(format!("{error:#}").contains("unique sender port"));
}
#[test]
fn background_is_saved_and_reloaded_without_global_state() {
    let temp = std::env::temp_dir().join(format!(
        "rekke-bg-{}-{}",
        std::process::id(),
        rekke::frame::unix_ns()
    ));
    std::fs::create_dir_all(&temp).unwrap();
    let path = temp.join("rig.toml");
    let mut c = Config::demo();
    c.output.enabled = false;
    c.tracking.background_path = "reference.png".into();
    let engine = Engine::start(c.clone(), path.clone(), None).unwrap();
    engine.demo_background().unwrap();
    let start = std::time::Instant::now();
    while !engine.state().background_loaded && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(engine.state().background_loaded);
    engine.wait();
    assert!(temp.join("reference.png").is_file());
    c.tracking.subtract_background = true;
    let engine = Engine::start(c, path, Some(2)).unwrap();
    let start = std::time::Instant::now();
    while !engine.is_finished() && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let state = engine.state();
    assert!(state.error.is_none(), "{:?}", state.error);
    assert!(state.latest.unwrap().measurement.background_subtracted);
    engine.wait();
    std::fs::remove_dir_all(temp).unwrap();
}
#[test]
fn threshold_and_enable_changes_apply_without_restarting_capture() {
    let mut c = Config::demo();
    c.output.enabled = false;
    let engine = Engine::start(c.clone(), PathBuf::from("unused-live.toml"), None).unwrap();
    let wait = |condition: &dyn Fn(&rekke::engine::State) -> bool| {
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            let state = engine.state();
            assert!(state.error.is_none(), "{:?}", state.error);
            if condition(&state) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("Live update was not observed");
    };
    wait(&|s| {
        s.latest
            .as_ref()
            .is_some_and(|v| v.measurement.right.angle_rad.is_some())
    });
    c.tracking.right.threshold = 1.0;
    engine.update(&c).unwrap();
    wait(&|s| {
        s.latest
            .as_ref()
            .is_some_and(|v| v.measurement.right.enabled && v.measurement.right.angle_rad.is_none())
    });
    c.tracking.right.threshold = 0.0;
    engine.update(&c).unwrap();
    wait(&|s| {
        s.latest
            .as_ref()
            .is_some_and(|v| v.measurement.right.angle_rad.is_some())
    });
    c.tracking.right.enabled = false;
    engine.update(&c).unwrap();
    wait(&|s| {
        s.latest.as_ref().is_some_and(|v| {
            !v.measurement.right.enabled && v.measurement.left_minus_right_rad.is_none()
        })
    });
    engine.wait();
}

#[test]
fn video_recording_can_start_stop_and_restart_without_reopening_capture() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = Config::demo();
    config.output.enabled = false;
    config.recording.directory = "recordings".into();
    let engine = Engine::start(config, directory.path().join("rig.toml"), None).unwrap();
    let wait = |predicate: &dyn Fn(&rekke::engine::State) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let state = engine.state();
            assert!(state.error.is_none(), "{:?}", state.error);
            if predicate(&state) {
                return state;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for recorder state"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    let before = wait(&|s| s.frames >= 3).frames;
    engine.record(true).unwrap();
    let first = wait(&|s| s.recording.active && s.recording.written >= 4);
    assert!(first.frames > before);
    engine.record(false).unwrap();
    let stopped = wait(&|s| {
        !s.recording.active
            && s.recording.written >= 4
            && s.recording.written == s.recording.accepted
    });
    assert_eq!(stopped.recording.accepted, stopped.recording.written);
    engine.record(true).unwrap();
    let second = wait(&|s| {
        s.recording.active
            && s.recording.written >= 3
            && s.recording.directory != first.recording.directory
    });
    assert!(second.frames > first.frames);
    engine.stop();
    let final_state = wait(&|s| s.finished);
    assert_eq!(
        final_state.recording.accepted,
        final_state.recording.written
    );
    assert!(final_state.recording.error.is_none());
    engine.wait();
    assert_eq!(
        directory
            .path()
            .join("recordings")
            .read_dir()
            .unwrap()
            .count(),
        2
    );
}
