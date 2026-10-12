use rekke::{
    config::Config,
    geometry::{self, Handle},
    persistence::Autosaver,
};
#[test]
fn autosave_flushes_last_marker_state_on_close_and_preserves_background_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rig.toml");
    let mut c = Config::demo();
    c.save(&path).unwrap();
    let saver = Autosaver::new(path.clone());
    for x in [322.0, 326.0, 330.0] {
        let p = rekke::config::Point { x, y: 250.0 };
        geometry::move_handle(&mut c, Handle::Body, p).unwrap();
        saver.submit(c.clone());
    }
    c.tracking.right.method = rekke::config::Method::WingBlur;
    c.tracking.right.spike_width_deg = 7.0;
    saver.submit(c.clone());
    drop(saver);
    assert_eq!(Config::load(&path).unwrap(), c);
    assert!(
        dir.path()
            .read_dir()
            .unwrap()
            .all(|e| e.unwrap().file_name() == "rig.toml")
    );
}
#[test]
fn invalid_autosave_keeps_the_last_valid_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rig.toml");
    let mut c = Config::demo();
    c.save(&path).unwrap();
    let original = std::fs::read(&path).unwrap();
    c.tracking.right.outer_radius = -1.0;
    let saver = Autosaver::new(path.clone());
    saver.submit(c);
    drop(saver);
    assert_eq!(std::fs::read(path).unwrap(), original);
}

#[test]
fn immediate_save_serializes_with_pending_drag_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rig.toml");
    let saver = Autosaver::new(path.clone());
    let mut c = Config::demo();
    saver.submit(c.clone());
    c.head.x = 325.0;
    saver.save_now(c.clone());
    let saved = saver
        .results
        .recv_timeout(std::time::Duration::from_secs(3))
        .unwrap()
        .unwrap();
    assert_eq!(saved, c);
    assert_eq!(Config::load(&path).unwrap(), c);
    drop(saver);
    assert_eq!(Config::load(&path).unwrap(), c);
}
