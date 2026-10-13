//! Kinefly-compatible ZMQ output.
//!
//! A PUB socket that speaks the ZeroMQ wire protocol the way libzmq does, so
//! every subscriber that worked with the Kinefly Docker bridge (pyzmq on
//! libzmq) works here too. That includes NetMQ releases that still speak
//! ZMTP 2.0, such as the NetMQ 4.0.0.x bundled with many Unity projects: a
//! ZMTP 3.0-only publisher gets no messages through to them. The handshake
//! follows libzmq's: send the signature, read the peer's, then continue in
//! ZMTP 3.0 (RFC 23, NULL mechanism) or fall back to ZMTP 2.0 (RFC 15).
//!
//! Each message is one frame of JSON, byte for byte what the bridge's
//! `json.dumps` produced under Python 2: the same key order, `", "` and
//! `": "` separators and Python's number format. Subscribers' topic
//! prefixes are honoured. Tracking never waits: like a libzmq PUB socket, a
//! subscriber more than `HIGH_WATER_MARK` messages behind misses messages.
use crate::tracker::Measurement;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Messages queued per subscriber before it misses some (libzmq's default).
pub const HIGH_WATER_MARK: usize = 1000;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Larger frames from a subscriber are refused; subscriptions are tiny.
const MAX_INCOMING_FRAME: u64 = 1 << 20;

/// The bridge's message (ros_zmq_bridge.py): left and right wing angles in
/// radians, their difference, and the frame's number and arrival time.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct KineflyMessage {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub roll: f64,
    pub frame_number: u64,
    pub timestamp_unix: f64,
}
impl From<&Measurement> for KineflyMessage {
    fn from(m: &Measurement) -> Self {
        // The bridge reads angles[0], not a separately selected primary angle.
        let x = m.left.angles_rad.first().copied().unwrap_or(0.0) as f64;
        let y = m.right.angles_rad.first().copied().unwrap_or(0.0) as f64;
        Self {
            x,
            y,
            z: 0.0,
            yaw: x - y,
            pitch: 0.0,
            roll: 0.0,
            frame_number: m.sequence,
            timestamp_unix: (m.timestamp_ns / 1_000_000_000) as f64
                + (m.timestamp_ns % 1_000_000_000) as f64 / 1e9,
        }
    }
}
impl KineflyMessage {
    /// The JSON exactly as the bridge sent it. Python 2 orders a dict's keys
    /// by their hashes, which for these eight keys built in the bridge's
    /// order gives the order below.
    pub fn to_json(&self) -> String {
        format!(
            "{{\"timestamp_unix\": {}, \"pitch\": {}, \"y\": {}, \"x\": {}, \"yaw\": {}, \"z\": {}, \"roll\": {}, \"frame_number\": {}}}",
            python_float(self.timestamp_unix),
            python_float(self.pitch),
            python_float(self.y),
            python_float(self.x),
            python_float(self.yaw),
            python_float(self.z),
            python_float(self.roll),
            self.frame_number
        )
    }
}

/// A float as Python's `repr` (and so `json.dumps`) writes it: the shortest
/// digits that read back to the same value; fixed notation with at least one
/// decimal from 1e-4 up to 1e16, exponent notation such as `1e-05` or
/// `1.5e+16` outside it.
pub fn python_float(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // Rust's `{:e}` gives the shortest round-trip digits, as Python does.
    let scientific = format!("{:e}", x.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("{:e} has an exponent");
    let exponent: i32 = exponent.parse().expect("{:e} exponent is an integer");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if !(-4..16).contains(&exponent) {
        let mut out = format!("{sign}{}", &digits[..1]);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let sign = if exponent < 0 { '-' } else { '+' };
        out.push_str(&format!("e{sign}{:02}", exponent.abs()));
        out
    } else if exponent < 0 {
        format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize))
    } else {
        let point = exponent as usize + 1;
        if digits.len() > point {
            format!("{sign}{}.{}", &digits[..point], &digits[point..])
        } else {
            format!("{sign}{digits}{}.0", "0".repeat(point - digits.len()))
        }
    }
}

