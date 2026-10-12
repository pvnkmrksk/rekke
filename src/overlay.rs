//! What the tracker sees, drawn on a frame: the body axis, every enabled
//! sector and the edges it found. Used for snapshots that agents (and
//! documentation) can look at without the GUI.
use crate::{
    config::{Config, Point},
    frame::GrayFrame,
    tracker::{Measurement, PartState},
};
use anyhow::{Context, Result};

/// Left, right, head and abdomen, as in the GUI.
const COLORS: [[u8; 3]; 4] = [
    [247, 93, 99],
    [87, 216, 115],
    [224, 107, 239],
    [86, 216, 227],
];
const HEAD: [u8; 3] = [224, 107, 239];
const TAIL: [u8; 3] = [86, 216, 227];

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// RGB, row-major.
    pub pixels: Vec<u8>,
}
impl Canvas {
    pub fn from_gray(frame: &GrayFrame) -> Self {
        Self {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels.iter().flat_map(|&v| [v, v, v]).collect(),
        }
    }
    /// Anti-aliased polyline of the given width, blended once so that
    /// overlapping segments do not darken.
    pub fn stroke(&mut self, points: &[Point], width: f32, color: [u8; 3], alpha: f32) {
        let Some(first) = points.first() else {
            return;
        };
        let r = width / 2.0;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for p in points {
            (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
        }
        let pad = r + 1.5;
        let left = (x0 - pad).floor().max(0.0) as i64;
        let top = (y0 - pad).floor().max(0.0) as i64;
        let right = ((x1 + pad).ceil() as i64).min(self.width as i64 - 1);
        let bottom = ((y1 + pad).ceil() as i64).min(self.height as i64 - 1);
        if right < left || bottom < top {
            return;
        }
        let w = (right - left + 1) as usize;
        let mut mask = vec![0.0_f32; w * (bottom - top + 1) as usize];
        let mut splat = |cx: f32, cy: f32| {
            let lo_x = ((cx - pad).floor() as i64).max(left);
            let hi_x = ((cx + pad).ceil() as i64).min(right);
            let lo_y = ((cy - pad).floor() as i64).max(top);
            let hi_y = ((cy + pad).ceil() as i64).min(bottom);
            for y in lo_y..=hi_y {
                for x in lo_x..=hi_x {
                    let d = (x as f32 - cx).hypot(y as f32 - cy);
                    let cover = (r + 0.5 - d).clamp(0.0, 1.0);
                    let m = &mut mask[(y - top) as usize * w + (x - left) as usize];
                    *m = m.max(cover);
                }
            }
        };
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let steps = (a.distance(b) * 2.0).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let t = k as f32 / steps as f32;
                splat(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            }
        }
        if points.len() == 1 {
            splat(first.x, first.y);
        }
        for y in top..=bottom {
            for x in left..=right {
                let m = mask[(y - top) as usize * w + (x - left) as usize] * alpha;
                if m > 0.0 {
                    let i = (y as usize * self.width as usize + x as usize) * 3;
                    for (c, &target) in self.pixels[i..i + 3].iter_mut().zip(&color) {
                        *c = (*c as f32 + (target as f32 - *c as f32) * m).round() as u8;
                    }
                }
            }
        }
    }
    pub fn line(&mut self, a: Point, b: Point, width: f32, color: [u8; 3], alpha: f32) {
        self.stroke(&[a, b], width, color, alpha);
    }
    pub fn dot(&mut self, at: Point, radius: f32, color: [u8; 3]) {
        self.stroke(&[at], radius * 2.0, [12, 14, 18], 0.85);
        self.stroke(&[at], radius * 2.0 - 2.5, color, 1.0);
    }
    pub fn arc(&mut self, centre: Point, radius: f32, from: f32, to: f32, color: [u8; 3]) {
        let steps = ((to - from).abs() * radius / 3.0).ceil().max(2.0) as usize;
        let points: Vec<Point> = (0..=steps)
            .map(|k| centre.at(radius, from + (to - from) * k as f32 / steps as f32))
            .collect();
        self.stroke(&points, 1.4, color, 0.75);
    }
    pub fn to_png(&self) -> Result<Vec<u8>> {
        use image::ImageEncoder;
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(
                &self.pixels,
                self.width,
                self.height,
                image::ExtendedColorType::Rgb8,
            )
            .context("Encode PNG")?;
        Ok(out)
    }
    pub fn to_jpeg(&self, quality: u8) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        jpeg_encoder::Encoder::new(&mut out, quality)
            .encode(
                &self.pixels,
                self.width as u16,
                self.height as u16,
                jpeg_encoder::ColorType::Rgb,
            )
            .context("Encode JPEG")?;
        Ok(out)
    }
    /// Shrink to at most `max_width` pixels wide, keeping the aspect ratio.
    pub fn fit_width(self, max_width: u32) -> Self {
        if self.width <= max_width || max_width == 0 {
            return self;
        }
        let height = (self.height as u64 * max_width as u64 / self.width as u64).max(1) as u32;
        let image = image::RgbImage::from_raw(self.width, self.height, self.pixels)
            .expect("canvas size matches its pixels");
        let small = image::imageops::resize(
            &image,
            max_width,
            height,
            image::imageops::FilterType::Triangle,
        );
        Self {
            width: max_width,
            height,
            pixels: small.into_raw(),
        }
    }
}

