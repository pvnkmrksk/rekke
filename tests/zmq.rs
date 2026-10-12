use rekkefly::{
    config::Config, frame::GrayFrame, output::Publisher, tracker::Tracker,
    zmq_output::KineflyMessage,
};
use zeromq::{Socket, SocketRecv};
#[test]
fn legacy_bridge_fields_first_edge_zero_fallback_and_unix_timestamp() {
    let config = Config::demo();
    let frame =
        GrayFrame::new(640, 480, vec![0; 640 * 480], 1_700_000_000_125_000_000, 77).unwrap();
    let mut measurement = Tracker::new(&config, 640, 480)
        .process(&frame, None, 0)
        .unwrap();
    measurement.left.angles_rad = vec![0.25, 0.6];
    measurement.left.angle_rad = Some(0.6);
    measurement.right.angles_rad = vec![-0.5];
    let value = KineflyMessage::from(&measurement);
    assert_eq!(value.x, 0.25);
    assert_eq!(value.y, -0.5);
    assert_eq!(value.yaw, 0.75);
    assert_eq!(value.frame_number, 77);
    assert_eq!(value.timestamp_unix, 1_700_000_000.125);
    assert_eq!(
        serde_json::to_value(&value)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        8
    );
    measurement.left.angles_rad.clear();
    assert_eq!(KineflyMessage::from(&measurement).x, 0.0);
}
#[test]
fn zmq_pub_sends_one_utf8_json_frame_and_rejects_port_conflicts() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut config = Config::demo();
    config.output.enabled = false;
    config.output.zmq_enabled = true;
    config.output.zmq_bind = format!("tcp://*:{port}");
    let mut publisher = Publisher::new(&config, std::path::Path::new("unused.toml")).unwrap();
    assert!(Publisher::new(&config, std::path::Path::new("unused.toml")).is_err());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut subscriber = zeromq::SubSocket::new();
        subscriber
            .connect(&format!("tcp://127.0.0.1:{port}"))
            .await
            .unwrap();
        subscriber.subscribe("").await.unwrap();
        let frame = GrayFrame::new(640, 480, vec![0; 640 * 480], 123_000_000_000, 17).unwrap();
        let m = Tracker::new(&config, 640, 480)
            .process(&frame, None, 0)
            .unwrap();
        let mut received = None;
        for _ in 0..20 {
            publisher.send(&m).unwrap();
            if let Ok(Ok(message)) =
                tokio::time::timeout(std::time::Duration::from_millis(100), subscriber.recv()).await
            {
                received = Some(message);
                break;
            }
        }
        let message = received.expect("subscriber should receive a published frame");
        assert_eq!(message.len(), 1);
        let value: KineflyMessage = serde_json::from_slice(message.get(0).unwrap()).unwrap();
        assert_eq!(value.frame_number, 17);
        assert_eq!(value.timestamp_unix, 123.0);
        assert_eq!(value.x, 0.0);
        assert_eq!(value.yaw, 0.0);
    });
    publisher.flush().unwrap();
}