/// `tcp://HOST:PORT` as libzmq accepts it: HOST is `*` (every IPv4
/// interface), an IP address, `[IPv6]` or a host name; PORT is nonzero.
pub fn parse_endpoint(endpoint: &str) -> Result<(String, u16)> {
    let rest = endpoint
        .strip_prefix("tcp://")
        .with_context(|| format!("ZMQ endpoint {endpoint:?} must start with tcp://"))?;
    let (host, port) = rest
        .rsplit_once(':')
        .with_context(|| format!("ZMQ endpoint {endpoint:?} needs a port: tcp://*:9871"))?;
    let port: u16 = port
        .parse()
        .ok()
        .filter(|p| *p > 0)
        .with_context(|| format!("ZMQ endpoint {endpoint:?} needs a port from 1 to 65535"))?;
    let host = host.trim_start_matches('[').trim_end_matches(']');
    ensure!(
        !host.is_empty(),
        "ZMQ endpoint {endpoint:?} needs a host, such as *"
    );
    Ok((host.to_owned(), port))
}

/// The socket types that may subscribe, in ZMTP 2.0 numbers and 3.0 names.
const PUB: u8 = 1;
const SUB: u8 = 2;
const XSUB: u8 = 10;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Protocol {
    /// ZMTP 3.0: frames may be commands.
    V3,
    /// ZMTP 2.0: data frames only.
    V2,
}

enum Outgoing {
    Data(Arc<[u8]>),
    /// A ZMTP 3 command, such as PONG.
    Command(Vec<u8>),
}

struct Peer {
    queue: SyncSender<Outgoing>,
    topics: Mutex<Vec<Vec<u8>>>,
    alive: AtomicBool,
}
impl Peer {
    fn wants(&self, message: &[u8]) -> bool {
        self.topics
            .lock()
            .unwrap()
            .iter()
            .any(|t| message.starts_with(t))
    }
}

#[derive(Default)]
struct Shared {
    peers: Mutex<Vec<Arc<Peer>>>,
    /// Every open connection, handshaking or not, so finish() can close them.
    connections: Mutex<HashMap<u64, TcpStream>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
    stop: AtomicBool,
}

pub struct ZmqPublisher {
    shared: Arc<Shared>,
    acceptor: Option<JoinHandle<()>>,
    address: SocketAddr,
    /// Messages a subscriber missed because it fell behind.
    pub dropped: u64,
}
impl ZmqPublisher {
    pub fn new(endpoint: String) -> Result<Self> {
        let (host, port) = parse_endpoint(&endpoint)?;
        let host = if host == "*" {
            "0.0.0.0".to_owned()
        } else {
            host
        };
        let addresses: Vec<SocketAddr> = (host.as_str(), port)
            .to_socket_addrs()
            .with_context(|| format!("Resolve ZMQ endpoint {endpoint}"))?
            .collect();
        let listener = TcpListener::bind(&addresses[..])
            .with_context(|| format!("Bind ZMQ {endpoint}; use a unique port per instance"))?;
        let address = listener.local_addr()?;
        // Polling accept lets finish() stop it without a wake-up connection.
        listener.set_nonblocking(true)?;
        let shared = Arc::new(Shared::default());
        let state = shared.clone();
        let acceptor = thread::Builder::new()
            .name("zmq-accept".into())
            .spawn(move || accept(listener, state))?;
        Ok(Self {
            shared,
            acceptor: Some(acceptor),
            address,
            dropped: 0,
        })
    }
    /// Where the publisher listens (the port, when bound to port 0 in tests).
    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }
    pub fn send(&mut self, measurement: &Measurement) -> Result<()> {
        self.send_bytes(KineflyMessage::from(measurement).to_json().into_bytes());
        Ok(())
    }
    /// Queues one message for every subscriber whose topics match; never blocks.
    pub fn send_bytes(&mut self, message: Vec<u8>) {
        let message: Arc<[u8]> = message.into();
        let mut peers = self.shared.peers.lock().unwrap();
        peers.retain(|p| p.alive.load(Ordering::Relaxed));
        for peer in peers.iter().filter(|p| p.wants(&message)) {
            match peer.queue.try_send(Outgoing::Data(message.clone())) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => self.dropped += 1,
                Err(TrySendError::Disconnected(_)) => peer.alive.store(false, Ordering::Relaxed),
            }
        }
    }
    /// Subscribers connected and past the handshake.
    pub fn subscribers(&self) -> usize {
        let peers = self.shared.peers.lock().unwrap();
        peers
            .iter()
            .filter(|p| p.alive.load(Ordering::Relaxed))
            .count()
    }
    pub fn finish(&mut self) -> Result<()> {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(acceptor) = self.acceptor.take() {
            acceptor
                .join()
                .map_err(|_| anyhow::anyhow!("ZMQ acceptor panicked"))?;
        }
        for peer in self.shared.peers.lock().unwrap().drain(..) {
            peer.alive.store(false, Ordering::Relaxed);
        }
        for (_, stream) in self.shared.connections.lock().unwrap().drain() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        let threads: Vec<JoinHandle<()>> = self.shared.threads.lock().unwrap().drain(..).collect();
        for t in threads {
            let _ = t.join();
        }
        Ok(())
    }
}
impl Drop for ZmqPublisher {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

fn accept(listener: TcpListener, shared: Arc<Shared>) {
    let mut next = 0u64;
    while !shared.stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                // macOS and Windows hand out sockets in the listener's
                // non-blocking mode.
                let Ok(handle) = stream.set_nonblocking(false).and(stream.try_clone()) else {
                    continue;
                };
                let id = next;
                next += 1;
                shared.connections.lock().unwrap().insert(id, handle);
                let state = shared.clone();
                let mut threads = shared.threads.lock().unwrap();
                threads.retain(|t| !t.is_finished());
                if let Ok(thread) =
                    thread::Builder::new()
                        .name("zmq-subscriber".into())
                        .spawn(move || {
                            serve(stream, &state);
                            state.connections.lock().unwrap().remove(&id);
                        })
                {
                    threads.push(thread);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => thread::sleep(Duration::from_millis(25)),
        }
    }
}