/// The frame with the body axis, sectors and detected edges. The primary
/// edge of each part is drawn bold, other edges thin.
pub fn render(frame: &GrayFrame, config: &Config, measurement: Option<&Measurement>) -> Canvas {
    let mut canvas = Canvas::from_gray(frame);
    let scale = (frame.width.max(frame.height) as f32 / 640.0).max(1.0);
    canvas.line(config.tail, config.head, 1.2 * scale, [200, 200, 200], 0.6);
    let parts: [Option<&PartState>; 4] = match measurement {
        Some(m) => [
            Some(&m.left),
            Some(&m.right),
            Some(&m.head),
            Some(&m.abdomen),
        ],
        None => [None; 4],
    };
    for (i, (s, part)) in config.sectors().into_iter().zip(parts).enumerate() {
        if !s.enabled {
            continue;
        }
        let color = COLORS[i];
        let (from, to) = (
            s.start_deg.to_radians(),
            (s.start_deg + s.span_deg).to_radians(),
        );
        canvas.arc(s.hinge, s.inner_radius, from, to, color);
        canvas.arc(s.hinge, s.outer_radius, from, to, color);
        for a in [from, to] {
            canvas.line(
                s.hinge.at(s.inner_radius, a),
                s.hinge.at(s.outer_radius, a),
                1.4,
                color,
                0.75,
            );
        }
        if let Some(part) = part {
            for &a in part.image_angles.iter().rev() {
                let primary = part.image_angle_rad.is_some_and(|p| (p - a).abs() < 1e-5);
                let (width, alpha) = if primary {
                    (3.0 * scale, 1.0)
                } else {
                    (1.5 * scale, 0.55)
                };
                let reach = s.outer_radius + 12.0 * scale;
                canvas.line(
                    s.hinge,
                    s.hinge.at(reach, a),
                    width + 2.0,
                    [12, 14, 18],
                    alpha * 0.6,
                );
                canvas.line(s.hinge, s.hinge.at(reach, a), width, color, alpha);
            }
        }
        canvas.dot(s.hinge, 4.0 * scale, color);
    }
    canvas.dot(config.head, 5.0 * scale, HEAD);
    canvas.dot(config.tail, 5.0 * scale, TAIL);
    canvas
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strokes_blend_once_and_stay_inside_the_image() {
        let frame = GrayFrame::new(40, 30, vec![100; 1200], 0, 0).unwrap();
        let mut c = Canvas::from_gray(&frame);
        // A doubled-back line is no darker than a single pass.
        let (a, b) = (Point { x: 5.0, y: 15.0 }, Point { x: 35.0, y: 15.0 });
        c.stroke(&[a, b, a], 3.0, [0, 0, 0], 0.5);
        let i = (15 * 40 + 20) * 3;
        assert_eq!(c.pixels[i], 50);
        // Shapes beyond the edges are clipped, never panic.
        c.line(
            Point { x: -50.0, y: -50.0 },
            Point { x: 90.0, y: 80.0 },
            4.0,
            [255, 0, 0],
            1.0,
        );
        c.arc(Point { x: 20.0, y: 15.0 }, 60.0, 0.0, 6.0, [0, 255, 0]);
        assert_eq!(c.pixels.len(), 40 * 30 * 3);
        assert!(c.to_png().unwrap().starts_with(b"\x89PNG"));
        assert!(c.to_jpeg(80).unwrap().starts_with(&[0xFF, 0xD8]));
        let small = c.fit_width(20);
        assert_eq!((small.width, small.height), (20, 15));
    }
}
