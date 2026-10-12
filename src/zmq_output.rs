use crate::tracker::Measurement;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::Duration,
};
use zeromq::{Socket, SocketSend};

/// Exactly ros_zmq_bridge.py's active, single-frame JSON contract.
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
pub struct ZmqPublisher {
    sender: Option<tokio::sync::mpsc::Sender<Vec<u8>>>,
    thread: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
    pub dropped: u64,
}
impl ZmqPublisher {
    pub fn new(endpoint: String) -> Result<Self> {
        // libzmq accepts '*' for an all-interface IPv4 PUB bind. zmq.rs
        // parses it as a DNS name, so normalize without changing saved config.
        let endpoint = endpoint
            .strip_prefix("tcp://*:")
            .map(|port| format!("tcp://0.0.0.0:{port}"))
            .unwrap_or(endpoint);
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<Vec<u8>>(256);
        let (ready, opened) = mpsc::sync_channel(1);
        let error = Arc::new(Mutex::new(None));
        let shared = error.clone();
        let thread = thread::Builder::new()
            .name("zmq-publisher".into())
            .spawn(move || {
                let result: Result<()> = (|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()?;
                    runtime.block_on(async {
                        let mut socket = zeromq::PubSocket::new();
                        if let Err(e) = socket.bind(&endpoint).await {
                            let message =
                                format!("Bind ZMQ {endpoint}; use a unique port per instance: {e}");
                            let _ = ready.send(Err(message.clone()));
                            bail!(message);
                        }
                        let _ = ready.send(Ok(()));
                        while let Some(json) = receiver.recv().await {
                            // A stalled peer must never stall acquisition or shutdown.
                            // On timeout discard the socket, rather than reuse a
                            // potentially partially written ZMTP frame.
                            tokio::time::timeout(
                                Duration::from_millis(500),
                                socket.send(json.into()),
                            )
                            .await
                            .context("ZMQ subscriber stalled for 500 ms; publisher stopped")??;
                        }
                        Ok(())
                    })
                })();
                if let Err(e) = result {
                    *shared.lock().unwrap() = Some(format!("{e:#}"));
                }
            })?;
        match opened.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => Ok(Self {
                sender: Some(sender),
                thread: Some(thread),
                error,
                dropped: 0,
            }),
            result => {
                drop(sender);
                let _ = thread.join();
                bail!("ZMQ startup: {:?}", result);
            }
        }
    }
    pub fn send(&mut self, measurement: &Measurement) -> Result<()> {
        if let Some(error) = self.error.lock().unwrap().as_ref() {
            bail!("ZMQ publisher: {error}");
        }
        let json = serde_json::to_vec(&KineflyMessage::from(measurement))?;
        match self.sender.as_ref().unwrap().try_send(json) {
            Ok(()) => Ok(()),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                self.dropped += 1;
                Ok(())
            }
            Err(e) => Err(e).context("ZMQ worker stopped"),
        }
    }
    pub fn finish(&mut self) -> Result<()> {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("ZMQ worker panicked"))?;
        }
        if let Some(error) = self.error.lock().unwrap().as_ref() {
            bail!("{error}");
        }
        Ok(())
    }
}
impl Drop for ZmqPublisher {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
