use rekke::{
    config::Config,
    rigs::{RigEntry, RigSet},
};
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
        rigs: vec![
            RigEntry {
                config: "one.toml".into(),
                enabled: true,
                record: false,
            },
            RigEntry {
                config: "two.toml".into(),
                enabled: true,
                record: true,
            },
        ],
    };
    (dir, rigs)
}
#[test]
fn manifest_maps_independent_paths_ports_and_record_flags() {
    let (dir, rigs) = fixture();
    let resolved = rigs.resolve(&dir.path().join("rigs.toml"), false).unwrap();
    assert_eq!(resolved.len(), 2);
    assert!(resolved[0].0.is_absolute());
    assert!(resolved[1].2);
    assert_ne!(resolved[0].1.output.zmq_bind, resolved[1].1.output.zmq_bind);
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
    rigs.rigs[1].config = "one.toml".into();
    assert!(rigs.resolve(&dir.path().join("rigs.toml"), false).is_err());
    rigs.rigs[1].enabled = false;
    assert_eq!(
        rigs.resolve(&dir.path().join("rigs.toml"), false)
            .unwrap()
            .len(),
        1
    );
}
