use rekkefly::{
    config::{Config, SourceConfig},
    lock::ConfigLock,
    rigs::{LaunchOptions, RigEntry, RigSet, child_arguments},
};
fn entry(config: &str, record: bool) -> RigEntry {
    RigEntry {
        config: Some(config.into()),
        record,
        enabled: true,
        ..Default::default()
    }
}
fn fixture() -> (tempfile::TempDir, RigSet) {
    let dir = tempfile::tempdir().unwrap();
    let mut a = Config::demo();
    a.instance = "one".into();
    a.output.zmq_enabled = true;
    a.save(&dir.path().join("one.toml")).unwrap();
    let mut b = a;
    b.instance = "two".into();
    b.output.bind = "127.0.0.1:5502".into();
    b.output.zmq_bind = "tcp://*:9872".into();
    b.tracking.background_path = "two.png".into();
    b.save(&dir.path().join("two.toml")).unwrap();
    let rigs = RigSet {
        rigs: vec![entry("one.toml", false), entry("two.toml", true)],
    };
    (dir, rigs)
}
#[test]
fn manifest_maps_independent_paths_ports_and_record_flags() {
    let (dir, rigs) = fixture();
    let resolved = rigs.resolve(&dir.path().join("rigs.toml"), false).unwrap();
    assert_eq!(resolved.len(), 2);
    assert!(resolved[0].path.is_absolute());
    assert!(resolved[1].record);
    assert_ne!(
        resolved[0].config.output.zmq_bind,
        resolved[1].config.output.zmq_bind
    );
}
#[test]
fn shared_zmq_port_and_background_are_rejected_before_launch() {
    let (dir, rigs) = fixture();
    let path = dir.path().join("two.toml");
    let mut c = Config::load(&path).unwrap();
    c.output.zmq_bind = "tcp://127.0.0.1:9871".into();
    c.save(&path).unwrap();
    assert!(
        rigs.resolve(&dir.path().join("rigs.toml"), false)
            .unwrap_err()
            .to_string()
            .contains("ZMQ port")
    );
    c.output.zmq_bind = "tcp://*:9872".into();
    c.tracking.background_path = "./rig-a.background.png".into();
    c.save(&path).unwrap();
    assert!(
        rigs.resolve(&dir.path().join("rigs.toml"), false)
            .unwrap_err()
            .to_string()
            .contains("background")
    );
}
#[test]
fn disabled_rigs_are_skipped_and_duplicate_configs_rejected() {
    let (dir, mut rigs) = fixture();
    rigs.rigs[1].config = Some("one.toml".into());
    assert!(rigs.resolve(&dir.path().join("rigs.toml"), false).is_err());
    rigs.rigs[1].enabled = false;
    assert_eq!(
        rigs.resolve(&dir.path().join("rigs.toml"), false)
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn labels_devices_and_ports_from_one_manifest_create_missing_configs() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("rigs.toml");
    std::fs::write(
        &manifest,
        r#"
[[rigs]]
label = "left-arena"
device = "0"
zmq_port = 9901

[[rigs]]
label = "right arena"
device = "/dev/video2"
zmq_port = 9902
udp_port = 5602
udp_target = "127.0.0.1:6602"
record = true
"#,
    )
    .unwrap();
    let rigs = RigSet::load(&manifest)
        .unwrap()
        .resolve(&manifest, false)
        .unwrap();
    assert!(dir.path().join("left-arena.toml").is_file());
    assert!(dir.path().join("right_arena.toml").is_file());
    let (a, b) = (&rigs[0], &rigs[1]);
    assert_eq!(a.config.instance, "left-arena");
    assert_eq!(b.config.instance, "right arena");
    assert!(matches!(&a.config.source, SourceConfig::Camera { device, .. } if device == "0"));
    assert!(
        matches!(&b.config.source, SourceConfig::Camera { device, .. } if device == "/dev/video2")
    );
    assert_eq!(a.config.output.zmq_bind, "tcp://*:9901");
    assert_eq!(b.config.output.bind, "127.0.0.1:5602");
    assert_eq!(b.config.output.target, "127.0.0.1:6602");
    assert_ne!(
        a.config.tracking.background_path,
        b.config.tracking.background_path
    );
    // The manifest wins for this run only; the rig file keeps its values.
    let saved = Config::load(&b.path).unwrap();
    assert_eq!(saved.output.zmq_bind, "tcp://*:9872");
    let args = child_arguments(b, LaunchOptions::default());
    for expected in [
        "--label",
        "right arena",
        "--device",
        "/dev/video2",
        "--zmq-port",
        "9902",
        "--udp-port",
        "5602",
        "--record",
    ] {
        assert!(args.iter().any(|a| a == expected), "{expected} in {args:?}");
    }
    let demo = child_arguments(
        b,
        LaunchOptions {
            demo: true,
            ..Default::default()
        },
    );
    assert!(demo.contains(&"--demo".to_string()));
    assert!(!demo.contains(&"--device".to_string()), "{demo:?}");
    // Resolving again reuses the created configs.
    let again = RigSet::load(&manifest)
        .unwrap()
        .resolve(&manifest, false)
        .unwrap();
    assert_eq!(again[0].path, rigs[0].path);
}
#[test]
fn shared_cameras_and_labels_are_explained() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("rigs.toml");
    let set = |text: &str| {
        std::fs::write(&manifest, text).unwrap();
        RigSet::load(&manifest)
            .unwrap()
            .resolve(&manifest, false)
            .unwrap_err()
            .to_string()
    };
    let error = set(
        "[[rigs]]\nlabel = \"a\"\ndevice = \"1\"\nzmq_port = 9001\nudp_port = 5001\n\
         [[rigs]]\nlabel = \"b\"\ndevice = \"1\"\nzmq_port = 9002\nudp_port = 5002\n",
    );
    assert!(error.contains("share camera"), "{error}");
    let error = set("[[rigs]]\nlabel = \"a\"\ndevice = \"1\"\nvideo = \"x.avi\"\n");
    assert!(error.contains("either device or video"), "{error}");
    assert!(
        RigSet::load(&manifest).is_ok(),
        "the manifest itself parses"
    );
    std::fs::write(&manifest, "[[rigs]]\ncamera = \"1\"\n").unwrap();
    assert!(
        RigSet::load(&manifest).is_err(),
        "unknown keys are rejected"
    );
}
#[test]
fn a_rig_already_open_elsewhere_is_reported_before_launch() {
    let (dir, rigs) = fixture();
    let manifest = dir.path().join("rigs.toml");
    std::fs::write(&manifest, toml::to_string(&rigs).unwrap()).unwrap();
    let _held = ConfigLock::acquire(&dir.path().join("one.toml")).unwrap();
    let error = rekkefly::rigs::launch(
        &manifest,
        LaunchOptions {
            headless: true,
            demo: true,
            frames: Some(1),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("already open"), "{error}");
}