/// One subscriber: the handshake, then this thread reads its subscriptions
/// while a second one writes its messages.
fn serve(stream: TcpStream, shared: &Shared) {
    let Ok(protocol) = handshake(&stream) else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let _ = stream.set_read_timeout(None);
    let (queue, outgoing) = sync_channel(HIGH_WATER_MARK);
    let peer = Arc::new(Peer {
        queue,
        topics: Mutex::new(Vec::new()),
        alive: AtomicBool::new(true),
    });
    {
        // Checked under the lock: finish() sets stop before it empties the list.
        let mut peers = shared.peers.lock().unwrap();
        if shared.stop.load(Ordering::Relaxed) {
            return;
        }
        peers.push(peer.clone());
    }
    let writing = peer.clone();
    let Ok(write_thread) = thread::Builder::new()
        .name("zmq-write".into())
        .spawn(move || write_loop(writer, outgoing, writing))
    else {
        peer.alive.store(false, Ordering::Relaxed);
        return;
    };
    read_loop(&stream, protocol, &peer);
    peer.alive.store(false, Ordering::Relaxed);
    let _ = stream.shutdown(Shutdown::Both);
    // Wake the writer if it waits on an empty queue.
    let _ = peer.queue.try_send(Outgoing::Command(Vec::new()));
    let _ = write_thread.join();
}

fn write_loop(mut stream: TcpStream, outgoing: Receiver<Outgoing>, peer: Arc<Peer>) {
    while let Ok(item) = outgoing.recv() {
        if !peer.alive.load(Ordering::Relaxed) {
            break;
        }
        let ok = match item {
            Outgoing::Data(body) => write_frame(&mut stream, 0, &body).is_ok(),
            Outgoing::Command(body) if body.is_empty() => true,
            Outgoing::Command(body) => write_frame(&mut stream, COMMAND, &body).is_ok(),
        };
        if !ok {
            peer.alive.store(false, Ordering::Relaxed);
            let _ = stream.shutdown(Shutdown::Both);
            break;
        }
    }
}

