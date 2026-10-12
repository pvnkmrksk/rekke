use anyhow::{Context, Result, ensure};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct GrayFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub timestamp_ns: u64,
    pub sequence: u64,
}
impl GrayFrame {
    pub fn new(
        width: u32,
        height: u32,
        pixels: Vec<u8>,
        timestamp_ns: u64,
        sequence: u64,
    ) -> Result<Self> {
        ensure!(
            pixels.len() == width as usize * height as usize,
            "Frame length does not match dimensions"
        );
        Ok(Self {
            width,
            height,
            pixels,
            timestamp_ns,
            sequence,
        })
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        use image::ImageEncoder;
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes).write_image(
            &self.pixels,
            self.width,
            self.height,
            image::ExtendedColorType::L8,
        )?;
        crate::config::atomic_write(path, &bytes).context("Save background PNG")
    }
    pub fn load(path: &Path) -> Result<Self> {
        let img = image::open(path)
            .context("Read background image")?
            .to_luma8();
        Self::new(img.width(), img.height(), img.into_raw(), 0, 0)
    }
    pub fn difference(&self, bg: &Self) -> Result<Self> {
        ensure!(
            self.width == bg.width && self.height == bg.height,
            "Background dimensions differ from camera frame"
        );
        let pixels = self
            .pixels
            .iter()
            .zip(&bg.pixels)
            .map(|(a, b)| a.abs_diff(*b))
            .collect();
        Self::new(
            self.width,
            self.height,
            pixels,
            self.timestamp_ns,
            self.sequence,
        )
    }
}
pub fn unix_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}
