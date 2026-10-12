use crate::config::Config;
use std::{
    path::PathBuf,
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

/// Coalesce drag events and perform durable atomic writes away from the GUI.
enum SaveRequest {
    Later(Box<Config>),
    Now(Box<Config>),
}
impl SaveRequest {
    fn parts(self) -> (Config, bool) {
        match self {
            Self::Later(config) => (*config, false),
            Self::Now(config) => (*config, true),
        }
    }
}
pub struct Autosaver {
    sender: Option<mpsc::Sender<SaveRequest>>,
    pub results: mpsc::Receiver<Result<Config, String>>,
    thread: Option<JoinHandle<()>>,
}
impl Autosaver {
    pub fn new(path: PathBuf) -> Self {
        let (sender, receiver) = mpsc::channel::<SaveRequest>();
        let (result, results) = mpsc::channel();
        let thread = thread::spawn(move || {
            while let Ok(request) = receiver.recv() {
                let (mut config, immediate) = request.parts();
                let disconnected = if immediate {
                    false
                } else {
                    loop {
                        match receiver.recv_timeout(Duration::from_millis(300)) {
                            Ok(next) => {
                                let (next, immediate) = next.parts();
                                config = next;
                                if immediate {
                                    break false;
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => break false,
                            Err(mpsc::RecvTimeoutError::Disconnected) => break true,
                        }
                    }
                };
                let saved = config
                    .save(&path)
                    .map(|_| config)
                    .map_err(|e| format!("Autosave: {e:#}"));
                let _ = result.send(saved);
                if disconnected {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            results,
            thread: Some(thread),
        }
    }
    pub fn save_now(&self, config: Config) {
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .send(SaveRequest::Now(Box::new(config)));
    }
    pub fn submit(&self, config: Config) {
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .send(SaveRequest::Later(Box::new(config)));
    }
}
impl Drop for Autosaver {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
