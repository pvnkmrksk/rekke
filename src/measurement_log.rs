use anyhow::{Context, Result, bail};
use std::{
    fs::OpenOptions,
    io::{BufWriter, Write},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub struct MeasurementLog {
    sender: Option<mpsc::SyncSender<Vec<u8>>>,
    thread: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}
impl MeasurementLog {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| {
                format!(
                    "Create measurement recording {} (choose a new filename)",
                    path.display()
                )
            })?;
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(128);
        let error = Arc::new(Mutex::new(None));
        let shared = error.clone();
        let thread = thread::Builder::new()
            .name("measurement-log".into())
            .spawn(move || {
                let mut writer = BufWriter::new(file);
                let result: Result<()> = (|| {
                    let mut last_flush = Instant::now();
                    loop {
                        match receiver.recv_timeout(Duration::from_secs(1)) {
                            Ok(json) => {
                                writer.write_all(&json)?;
                                writer.write_all(b"\n")?;
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                        if last_flush.elapsed() >= Duration::from_secs(1) {
                            writer.flush()?;
                            last_flush = Instant::now();
                        }
                    }
                    writer.flush()?;
                    Ok(())
                })();
                if let Err(e) = result {
                    *shared.lock().unwrap() = Some(format!("Measurement log: {e:#}"));
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            thread: Some(thread),
            error,
        })
    }
    pub fn send(&self, json: Vec<u8>) -> Result<()> {
        if let Some(error) = self.error.lock().unwrap().as_ref() {
            bail!("{error}");
        }
        self.sender.as_ref().context("Measurement log stopped")?.try_send(json).context("Measurement log queue full or writer failed; stopping acquisition rather than silently losing measurements")?;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<()> {
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("Measurement log worker panicked"))?;
        }
        if let Some(error) = self.error.lock().unwrap().as_ref() {
            bail!("{error}");
        }
        Ok(())
    }
}
impl Drop for MeasurementLog {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}
