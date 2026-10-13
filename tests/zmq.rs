use rekkefly::{
    config::Config,
    frame::GrayFrame,
    output::Publisher,
    tracker::Tracker,
    zmq_output::{KineflyMessage, ZmqPublisher},
};
use std::{
    io::{Read, Write},
    net::TcpStream,
    time::Duration,
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
        let bytes = message.get(0).unwrap();
        assert!(
            bytes.starts_with(br#"{"timestamp_unix": 123.0, "pitch": 0.0, "y": "#),
            "{bytes:?}"
        );
        let value: KineflyMessage = serde_json::from_slice(bytes).unwrap();
        assert_eq!(value.frame_number, 17);
        assert_eq!(value.timestamp_unix, 123.0);
        assert_eq!(value.x, 0.0);
        assert_eq!(value.yaw, 0.0);
    });
    publisher.flush().unwrap();
}

/// The publisher and its port, for tests that speak the wire protocol.
fn publisher() -> (ZmqPublisher, u16) {
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let p = ZmqPublisher::new(format!("tcp://127.0.0.1:{port}")).unwrap();
    assert_eq!(p.local_addr().port(), port);
    (p, port)
}
fn wait_for(p: &ZmqPublisher, subscribers: usize) {
    for _ in 0..200 {
        if p.subscribers() >= subscribers {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("{subscribers} subscribers never connected");
}
/// Reads one ZMTP data frame's body.
fn frame(s: &mut TcpStream) -> Vec<u8> {
    let mut head = [0u8; 2];
    s.read_exact(&mut head).unwrap();
    assert_eq!(head[0] & 0x06, 0, "a short data frame");
    let mut body = vec![0; head[1] as usize];
    s.read_exact(&mut body).unwrap();
    body
}

#[test]
fn a_zmtp_2_subscriber_like_old_netmq_receives_messages() {
    // The bytes NetMQ 4.0.0.1 (bundled with many Unity projects) sends: a
    // ZMTP 2.0 greeting as a SUB, an empty identity and a subscription to
    // everything. A ZMTP 3.0-only publisher delivered nothing to it.
    let (mut p, port) = publisher();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(&[0xff, 0, 0, 0, 0, 0, 0, 0, 1, 0x7f]).unwrap();
    s.write_all(&[0x01, 0x02]).unwrap();
    s.write_all(&[0x00, 0x00, 0x00, 0x01, 0x01]).unwrap();
    let mut greeting = [0u8; 12];
    s.read_exact(&mut greeting).unwrap();
    assert_eq!(greeting[0], 0xff);
    assert_eq!(greeting[9], 0x7f);
    assert_eq!(greeting[11], 1, "socket type PUB");
    let mut identity = [0u8; 2];
    s.read_exact(&mut identity).unwrap();
    assert_eq!(identity, [0, 0]);
    wait_for(&p, 1);
    // The subscription arrives just after the handshake.
    std::thread::sleep(Duration::from_millis(50));
    p.send_bytes(b"{\"x\": 1.0}".to_vec());
    assert_eq!(frame(&mut s), b"{\"x\": 1.0}");
    // Stopping doesn't wait for connected subscribers to leave (on Windows,
    // a thread blocked reading a socket isn't woken by shutting it down).
    let start = std::time::Instant::now();
    p.finish().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    let mut rest = Vec::new();
    s.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty(), "the publisher closed the connection");
}

#[test]
fn a_zmtp_1_subscriber_like_libzmq_2_receives_every_message() {
    // libzmq 2.x sends no greeting, only its identity message (length 1,
    // counting the flags; flags 0; no identity), and filters messages itself
    // instead of subscribing. libzmq publishers, such as the Kinefly Docker
    // bridge, send it everything.
    let (mut p, port) = publisher();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s.write_all(&[0x01, 0x00]).unwrap();
    let mut greeting = [0u8; 10];
    s.read_exact(&mut greeting).unwrap();
    assert_eq!(
        greeting,
        [0xff, 0, 0, 0, 0, 0, 0, 0, 1, 0x7f],
        "the signature, which reads as an empty identity message"
    );
    wait_for(&p, 1);
    p.send_bytes(b"{\"x\": 1.0}".to_vec());
    let mut head = [0u8; 2];
    s.read_exact(&mut head).unwrap();
    assert_eq!(
        head,
        [11, 0],
        "the length, counting the flags, then the flags"
    );
    let mut body = [0u8; 10];
    s.read_exact(&mut body).unwrap();
    assert_eq!(&body, b"{\"x\": 1.0}");
    let long = vec![b'a'; 300];
    p.send_bytes(long.clone());
    let mut head = [0u8; 10];
    s.read_exact(&mut head).unwrap();
    assert_eq!(head[0], 0xff, "a long length");
    assert_eq!(u64::from_be_bytes(head[1..9].try_into().unwrap()), 301);
    assert_eq!(head[9], 0);
    let mut body = vec![0u8; 300];
    s.read_exact(&mut body).unwrap();
    assert_eq!(body, long);
    p.finish().unwrap();
}

#[test]
fn only_matching_subscriptions_receive_and_a_stalled_subscriber_never_blocks() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (mut p, port) = publisher();
    runtime.block_on(async {
        let endpoint = format!("tcp://127.0.0.1:{port}");
        let mut wanted = zeromq::SubSocket::new();
        wanted.connect(&endpoint).await.unwrap();
        wanted.subscribe("{\"timestamp_unix\"").await.unwrap();
        let mut other = zeromq::SubSocket::new();
        other.connect(&endpoint).await.unwrap();
        other.subscribe("nothing").await.unwrap();
        // A raw ZMTP 2.0 subscriber that never reads.
        let mut stalled = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stalled
            .write_all(&[
                0xff, 0, 0, 0, 0, 0, 0, 0, 1, 0x7f, 0x01, 0x02, 0, 0, 0, 1, 1,
            ])
            .unwrap();
        wait_for(&p, 3);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let message = KineflyMessage {
            x: 0.25,
            y: -0.5,
            z: 0.0,
            yaw: 0.75,
            pitch: 0.0,
            roll: 0.0,
            frame_number: 1,
            timestamp_unix: 2.5,
        };
        // Far more than the stalled peer's queue and socket buffers hold. A
        // publisher that waited for it would never finish this loop.
        let bytes = message.to_json().into_bytes();
        let start = std::time::Instant::now();
        for _ in 0..200_000 {
            p.send_bytes(bytes.clone());
        }
        assert!(
            start.elapsed() < Duration::from_secs(30),
            "sending never waits"
        );
        assert!(p.dropped > 0, "the stalled subscriber missed messages");
        let got = tokio::time::timeout(Duration::from_secs(2), wanted.recv())
            .await
            .expect("the matching subscriber receives")
            .unwrap();
        let value: KineflyMessage = serde_json::from_slice(got.get(0).unwrap()).unwrap();
        assert_eq!(value, message);
        assert!(
            tokio::time::timeout(Duration::from_millis(300), other.recv())
                .await
                .is_err(),
            "a subscription to another topic receives nothing"
        );
        drop(stalled);
    });
    p.finish().unwrap();
}

#[test]
fn finishing_closes_subscribers_that_never_finish_the_handshake() {
    let (mut p, port) = publisher();
    let _silent = TcpStream::connect(("127.0.0.1", port)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let start = std::time::Instant::now();
    p.finish().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
}
