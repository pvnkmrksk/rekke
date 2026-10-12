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
/// Scale an image into `width`×`height` without changing its aspect ratio,
/// centering it and padding with its mean border value. Distorting the aspect
/// would distort every measured angle.
pub fn fit(pixels: Vec<u8>, from: (u32, u32), width: u32, height: u32) -> Result<Vec<u8>> {
    let (sw, sh) = from;
    ensure!(
        pixels.len() == sw as usize * sh as usize && sw > 0 && sh > 0,
        "Frame length does not match dimensions"
    );
    if (sw, sh) == (width, height) {
        return Ok(pixels);
    }
    let scale = (width as f64 / sw as f64).min(height as f64 / sh as f64);
    let fw = ((sw as f64 * scale).round() as u32).clamp(1, width);
    let fh = ((sh as f64 * scale).round() as u32).clamp(1, height);
    let image = image::GrayImage::from_raw(sw, sh, pixels).context("Frame size")?;
    let scaled = image::imageops::resize(&image, fw, fh, image::imageops::FilterType::Triangle);
    if (fw, fh) == (width, height) {
        return Ok(scaled.into_raw());
    }
    let (mut sum, mut count) = (0u64, 0u64);
    for x in 0..fw {
        sum += scaled.get_pixel(x, 0).0[0] as u64 + scaled.get_pixel(x, fh - 1).0[0] as u64;
        count += 2;
    }
    for y in 0..fh {
        sum += scaled.get_pixel(0, y).0[0] as u64 + scaled.get_pixel(fw - 1, y).0[0] as u64;
        count += 2;
    }
    let fill = (sum / count.max(1)) as u8;
    let mut out = vec![fill; width as usize * height as usize];
    let (ox, oy) = ((width - fw) / 2, (height - fh) / 2);
    let raw = scaled.into_raw();
    for y in 0..fh as usize {
        let start = (oy as usize + y) * width as usize + ox as usize;
        out[start..start + fw as usize]
            .copy_from_slice(&raw[y * fw as usize..(y + 1) * fw as usize]);
    }
    Ok(out)
}
pub fn unix_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fitting_preserves_aspect_and_centers_with_padding() {
        // 4×2 white image into 4×4: scaled 4×2, centered rows 1–2.
        let out = fit(vec![255; 8], (4, 2), 4, 4).unwrap();
        assert_eq!(out.len(), 16);
        assert!(out[4..12].iter().all(|&v| v == 255));
        assert_eq!(out[0], 255, "padding uses the border mean");
        let gradient: Vec<u8> = (0..16).map(|i| (i * 10) as u8).collect();
        assert_eq!(fit(gradient.clone(), (4, 4), 4, 4).unwrap(), gradient);
        let half = fit(vec![100; 64], (8, 8), 4, 4).unwrap();
        assert!(half.iter().all(|&v| v == 100));
    }
}
