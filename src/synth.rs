//! Synthetic tethered-fly scenes with known wing angles.
//!
//! The renderer models a backlit fly seen from above: an opaque body, a
//! translucent motion-blurred wing stroke envelope per wing (darker at the
//! stroke reversals, where the wing dwells), thin antennae, six segmented legs
//! that occasionally kick through the wing sectors, illumination falloff and
//! sensor noise. It is used by the simulated camera, the bundled sample
//! video and the detector regression tests, so every frame has ground truth.
use crate::{
    config::{Config, Point},
    frame::GrayFrame,
    tracker::wrap_angle,
};
use std::f32::consts::{PI, TAU};

/// Reference coordinates are for a 640×480 image and scale to any size.
const REF_W: f32 = 640.0;
const REF_H: f32 = 480.0;
const HEAD: Point = Point { x: 320.0, y: 175.0 };
const TAIL: Point = Point { x: 320.0, y: 330.0 };
const HINGES: [Point; 2] = [Point { x: 296.0, y: 240.0 }, Point { x: 344.0, y: 240.0 }];

/// What the scene contains. Defaults resemble a typical tethered flight rig.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneOptions {
    /// Translucent wing darkening at the envelope's dwell (0–1).
    pub wing_contrast: f32,
    /// Wing length from the hinge, in reference pixels.
    pub wing_length: f32,
    /// Mean leading-edge angle, degrees from perpendicular, toward the head.
    pub front_deg: f32,
    /// Mean trailing (dorsal reversal) angle, degrees, negative toward tail.
    pub back_deg: f32,
    /// Leading-edge modulation (turning) amplitude, degrees.
    pub modulation_deg: f32,
    /// Leading-edge modulation frequency, Hz.
    pub modulation_hz: f32,
    pub antennae: bool,
    /// Legs held in their flight posture.
    pub legs: bool,
    /// Periodic leg kicks that sweep through the wing sectors.
    pub leg_kicks: bool,
    /// Front legs reaching forward, in front of the leading edge.
    pub reaching_legs: bool,
    /// Leg thickness multiplier (1 = a Drosophila-like 2–3 px at 640×480).
    pub leg_width: f32,
    /// Gaussian sensor noise, gray levels.
    pub noise: f32,
    pub seed: u32,
}
impl Default for SceneOptions {
    fn default() -> Self {
        Self {
            wing_contrast: 0.14,
            wing_length: 170.0,
            front_deg: 50.0,
            back_deg: -62.0,
            modulation_deg: 10.0,
            modulation_hz: 0.8,
            antennae: true,
            legs: true,
            leg_kicks: true,
            reaching_legs: false,
            leg_width: 1.0,
            noise: 1.6,
            seed: 7,
        }
    }
}
/// Ground truth for one instant, in Rekke's angle conventions (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Truth {
    /// Leading edge, relative to the outward perpendicular, positive toward head.
    pub front: [f32; 2],
    pub back: [f32; 2],
    /// 0–1 extension of [left, right] × [front, middle, hind] legs.
    pub legs: [[f32; 3]; 2],
}

