use crate::{
    config::{Config, Method, Point, PrimaryEdge, Sector},
    frame::GrayFrame,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::f32::consts::{PI, TAU};

pub const NAMES: [&str; 4] = ["Left wing", "Right wing", "Head", "Abdomen"];
pub fn wrap_angle(a: f32) -> f32 {
    (a + PI).rem_euclid(TAU) - PI
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PartState {
    pub enabled: bool,
    /// Signed relative angle; wings positive toward the head.
    pub angle_rad: Option<f32>,
    pub image_angle_rad: Option<f32>,
    /// Strongest first. No edges means the measurement is invalid.
    pub angles_rad: Vec<f32>,
    pub gradients: Vec<f32>,
    pub radius_px: Option<f32>,
    pub intensity: f32,
    pub strength: f32,
    /// Strongest available score before the user threshold, for live tuning.
    #[serde(default)]
    pub peak_gradient: f32,
    pub coverage: f32,
    #[serde(skip)]
    pub profile: Vec<f32>,
    #[serde(skip)]
    pub image_angles: Vec<f32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Measurement {
    pub schema_version: u32,
    pub instance: String,
    pub sequence: u64,
    /// Host receive time, not a hardware exposure timestamp.
    pub timestamp_ns: u64,
    pub elapsed_ns: u64,
    pub width: u32,
    pub height: u32,
    pub capture_fps: f32,
    pub processing_us: f32,
    pub background_subtracted: bool,
    pub left: PartState,
    pub right: PartState,
    pub head: PartState,
    pub abdomen: PartState,
    pub left_minus_right_rad: Option<f32>,
    pub left_plus_right_rad: Option<f32>,
}

#[derive(Clone)]
struct Sample {
    index: usize,
    wx: f32,
    wy: f32,
    radius: f32,
}
impl Sample {
    fn value(&self, frame: &GrayFrame) -> f32 {
        let p = &frame.pixels;
        let i = self.index;
        let w = frame.width as usize;
        // Difference form preserves uniform pixels exactly, even at threshold zero.
        let top = p[i] as f32 + (p[i + 1] as f32 - p[i] as f32) * self.wx;
        let bot = p[i + w] as f32 + (p[i + w + 1] as f32 - p[i + w] as f32) * self.wx;
        top + (bot - top) * self.wy
    }
}
fn smooth_gradient(profile: &[f32]) -> Vec<f32> {
    let smooth: Vec<f32> = (0..profile.len())
        .map(|i| {
            let lo = i.saturating_sub(1);
            let hi = (i + 1).min(profile.len() - 1);
            profile[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32
        })
        .collect();
    (0..profile.len())
        .map(|i| smooth[(i + 2).min(profile.len() - 1)] - smooth[i.saturating_sub(2)])
        .collect()
}
fn median(values: &mut [f32]) -> f32 {
    values.sort_unstable_by(f32::total_cmp);
    let m = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[m - 1] + values[m]) * 0.5
    } else {
        values[m]
    }
}
// Flat grayscale opening removes short bright pulses without attenuating a
// broad, low shoulder. Monotonic queues keep both passes linear in angle bins.
fn envelope(profile: &[f32], radius: usize, minimum: bool) -> Vec<f32> {
    let mut queue: VecDeque<(usize, f32)> = VecDeque::new();
    let mut out = Vec::with_capacity(profile.len());
    let window = radius * 2 + 1;
    for i in 0..profile.len() + radius * 2 {
        let value = profile[i.saturating_sub(radius).min(profile.len() - 1)];
        while queue
            .back()
            .is_some_and(|&(_, v)| if minimum { v >= value } else { v <= value })
        {
            queue.pop_back();
        }
        queue.push_back((i, value));
        while queue.front().is_some_and(|&(j, _)| j + window <= i) {
            queue.pop_front();
        }
        if i + 1 >= window {
            out.push(queue.front().unwrap().1);
        }
    }
    out
}
fn remove_spikes(profile: &[f32], radius: usize) -> Vec<f32> {
    if radius == 0 {
        return profile.to_vec();
    }
    envelope(&envelope(profile, radius, true), radius, false)
}
struct PolarGrid {
    samples: Vec<Option<Sample>>,
    bins: usize,
    radial: usize,
    coverage: Vec<f32>,
}
impl PolarGrid {
    fn new(s: &Sector, width: u32, height: u32, bins: usize, radial: usize) -> Self {
        let mut samples = Vec::with_capacity(bins * radial);
        let mut coverage = Vec::with_capacity(bins);
        for b in 0..bins {
            let a = (s.start_deg + s.span_deg * b as f32 / (bins - 1) as f32).to_radians();
            let mut count = 0;
            for r in 0..radial {
                let radius = s.inner_radius
                    + (s.outer_radius - s.inner_radius) * (r as f32 + 0.5) / radial as f32;
                let p = s.hinge.at(radius, a);
                if p.x >= 0.0 && p.y >= 0.0 && p.x < (width - 1) as f32 && p.y < (height - 1) as f32
                {
                    let x = p.x.floor() as usize;
                    let y = p.y.floor() as usize;
                    samples.push(Some(Sample {
                        index: y * width as usize + x,
                        wx: p.x - x as f32,
                        wy: p.y - y as f32,
                        radius,
                    }));
                    count += 1;
                } else {
                    samples.push(None);
                }
            }
            coverage.push(count as f32 / radial as f32);
        }
        Self {
            samples,
            bins,
            radial,
            coverage,
        }
    }
    fn profile(&self, frame: &GrayFrame) -> (Vec<f32>, Vec<f32>) {
        let mut profile = vec![0.0; self.bins];
        let mut radii = vec![0.0; self.bins];
        for b in 0..self.bins {
            let mut count = 0;
            for sample in self.samples[b * self.radial..(b + 1) * self.radial]
                .iter()
                .flatten()
            {
                let value = sample.value(frame);
                profile[b] += value;
                radii[b] += value * sample.radius;
                count += 1;
            }
            if profile[b] > 0.0 {
                radii[b] /= profile[b];
            }
            if count > 0 {
                profile[b] /= count as f32 * 255.0;
            }
        }
        (profile, radii)
    }
    fn consensus_gradient(
        &self,
        frame: &GrayFrame,
        contrast_cap: f32,
        spike_radius: usize,
    ) -> Vec<f32> {
        let bands = self.radial.min(8);
        let mut profiles = vec![vec![0.0; self.bins]; bands];
        for b in 0..self.bins {
            for (band, profile) in profiles.iter_mut().enumerate() {
                let lo = band * self.radial / bands;
                let hi = (band + 1) * self.radial / bands;
                let mut count = 0;
                let mut sum = 0.0;
                for sample in self.samples[b * self.radial + lo..b * self.radial + hi]
                    .iter()
                    .flatten()
                {
                    sum += sample.value(frame);
                    count += 1;
                }
                if count > 0 {
                    profile[b] = sum / (count as f32 * 255.0);
                }
            }
        }
        let gradients: Vec<Vec<f32>> = profiles
            .iter_mut()
            .map(|p| {
                // Empty bins beyond the image must not become the background
                // baseline and saturate otherwise valid, low-contrast bands.
                let baseline = p
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| self.coverage[*i] >= 0.9)
                    .map(|(_, v)| *v)
                    .fold(f32::INFINITY, f32::min);
                for value in p.iter_mut() {
                    *value = value.min(baseline + contrast_cap);
                }
                smooth_gradient(&remove_spikes(p, spike_radius))
            })
            .collect();
        (0..self.bins)
            .map(|i| {
                let mut values = [0.0; 8];
                for (v, gradient) in values.iter_mut().zip(&gradients) {
                    *v = gradient[i];
                }
                median(&mut values[..bands])
            })
            .collect()
    }
}
pub struct Tracker {
    config: Config,
    width: u32,
    height: u32,
    grids: [PolarGrid; 4],
}
impl Tracker {
    pub fn update(&mut self, config: &Config) {
        // Threshold/polarity/toggles do not invalidate geometry sampling tables.
        for i in 0..4 {
            let old = self.config.sectors()[i];
            let new = config.sectors()[i];
            if old.hinge != new.hinge
                || old.start_deg != new.start_deg
                || old.span_deg != new.span_deg
                || old.inner_radius != new.inner_radius
                || old.outer_radius != new.outer_radius
                || self.config.tracking.angle_bins != config.tracking.angle_bins
                || self.config.tracking.radial_samples != config.tracking.radial_samples
            {
                self.grids[i] = PolarGrid::new(
                    new,
                    self.width,
                    self.height,
                    config.tracking.angle_bins,
                    config.tracking.radial_samples,
                );
            }
        }
        self.config = config.clone();
    }
    pub fn new(config: &Config, width: u32, height: u32) -> Self {
        let grids = config.sectors().map(|s| {
            PolarGrid::new(
                s,
                width,
                height,
                config.tracking.angle_bins,
                config.tracking.radial_samples,
            )
        });
        Self {
            config: config.clone(),
            width,
            height,
            grids,
        }
    }
    pub fn process(
        &self,
        raw: &GrayFrame,
        background: Option<&GrayFrame>,
        elapsed_ns: u64,
    ) -> Result<Measurement> {
        ensure!(
            raw.width == self.width && raw.height == self.height,
            "Camera resolution changed; restart acquisition with matching configuration"
        );
        let clock = std::time::Instant::now();
        let subtract = self.config.tracking.subtract_background;
        let processed = if subtract {
            Some(raw.difference(background.ok_or_else(|| {
                anyhow::anyhow!("Background subtraction requested but no reference is loaded")
            })?)?)
        } else if self.config.tracking.dark_on_light {
            Some(GrayFrame {
                width: raw.width,
                height: raw.height,
                timestamp_ns: raw.timestamp_ns,
                sequence: raw.sequence,
                pixels: raw.pixels.iter().map(|x| 255 - x).collect(),
            })
        } else {
            None
        };
        let foreground = processed.as_ref().unwrap_or(raw);
        let states: [PartState; 4] = std::array::from_fn(|i| self.track(i, foreground));
        let [left, right, head, abdomen] = states;
        let difference = left.angle_rad.zip(right.angle_rad).map(|(l, r)| l - r);
        let sum = left.angle_rad.zip(right.angle_rad).map(|(l, r)| l + r);
        Ok(Measurement {
            schema_version: 1,
            instance: self.config.instance.clone(),
            sequence: raw.sequence,
            timestamp_ns: raw.timestamp_ns,
            elapsed_ns,
            width: raw.width,
            height: raw.height,
            capture_fps: 0.0,
            processing_us: clock.elapsed().as_secs_f32() * 1e6,
            background_subtracted: subtract,
            left,
            right,
            head,
            abdomen,
            left_minus_right_rad: difference,
            left_plus_right_rad: sum,
        })
    }
    fn track(&self, index: usize, foreground: &GrayFrame) -> PartState {
        let s = self.config.sectors()[index];
        if !s.enabled {
            return PartState::default();
        }
        let grid = &self.grids[index];
        let (profile, radii) = grid.profile(foreground);
        let coverage = grid.coverage.iter().sum::<f32>() / grid.bins as f32;
        let intensity = profile.iter().sum::<f32>() / grid.bins as f32;
        let mut state = PartState {
            enabled: true,
            intensity,
            coverage,
            ..Default::default()
        };
        let origin = self.origin(index);
        let relative = |a| wrap_angle(a - origin) * self.sense(index);
        match s.method {
            Method::Edge | Method::WingBlur => {
                let spike_radius = if s.method == Method::WingBlur {
                    ((s.spike_width_deg / (s.span_deg / (grid.bins - 1) as f32)).ceil() as usize
                        / 2)
                    .min(grid.bins)
                } else {
                    0
                };
                let gradient = if s.method == Method::WingBlur {
                    grid.consensus_gradient(foreground, s.contrast_cap, spike_radius)
                } else {
                    smooth_gradient(&profile)
                };
                let valid = |i: usize| {
                    grid.coverage[i.saturating_sub(2 + spike_radius * 2)
                        ..=(i + 2 + spike_radius * 2).min(grid.bins - 1)]
                        .iter()
                        .all(|x| *x >= 0.9)
                };
                state.peak_gradient = (0..grid.bins)
                    .filter(|&i| valid(i))
                    .map(|i| gradient[i].abs())
                    .fold(0.0, f32::max);
                let mut candidates: Vec<usize> = (0..grid.bins)
                    .filter(|&i| {
                        let g = gradient[i].abs();
                        // Numerical floor only; well below a one-level 8-bit signal.
                        g > 1e-7
                            && g >= s.threshold
                            && (i == 0 || g >= gradient[i - 1].abs())
                            && (i == grid.bins - 1 || g > gradient[i + 1].abs())
                            && valid(i)
                    })
                    .collect();
                // A clipped/monotonic signal may have its peak beside an invalid bin.
                if candidates.is_empty()
                    && let Some(i) = (0..grid.bins)
                        .filter(|&i| {
                            valid(i) && gradient[i].abs() > 1e-7 && gradient[i].abs() >= s.threshold
                        })
                        .max_by(|&a, &b| gradient[a].abs().total_cmp(&gradient[b].abs()))
                {
                    candidates.push(i);
                }
                candidates
                    .sort_unstable_by(|a, b| gradient[*b].abs().total_cmp(&gradient[*a].abs()));
                let mut selected: Vec<usize> = Vec::new();
                for i in candidates {
                    // Suppress a complete same-sign derivative lobe, not only its peak.
                    let same_lobe = selected.iter().any(|&j| {
                        gradient[i] * gradient[j] > 0.0
                            && gradient[i.min(j)..=i.max(j)]
                                .iter()
                                .all(|x| x * gradient[i] > 0.0)
                    });
                    if !same_lobe && selected.iter().all(|&j| j.abs_diff(i) > 4) {
                        let a = (s.start_deg + s.span_deg * i as f32 / (grid.bins - 1) as f32)
                            .to_radians();
                        state.image_angles.push(a);
                        state.angles_rad.push(relative(a));
                        state.gradients.push(gradient[i]);
                        selected.push(i);
                        if selected.len() == s.max_edges {
                            break;
                        }
                    }
                }
                if !state.image_angles.is_empty() {
                    let selected = match s.primary_edge {
                        PrimaryEdge::Strongest => 0,
                        PrimaryEdge::TowardHead => {
                            state
                                .angles_rad
                                .iter()
                                .enumerate()
                                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                                .unwrap()
                                .0
                        }
                        PrimaryEdge::TowardTail => {
                            state
                                .angles_rad
                                .iter()
                                .enumerate()
                                .min_by(|(_, a), (_, b)| a.total_cmp(b))
                                .unwrap()
                                .0
                        }
                    };
                    state.image_angle_rad = Some(state.image_angles[selected]);
                    state.angle_rad = Some(state.angles_rad[selected]);
                    state.strength = state.gradients[selected].abs();
                }
            }
            Method::Centroid => {
                let baseline = profile.iter().copied().fold(f32::INFINITY, f32::min);
                let peak = profile.iter().copied().fold(0.0_f32, f32::max);
                state.peak_gradient = peak - baseline;
                let mut x = 0.0;
                let mut y = 0.0;
                let mut mass = 0.0;
                let mut radius = 0.0;
                for (i, &p) in profile.iter().enumerate() {
                    if grid.coverage[i] < 0.9 {
                        continue;
                    }
                    let weight = (p - baseline - s.threshold).max(0.0);
                    let a =
                        (s.start_deg + s.span_deg * i as f32 / (grid.bins - 1) as f32).to_radians();
                    x += weight * a.cos();
                    y += weight * a.sin();
                    mass += weight;
                    radius += weight * radii[i];
                }
                if mass > 0.0 && (peak - baseline) >= s.threshold && x.hypot(y) > 1e-6 {
                    let a = y.atan2(x);
                    state.image_angle_rad = Some(a);
                    state.angle_rad = Some(relative(a));
                    state.angles_rad.push(relative(a));
                    state.image_angles.push(a);
                    state.radius_px = Some(radius / mass);
                    state.strength = peak - baseline;
                }
            }
        }
        state.profile = profile;
        state
    }
    fn origin(&self, index: usize) -> f32 {
        match index {
            0 | 1 => {
                // Kinefly's wing zero is outward perpendicular to the body
                // midline, even when asymmetric hinges are not level.
                let hinge = self.config.sectors()[index].hinge;
                let tail = self.config.tail;
                let dx = self.config.head.x - tail.x;
                let dy = self.config.head.y - tail.y;
                let t = ((hinge.x - tail.x) * dx + (hinge.y - tail.y) * dy) / (dx * dx + dy * dy);
                Point {
                    x: tail.x + t * dx,
                    y: tail.y + t * dy,
                }
                .angle_to(hinge)
            }
            2 => self.config.tail.angle_to(self.config.head),
            _ => self.config.head.angle_to(self.config.tail),
        }
    }
    fn sense(&self, index: usize) -> f32 {
        if index >= 2 {
            return 1.0;
        }
        let f = Point {
            x: self.config.head.x - self.config.tail.x,
            y: self.config.head.y - self.config.tail.y,
        };
        let w = Point {
            x: self.config.tracking.right.hinge.x - self.config.tracking.left.hinge.x,
            y: self.config.tracking.right.hinge.y - self.config.tracking.left.hinge.y,
        };
        let handedness = (f.x * w.y - f.y * w.x).signum();
        if index == 0 { handedness } else { -handedness }
    }
}