/// Subscriptions arrive as messages (`\x01topic`, `\x00topic` to cancel) or,
/// from ZMTP 3.1 peers, as SUBSCRIBE and CANCEL commands.
fn read_loop(mut stream: &TcpStream, protocol: Protocol, peer: &Peer) {
    while let Ok((flags, body)) = read_frame(&mut stream) {
        if flags & COMMAND != 0 && protocol == Protocol::V3 {
            let Some((name, data)) = command(&body) else {
                continue;
            };
            match name {
                b"SUBSCRIBE" => subscribe(peer, data, true),
                b"CANCEL" => subscribe(peer, data, false),
                b"PING" if data.len() >= 2 => {
                    let mut pong = vec![4];
                    pong.extend_from_slice(b"PONG");
                    pong.extend_from_slice(&data[2..]);
                    let _ = peer.queue.try_send(Outgoing::Command(pong));
                }
                _ => {}
            }
        } else if let Some((&kind, topic)) = body.split_first() {
            match kind {
                1 => subscribe(peer, topic, true),
                0 => subscribe(peer, topic, false),
                _ => {}
            }
        }
    }
}

fn subscribe(peer: &Peer, topic: &[u8], add: bool) {
    let mut topics = peer.topics.lock().unwrap();
    if add {
        topics.push(topic.to_vec());
    } else if let Some(i) = topics.iter().position(|t| t == topic) {
        topics.swap_remove(i);
    }
}

const MORE: u8 = 0x01;
const LONG: u8 = 0x02;
const COMMAND: u8 = 0x04;

fn write_frame(stream: &mut impl Write, flags: u8, body: &[u8]) -> std::io::Result<()> {
    let mut header = Vec::with_capacity(9);
    if body.len() < 256 {
        header.extend([flags, body.len() as u8]);
    } else {
        header.push(flags | LONG);
        header.extend((body.len() as u64).to_be_bytes());
    }
    let mut frame = header;
    frame.extend_from_slice(body);
    stream.write_all(&frame)
}

fn read_frame(stream: &mut impl Read) -> Result<(u8, Vec<u8>)> {
    let mut flags = [0u8];
    stream.read_exact(&mut flags)?;
    let size = if flags[0] & LONG != 0 {
        let mut size = [0u8; 8];
        stream.read_exact(&mut size)?;
        u64::from_be_bytes(size)
    } else {
        let mut size = [0u8];
        stream.read_exact(&mut size)?;
        size[0] as u64
    };
    ensure!(
        size <= MAX_INCOMING_FRAME,
        "ZMQ frame of {size} bytes refused"
    );
    let mut body = vec![0u8; size as usize];
    stream.read_exact(&mut body)?;
    Ok((flags[0] & !MORE, body))
}

/// A ZMTP 3 command's name and data.
fn command(body: &[u8]) -> Option<(&[u8], &[u8])> {
    let (&length, rest) = body.split_first()?;
    (rest.len() >= length as usize).then(|| rest.split_at(length as usize))
}

