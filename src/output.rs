use crate::{
    config::{Config, OutputConfig},
    measurement_log::MeasurementLog,
    tracker::Measurement,
    zmq_output::ZmqPublisher,
};
use anyhow::{Context, Result};
use std::{
    net::{SocketAddr, UdpSocket},
    path::Path,
};

pub struct Publisher {
    socket: Option<UdpSocket>,
    target: Option<SocketAddr>,
    recording: Option<MeasurementLog>,
    zmq: Option<ZmqPublisher>,
}
impl Publisher {
    pub fn new(config: &Config, path: &Path) -> Result<Self> {
        let OutputConfig {
            enabled,
            bind,
            target,
            jsonl_path,
            ..
        } = &config.output;
        let socket = if *enabled {
            let socket = UdpSocket::bind(bind).with_context(|| {
                format!("Bind output port {bind}; choose a unique sender port for each instance")
            })?;
            socket.set_nonblocking(true)?;
            Some(socket)
        } else {
            None
        };
        let target = if *enabled {
            Some(target.parse()?)
        } else {
            None
        };
        let recording = if jsonl_path.is_empty() {
            None
        } else {
            let p = config.resolve(path, jsonl_path);
            if let Some(parent) = p.parent().filter(|p| !p.as_os_str().is_empty()) {
                std::fs::create_dir_all(parent)?;
            }
            Some(MeasurementLog::new(&p)?)
        };
        Ok(Self {
            socket,
            target,
            recording,
            zmq: if config.output.zmq_enabled {
                Some(ZmqPublisher::new(config.output.zmq_bind.clone())?)
            } else {
                None
            },
        })
    }
    pub fn send(&mut self, measurement: &Measurement) -> Result<bool> {
        if let Some(zmq) = &mut self.zmq {
            zmq.send(measurement)?;
        }
        let json = if self.socket.is_some() || self.recording.is_some() {
            serde_json::to_vec(measurement)?
        } else {
            Vec::new()
        };
        let mut sent = true;
        if let (Some(socket), Some(target)) = (&self.socket, self.target) {
            match socket.send_to(&json, target) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => sent = false,
                Err(e) => return Err(e).context("Send measurement over UDP"),
            }
        }
        if let Some(recording) = &self.recording {
            recording.send(json)?;
        }

        Ok(sent)
    }
    pub fn flush(&mut self) -> Result<()> {
        if let Some(w) = &mut self.recording {
            w.finish()?;
        }
        if let Some(zmq) = &mut self.zmq {
            zmq.finish()?;
        }
        Ok(())
    }
    pub fn zmq_dropped(&self) -> u64 {
        self.zmq.as_ref().map_or(0, |z| z.dropped)
    }
}