struct WingPixel {
    index: u32,
    radius: f32,
    /// Relative angle in degrees (positive toward head).
    angle: f32,
}
pub struct Scene {
    width: u32,
    height: u32,
    sx: f32,
    sy: f32,
    pub options: SceneOptions,
    illumination: Vec<f32>,
    /// Static opaque body: coverage (0–1) and premultiplied gray value.
    body: Vec<(f32, f32)>,
    wings: [Vec<WingPixel>; 2],
    noise: Vec<i8>,
}
fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}
impl Scene {
    pub fn new(width: u32, height: u32, options: SceneOptions) -> Self {
        let sx = width as f32 / REF_W;
        let sy = height as f32 / REF_H;
        let n = width as usize * height as usize;
        let mut illumination = Vec::with_capacity(n);
        for y in 0..height {
            for x in 0..width {
                let dx = (x as f32 / sx - 330.0) / 420.0;
                let dy = (y as f32 / sy - 230.0) / 420.0;
                illumination.push(214.0 - 46.0 * (dx * dx + dy * dy) + 7.0 * dx);
            }
        }
        let mut scene = Self {
            width,
            height,
            sx,
            sy,
            options,
            illumination,
            body: vec![(0.0, 0.0); n],
            wings: [Vec::new(), Vec::new()],
            noise: Vec::new(),
        };
        scene.paint_body();
        for wing in 0..2 {
            scene.wings[wing] = scene.wing_pixels(wing);
        }
        // A long Gaussian table, read from a random offset each frame.
        let mut state = scene.options.seed.max(1);
        scene.noise = (0..(1 << 20))
            .map(|_| {
                let mut sum = 0.0;
                for _ in 0..4 {
                    state = hash(state);
                    sum += (state & 0xffff) as f32 / 65535.0 - 0.5;
                }
                ((sum * 1.732).clamp(-3.5, 3.5) * 16.0) as i8
            })
            .collect();
        scene
    }
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn map(&self, p: Point) -> Point {
        Point {
            x: p.x * self.sx,
            y: p.y * self.sy,
        }
    }
    /// The calibration that matches this scene: body anchors, hinges and
    /// sectors spanning −45° to +85° around each wing.
    pub fn configure(&self, config: &mut Config) {
        config.head = self.map(HEAD);
        config.tail = self.map(TAIL);
        config.tracking.left.hinge = self.map(HINGES[0]);
        config.tracking.right.hinge = self.map(HINGES[1]);
    }
    fn ellipse(&mut self, c: Point, rx: f32, ry: f32, value: u8) {
        let c = self.map(c);
        let (rx, ry) = (rx * self.sx, ry * self.sy);
        let x0 = (c.x - rx - 2.0).floor().max(0.0) as u32;
        let x1 = ((c.x + rx + 2.0).ceil() as u32).min(self.width - 1);
        let y0 = (c.y - ry - 2.0).floor().max(0.0) as u32;
        let y1 = ((c.y + ry + 2.0).ceil() as u32).min(self.height - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = (x as f32 - c.x) / rx;
                let dy = (y as f32 - c.y) / ry;
                let d = (dx * dx + dy * dy).sqrt();
                // Approximate signed distance in pixels for antialiasing.
                let edge = (1.0 - d) * rx.min(ry);
                let cover = (edge + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 {
                    // Later shapes are painted over earlier ones.
                    let (alpha, premultiplied) =
                        &mut self.body[y as usize * self.width as usize + x as usize];
                    *premultiplied = value as f32 * cover + *premultiplied * (1.0 - cover);
                    *alpha = cover + *alpha * (1.0 - cover);
                }
            }
        }
    }
    fn paint_body(&mut self) {
        self.ellipse(Point { x: 320.0, y: 292.0 }, 18.0, 40.0, 46);
        for k in 0..4 {
            let y = 274.0 + k as f32 * 12.0;
            self.ellipse(Point { x: 320.0, y }, 17.0 - k as f32, 3.0, 34);
        }
        self.ellipse(Point { x: 320.0, y: 240.0 }, 22.0, 31.0, 36);
        self.ellipse(Point { x: 320.0, y: 197.0 }, 20.0, 14.0, 40);
        self.ellipse(Point { x: 307.0, y: 194.0 }, 9.0, 11.0, 30);
        self.ellipse(Point { x: 333.0, y: 194.0 }, 9.0, 11.0, 30);
        // Halteres and the tether glued to the thorax.
        self.ellipse(Point { x: 303.0, y: 262.0 }, 3.0, 3.0, 50);
        self.ellipse(Point { x: 337.0, y: 262.0 }, 3.0, 3.0, 50);
        self.ellipse(Point { x: 320.0, y: 236.0 }, 6.0, 6.0, 22);
    }
    fn wing_pixels(&self, wing: usize) -> Vec<WingPixel> {
        let hinge = self.map(HINGES[wing]);
        let head = self.map(HEAD);
        let tail = self.map(TAIL);
        let (dx, dy) = (head.x - tail.x, head.y - tail.y);
        let t = ((hinge.x - tail.x) * dx + (hinge.y - tail.y) * dy) / (dx * dx + dy * dy);
        let origin = Point {
            x: tail.x + t * dx,
            y: tail.y + t * dy,
        }
        .angle_to(hinge);
        let other = self.map(HINGES[1 - wing]);
        let w = if wing == 0 {
            (other.x - hinge.x, other.y - hinge.y)
        } else {
            (hinge.x - other.x, hinge.y - other.y)
        };
        let handed = (dx * w.1 - dy * w.0).signum();
        let sense = if wing == 0 { handed } else { -handed };
        let reach = (self.options.wing_length + 12.0) * self.sx.max(self.sy);
        let mut pixels = Vec::new();
        let y0 = (hinge.y - reach).max(0.0) as u32;
        let y1 = ((hinge.y + reach) as u32).min(self.height - 1);
        let x0 = (hinge.x - reach).max(0.0) as u32;
        let x1 = ((hinge.x + reach) as u32).min(self.width - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                // Distances in reference units keep the envelope shape stable.
                let p = Point {
                    x: x as f32 / self.sx,
                    y: y as f32 / self.sy,
                };
                let radius = HINGES[wing].distance(p);
                if radius > self.options.wing_length + 12.0 || radius < 8.0 {
                    continue;
                }
                let image = Point {
                    x: x as f32,
                    y: y as f32,
                };
                let angle = (wrap_angle(hinge.angle_to(image) - origin) * sense).to_degrees();
                if (-120.0..=125.0).contains(&angle) {
                    pixels.push(WingPixel {
                        index: y * self.width + x,
                        radius,
                        angle,
                    });
                }
            }
        }
        pixels
    }
    /// The deterministic behavior script at time `t` seconds.
    pub fn truth(&self, t: f32) -> Truth {
        let o = &self.options;
        let phase = TAU * o.modulation_hz * t;
        let turn = o.modulation_deg * phase.sin();
        let drift = [
            2.5 * (TAU * 0.37 * t + 1.0).sin(),
            2.5 * (TAU * 0.29 * t + 2.0).sin(),
        ];
        let front = [o.front_deg + turn + drift[0], o.front_deg - turn + drift[1]];
        let back = [
            o.back_deg - 0.3 * turn + 1.5 * (TAU * 0.5 * t).sin(),
            o.back_deg + 0.3 * turn + 1.5 * (TAU * 0.43 * t).sin(),
        ];
        // Leg kicks: a smooth out-and-back sweep lasting about a second.
        let pulse = |start: f32, period: f32, length: f32| {
            if !o.leg_kicks {
                return 0.0;
            }
            let local = (t - start).rem_euclid(period);
            if local < length {
                (PI * local / length).sin().powi(2)
            } else {
                0.0
            }
        };
        let reach: f32 = if o.reaching_legs { 1.0 } else { 0.0 };
        let legs = [
            [
                reach.max(pulse(2.0, 9.0, 1.4)),
                pulse(5.0, 7.0, 1.1),
                pulse(0.8, 6.0, 1.2),
            ],
            [
                reach.max(pulse(6.5, 8.0, 1.3)),
                pulse(3.4, 7.5, 1.0),
                pulse(4.2, 6.5, 1.2),
            ],
        ];
        Truth {
            front: front.map(f32::to_radians),
            back: back.map(f32::to_radians),
            legs,
        }
    }
    /// Background without any fly, for background-subtraction references.
    pub fn empty(&self) -> GrayFrame {
        let pixels = self
            .illumination
            .iter()
            .map(|v| v.round().clamp(0.0, 255.0) as u8)
            .collect();
        GrayFrame {
            width: self.width,
            height: self.height,
            pixels,
            timestamp_ns: crate::frame::unix_ns(),
            sequence: 0,
        }
    }
    pub fn render(&self, t: f32, sequence: u64) -> GrayFrame {
        let truth = self.truth(t);
        let mut light = self.illumination.clone();
        let o = &self.options;
        for (wing, pixels) in self.wings.iter().enumerate() {
            let front = truth.front[wing].to_degrees();
            let back = truth.back[wing].to_degrees();
            let soft = 2.5;
            let tip = o.wing_length;
            for p in pixels {
                let a = p.angle;
                if a > front + soft || a < back - soft {
                    continue;
                }
                let lead = 1.0 - smoothstep((a - (front - soft)) / (2.0 * soft));
                let trail = smoothstep((a - (back - soft)) / (2.0 * soft));
                let df = (front - a) / 10.0;
                let db = (a - back) / 10.0;
                // Wings dwell at the stroke reversals: darker envelope ends.
                let density = 0.6 + 0.4 * (-df * df).exp() + 0.3 * (-db * db).exp();
                let radial = smoothstep((p.radius - 14.0) / 22.0)
                    * (1.0 - smoothstep((p.radius - (tip - 34.0)) / 36.0));
                let opacity = o.wing_contrast * density * lead * trail * radial;
                light[p.index as usize] *= 1.0 - opacity.min(0.9);
            }
        }
        for (value, &(alpha, premultiplied)) in light.iter_mut().zip(&self.body) {
            if alpha > 0.0 {
                *value = *value * (1.0 - alpha) + premultiplied;
            }
        }
        if o.antennae {
            for side in [-1.0, 1.0] {
                let base = Point {
                    x: 320.0 + side * 6.0,
                    y: 184.0,
                };
                let wobble = 2.0 * (TAU * 0.9 * t + side).sin();
                let funiculus = Point {
                    x: base.x + side * 5.0,
                    y: base.y - 7.0,
                };
                let direction = (-90.0 + side * (52.0 + wobble)).to_radians();
                let arista = funiculus.at(30.0, direction);
                self.segment(&mut light, base, funiculus, 3.4, 44.0);
                self.segment(&mut light, funiculus, arista, 1.3, 70.0);
            }
        }
        if o.legs {
            for (wing, side) in [(0, -1.0), (1, 1.0)] {
                for (leg, &extension) in truth.legs[wing].iter().enumerate() {
                    let joints = leg_pose(leg, extension);
                    let mut previous = Point {
                        x: 320.0 + side * (320.0 - joints[0].x),
                        y: joints[0].y,
                    };
                    for (k, joint) in joints.iter().enumerate().skip(1) {
                        let next = Point {
                            x: 320.0 + side * (320.0 - joint.x),
                            y: joint.y,
                        };
                        let width = [3.2, 2.6, 2.0, 1.6][k] * o.leg_width;
                        self.segment(&mut light, previous, next, width, 40.0);
                        previous = next;
                    }
                }
            }
        }
        let mut pixels = Vec::with_capacity(light.len());
        let frame_seed = hash(sequence as u32 ^ o.seed.wrapping_mul(0x9e37_79b9));
        let offset = (frame_seed as usize) & (self.noise.len() - 1);
        let gain = 1.0 + ((frame_seed >> 20) as f32 / 4096.0 - 0.5) * 0.008;
        let noise = o.noise / 16.0;
        let mask = self.noise.len() - 1;
        for (i, value) in light.iter().enumerate() {
            let n = self.noise[(offset + i * 7) & mask] as f32 * noise;
            pixels.push((value * gain + n).round().clamp(0.0, 255.0) as u8);
        }
        GrayFrame {
            width: self.width,
            height: self.height,
            pixels,
            timestamp_ns: crate::frame::unix_ns(),
            sequence,
        }
    }
    /// An antialiased opaque capsule from `a` to `b` (reference coordinates).
    fn segment(&self, light: &mut [f32], a: Point, b: Point, width: f32, gray: f32) {
        let a = self.map(a);
        let b = self.map(b);
        let half = 0.5 * width * self.sx.max(self.sy);
        let x0 = (a.x.min(b.x) - half - 1.0).floor().max(0.0) as u32;
        let x1 = ((a.x.max(b.x) + half + 1.0).ceil() as u32).min(self.width - 1);
        let y0 = (a.y.min(b.y) - half - 1.0).floor().max(0.0) as u32;
        let y1 = ((a.y.max(b.y) + half + 1.0).ceil() as u32).min(self.height - 1);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = (dx * dx + dy * dy).max(1e-6);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 - a.x, y as f32 - a.y);
                let t = ((px * dx + py * dy) / length).clamp(0.0, 1.0);
                let distance = (px - t * dx).hypot(py - t * dy);
                let cover = (half - distance + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 {
                    let v = &mut light[y as usize * self.width as usize + x as usize];
                    *v = *v * (1.0 - cover) + gray * cover;
                }
            }
        }
    }
}
type Joints = [(f32, f32); 4];
/// Left-side leg joints (reference coordinates), from rest to extended.
fn leg_pose(leg: usize, extension: f32) -> [Point; 4] {
    let (rest, out): (Joints, Joints) = match leg {
        // Front leg: tucked under the head, or reaching forward-laterally.
        0 => (
            [
                (311.0, 222.0),
                (301.0, 212.0),
                (297.0, 198.0),
                (301.0, 187.0),
            ],
            [
                (311.0, 222.0),
                (295.0, 206.0),
                (274.0, 178.0),
                (262.0, 150.0),
            ],
        ),
        // Middle leg: lateral, kicking out through the wing envelope.
        1 => (
            [
                (309.0, 244.0),
                (291.0, 251.0),
                (275.0, 260.0),
                (268.0, 266.0),
            ],
            [
                (309.0, 244.0),
                (284.0, 256.0),
                (248.0, 276.0),
                (222.0, 294.0),
            ],
        ),
        // Hind leg: backward, sweeping laterally out to the rear of the sector.
        _ => (
            [
                (311.0, 256.0),
                (299.0, 270.0),
                (292.0, 291.0),
                (290.0, 305.0),
            ],
            [
                (311.0, 256.0),
                (291.0, 274.0),
                (258.0, 294.0),
                (224.0, 306.0),
            ],
        ),
    };
    let e = extension.clamp(0.0, 1.0);
    std::array::from_fn(|k| Point {
        x: rest[k].0 + (out[k].0 - rest[k].0) * e,
        y: rest[k].1 + (out[k].1 - rest[k].1) * e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scene_is_deterministic_and_darkens_only_inside_the_envelope() {
        let scene = Scene::new(640, 480, SceneOptions::default());
        let a = scene.render(1.25, 3);
        let b = scene.render(1.25, 3);
        assert_eq!(a.pixels, b.pixels);
        let truth = scene.truth(1.25);
        let empty = scene.empty();
        let hinge = HINGES[0];
        // Left wing: relative angle a → image angle π + a (head is up).
        let sample = |frame: &GrayFrame, rel: f32| {
            let p = hinge.at(110.0, PI + rel);
            frame.pixels[p.y as usize * 640 + p.x as usize] as f32
        };
        let inside = sample(&a, truth.front[0] - 0.15) - sample(&empty, truth.front[0] - 0.15);
        let outside = sample(&a, truth.front[0] + 0.15) - sample(&empty, truth.front[0] + 0.15);
        assert!(inside < -12.0, "envelope darkening {inside}");
        assert!(outside.abs() < 8.0, "background change {outside}");
    }
}