/// libzmq's handshake, as the publishing side: ZMTP 3.0 with the NULL
/// mechanism when the peer supports it, else ZMTP 2.0. ZMTP 1.0 peers
/// (libzmq 2.x, no signature) are refused.
fn handshake(stream: &TcpStream) -> Result<Protocol> {
    let mut s = stream;
    stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT))?;
    // Matches the latency of libzmq, which turns Nagle's algorithm off.
    stream.set_nodelay(true)?;
    s.write_all(&[0xff, 0, 0, 0, 0, 0, 0, 0, 1, 0x7f])?;
    let mut signature = [0u8; 10];
    s.read_exact(&mut signature)?;
    ensure!(
        signature[0] == 0xff && signature[9] & 1 == 1,
        "ZMTP 1.0 peers are not supported"
    );
    s.write_all(&[3])?;
    let mut revision = [0u8];
    s.read_exact(&mut revision)?;
    // Revision 1 is ZMTP 2.0; 0 would need ZMTP 1.0 framing.
    ensure!(revision[0] >= 1, "ZMTP 1.0 peers are not supported");
    if revision[0] >= 3 {
        // ZMTP 3.0: minor version, mechanism, as-server, filler.
        let mut rest = [0u8; 53];
        rest[1..5].copy_from_slice(b"NULL");
        s.write_all(&rest)?;
        let mut theirs = [0u8; 53];
        s.read_exact(&mut theirs)?;
        ensure!(
            &theirs[1..5] == b"NULL" && theirs[5..21].iter().all(|&b| b == 0),
            "Only the NULL security mechanism is supported"
        );
        let mut ready = vec![5];
        ready.extend_from_slice(b"READY");
        ready.push(11);
        ready.extend_from_slice(b"Socket-Type");
        ready.extend_from_slice(&3u32.to_be_bytes());
        ready.extend_from_slice(b"PUB");
        write_frame(&mut s, COMMAND, &ready)?;
        let (flags, body) = read_frame(&mut s)?;
        ensure!(flags & COMMAND != 0, "Expected a READY command");
        let (name, mut properties) = command(&body).context("Malformed READY")?;
        ensure!(name == b"READY", "Expected a READY command");
        while !properties.is_empty() {
            let (key, rest) = command(properties).context("Malformed READY property")?;
            ensure!(rest.len() >= 4, "Malformed READY property");
            let length = u32::from_be_bytes(rest[..4].try_into().unwrap()) as usize;
            ensure!(rest.len() >= 4 + length, "Malformed READY property");
            let value = &rest[4..4 + length];
            if key.eq_ignore_ascii_case(b"Socket-Type") {
                ensure!(
                    value == b"SUB" || value == b"XSUB",
                    "Only SUB and XSUB sockets may connect to a PUB socket"
                );
            }
            properties = &rest[4 + length..];
        }
        Ok(Protocol::V3)
    } else {
        // ZMTP 2.0: socket types, then each side's identity frame.
        s.write_all(&[PUB])?;
        let mut kind = [0u8];
        s.read_exact(&mut kind)?;
        ensure!(
            kind[0] == SUB || kind[0] == XSUB,
            "Only SUB and XSUB sockets may connect to a PUB socket"
        );
        s.write_all(&[0, 0])?;
        read_frame(&mut s)?;
        Ok(Protocol::V2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn floats_are_written_as_python_writes_them() {
        // Expected strings from Python's json.dumps.
        for (value, python) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (0.25, "0.25"),
            (-0.5, "-0.5"),
            (0.695575475692749, "0.695575475692749"),
            (1e-05, "1e-05"),
            (3.2e-05, "3.2e-05"),
            (-4.5e-05, "-4.5e-05"),
            (0.0001, "0.0001"),
            (0.00012345, "0.00012345"),
            (1e15, "1000000000000000.0"),
            (1e16, "1e+16"),
            (1.5e16, "1.5e+16"),
            (123456789012345680.0, "1.2345678901234568e+17"),
            (1791899197.2829123, "1791899197.2829123"),
            (1700000000.125, "1700000000.125"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (0.1 + 0.2, "0.30000000000000004"),
            (12.0, "12.0"),
            (100.0, "100.0"),
        ] {
            assert_eq!(python_float(value), python, "{value:e}");
        }
    }
    #[test]
    fn the_message_is_the_bridges_bytes() {
        let m = KineflyMessage {
            x: 0.25,
            y: -0.5,
            z: 0.0,
            yaw: 0.75,
            pitch: 0.0,
            roll: 0.0,
            frame_number: 77,
            timestamp_unix: 1700000000.125,
        };
        // Python 2.7: json.dumps({"x": 0.25, "y": -0.5, "z": 0.0, "yaw": 0.75,
        // "pitch": 0.0, "roll": 0.0, "frame_number": 77,
        // "timestamp_unix": 1700000000.125})
        assert_eq!(
            m.to_json(),
            r#"{"timestamp_unix": 1700000000.125, "pitch": 0.0, "y": -0.5, "x": 0.25, "yaw": 0.75, "z": 0.0, "roll": 0.0, "frame_number": 77}"#
        );
        let parsed: KineflyMessage = serde_json::from_str(&m.to_json()).unwrap();
        assert_eq!(parsed, m);
    }
    #[test]
    fn endpoints_are_parsed_like_libzmq() {
        assert_eq!(parse_endpoint("tcp://*:9871").unwrap(), ("*".into(), 9871));
        assert_eq!(
            parse_endpoint("tcp://127.0.0.1:5").unwrap(),
            ("127.0.0.1".into(), 5)
        );
        assert_eq!(
            parse_endpoint("tcp://[::1]:9871").unwrap(),
            ("::1".into(), 9871)
        );
        for bad in [
            "udp://*:9871",
            "tcp://*",
            "tcp://*:0",
            "tcp://*:70000",
            "tcp://:9871",
        ] {
            assert!(parse_endpoint(bad).is_err(), "{bad}");
        }
    }
}
