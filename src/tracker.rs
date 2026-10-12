use crate::{
    config::{Config, Method, Point, PrimaryEdge, Sector},
    frame::GrayFrame,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
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
    /// The primary edge first, then the others by strength. No edges means
    /// the measurement is invalid.
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
    /// What the robust detector ranks edges on: the median level across
    /// radial bands after thin features (legs, antennae) are removed.
    #[serde(skip)]
    pub filtered: Vec<f32>,
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

/// How pixel values are read: raw, inverted (dark insect on a light
/// background) or as the absolute difference from a saved background. The
/// transform is applied while sampling, so no full-frame copy is ever made.
#[derive(Clone, Copy)]
enum Pixels<'a> {
    Raw(&'a [u8]),
    Inverted(&'a [u8]),
    Difference(&'a [u8], &'a [u8]),
}
#[inline(always)]
fn bilinear(a: f32, b: f32, c: f32, d: f32, wx: f32, wy: f32) -> f32 {
    // Difference form preserves uniform pixels exactly, even at threshold zero.
    let top = a + (b - a) * wx;
    let bot = c + (d - c) * wx;
    top + (bot - top) * wy
}
/// Three-bin box smoothing, then a central difference over ±2 bins, with
/// the window clipped at the profile ends.
fn smooth_gradient(profile: &[f32]) -> Vec<f32> {
    let n = profile.len();
    let mut smooth = vec![0.0; n];
    for (i, value) in smooth.iter_mut().enumerate() {
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(n - 1);
        let mut sum = 0.0;
        for v in &profile[lo..=hi] {
            sum += v;
        }
        *value = sum / (hi - lo + 1) as f32;
    }
    (0..n)
        .map(|i| smooth[(i + 2).min(n - 1)] - smooth[i.saturating_sub(2)])
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
// broad, low shoulder. Edge samples are replicated beyond the profile ends.
// Van Herk/Gil-Werman: three comparisons per bin for any window width.
fn envelope(profile: &[f32], radius: usize, minimum: bool) -> Vec<f32> {
    let n = profile.len();
    let w = 2 * radius + 1;
    let pick = |a: f32, b: f32| if minimum { a.min(b) } else { a.max(b) };
    let padded: Vec<f32> = (0..n + 2 * radius)
        .map(|i| profile[i.saturating_sub(radius).min(n - 1)])
        .collect();
    let m = padded.len();
    let mut forward = padded.clone();
    let mut backward = padded.clone();
    for i in 1..m {
        if i % w != 0 {
            forward[i] = pick(forward[i - 1], padded[i]);
        }
    }
    for i in (0..m - 1).rev() {
        if (i + 1) % w != 0 {
            backward[i] = pick(backward[i + 1], padded[i]);
        }
    }
    (0..n)
        .map(|j| pick(backward[j], forward[j + w - 1]))
        .collect()
}
fn remove_spikes(profile: &[f32], radius: usize) -> Vec<f32> {
    if radius == 0 {
        return profile.to_vec();
    }
    envelope(&envelope(profile, radius, true), radius, false)
}
#[derive(Clone, Copy)]
struct Sample {
    /// Top-left pixel index; `OUTSIDE` lies beyond the image.
    index: u32,
    wx: f32,
    wy: f32,
}
const OUTSIDE: u32 = u32::MAX;
/// Precomputed bilinear sampling positions for one annular sector, stored
/// bin-major: all radii of bin 0, then bin 1, and so on.
struct PolarGrid {
    samples: Vec<Sample>,
    /// Radius of each radial sample, pixels.
    radius: Vec<f32>,
    bins: usize,
    radial: usize,
    width: usize,
    coverage: Vec<f32>,
}
/// Per-bin sums, counts and radius-weighted sums, then per-band sums and counts.
type Sums = (Vec<f32>, Vec<u32>, Vec<f32>, Vec<f32>, Vec<u32>);
/// One sector's angular profiles from a single sampling pass.
struct Profiles {
    profile: Vec<f32>,
    /// Intensity-weighted mean radius per bin (centroid detector only).
    radii: Vec<f32>,
    /// Up to eight radial bands, each an angular profile (WingBlur only).
    bands: Vec<Vec<f32>>,
}
impl PolarGrid {
    fn new(s: &Sector, width: u32, height: u32, bins: usize, radial: usize) -> Self {
        let radius: Vec<f32> = (0..radial)
            .map(|r| {
                s.inner_radius
                    + (s.outer_radius - s.inner_radius) * (r as f32 + 0.5) / radial as f32
            })
            .collect();
        let mut samples = Vec::with_capacity(bins * radial);
        let mut coverage = Vec::with_capacity(bins);
        for b in 0..bins {
            let a = (s.start_deg + s.span_deg * b as f32 / (bins - 1) as f32).to_radians();
            let mut count = 0;
            for &r in &radius {
                let p = s.hinge.at(r, a);
                if p.x >= 0.0 && p.y >= 0.0 && p.x < (width - 1) as f32 && p.y < (height - 1) as f32
                {
                    let x = p.x.floor() as usize;
                    let y = p.y.floor() as usize;
                    samples.push(Sample {
                        index: (y * width as usize + x) as u32,
                        wx: p.x - x as f32,
                        wy: p.y - y as f32,
                    });
                    count += 1;
                } else {
                    samples.push(Sample {
                        index: OUTSIDE,
                        wx: 0.0,
                        wy: 0.0,
                    });
                }
            }
            coverage.push(count as f32 / radial as f32);
        }
        Self {
            samples,
            radius,
            bins,
            radial,
            width: width as usize,
            coverage,
        }
    }
    /// Sum sampled values per bin, per radial band and weighted by radius.
    #[inline(always)]
    fn accumulate<F: Fn(usize, f32, f32) -> f32>(
        &self,
        value: F,
        radii: bool,
        bands: usize,
    ) -> Sums {
        let bins = self.bins;
        let mut sums = vec![0.0f32; bins];
        let mut counts = vec![0u32; bins];
        let mut weighted = if radii {
            vec![0.0f32; bins]
        } else {
            Vec::new()
        };
        let mut band_sums = vec![0.0f32; bins * bands];
        let mut band_counts = vec![0u32; bins * bands];
        let edges: Vec<usize> = (0..=bands)
            .map(|k| k * self.radial / bands.max(1))
            .collect();
        for (b, bin) in self.samples.chunks_exact(self.radial).enumerate() {
            if bands == 0 && !radii {
                let (mut sum, mut count) = (0.0f32, 0u32);
                for s in bin {
                    if s.index != OUTSIDE {
                        sum += value(s.index as usize, s.wx, s.wy);
                        count += 1;
                    }
                }
                sums[b] = sum;
                counts[b] = count;
                continue;
            }
            for band in 0..bands.max(1) {
                let (lo, hi) = if bands == 0 {
                    (0, self.radial)
                } else {
                    (edges[band], edges[band + 1])
                };
                let (mut sum, mut count, mut w) = (0.0f32, 0u32, 0.0f32);
                for (s, &r) in bin[lo..hi].iter().zip(&self.radius[lo..hi]) {
                    if s.index != OUTSIDE {
                        let v = value(s.index as usize, s.wx, s.wy);
                        sum += v;
                        w += v * r;
                        count += 1;
                    }
                }
                sums[b] += sum;
                counts[b] += count;
                if radii {
                    weighted[b] += w;
                }
                if bands > 0 {
                    band_sums[b * bands + band] = sum;
                    band_counts[b * bands + band] = count;
                }
            }
        }
        (sums, counts, weighted, band_sums, band_counts)
    }
    fn profiles(&self, pixels: Pixels, radii: bool, bands: usize) -> Profiles {
        let w = self.width;
        let (sums, counts, mut weighted, mut band_sums, band_counts) = match pixels {
            Pixels::Raw(p) | Pixels::Inverted(p) => self.accumulate(
                |i, wx, wy| {
                    let q = &p[i..i + w + 2];
                    bilinear(
                        q[0] as f32,
                        q[1] as f32,
                        q[w] as f32,
                        q[w + 1] as f32,
                        wx,
                        wy,
                    )
                },
                radii,
                bands,
            ),
            Pixels::Difference(p, b) => self.accumulate(
                |i, wx, wy| {
                    let (q, r) = (&p[i..i + w + 2], &b[i..i + w + 2]);
                    let d = |k: usize| q[k].abs_diff(r[k]) as f32;
                    bilinear(d(0), d(1), d(w), d(w + 1), wx, wy)
                },
                radii,
                bands,
            ),
        };
        let mut sums = sums;
        if let Pixels::Inverted(_) = pixels {
            // mean(255 − v) = 255 − mean(v): invert the sums, not every pixel.
            for (b, sum) in sums.iter_mut().enumerate() {
                let n = counts[b] as f32;
                if radii {
                    let lo = b * self.radial;
                    let radius_sum: f32 = self.samples[lo..lo + self.radial]
                        .iter()
                        .zip(&self.radius)
                        .filter(|(s, _)| s.index != OUTSIDE)
                        .map(|(_, r)| r)
                        .sum();
                    weighted[b] = 255.0 * radius_sum - weighted[b];
                }
                *sum = 255.0 * n - *sum;
            }
            for (sum, &n) in band_sums.iter_mut().zip(&band_counts) {
                *sum = 255.0 * n as f32 - *sum;
            }
        }
        let profile: Vec<f32> = sums
            .iter()
            .zip(&counts)
            .map(|(s, &n)| if n > 0 { s / (n as f32 * 255.0) } else { 0.0 })
            .collect();
        let radii = if radii {
            weighted
                .iter()
                .zip(&sums)
                .map(|(w, s)| if *s > 0.0 { w / s } else { 0.0 })
                .collect()
        } else {
            Vec::new()
        };
        let bands = (0..bands)
            .map(|band| {
                (0..self.bins)
                    .map(|b| {
                        let k = b * bands + band;
                        if band_counts[k] > 0 {
                            band_sums[k] / (band_counts[k] as f32 * 255.0)
                        } else {
                            0.0
                        }
                    })
                    .collect()
            })
            .collect();
        Profiles {
            profile,
            radii,
            bands,
        }
    }
    /// Median signed derivative across radial bands, after limiting contrast
    /// and removing angular pulses narrower than `spike_radius` bins. Also
    /// returns the median filtered level above each band's background, which
    /// shows where the broad wing shoulder is.
    fn consensus_gradient(
        &self,
        mut bands: Vec<Vec<f32>>,
        contrast_cap: f32,
        spike_radius: usize,
    ) -> (Vec<f32>, Vec<f32>) {
        let (gradients, levels): (Vec<Vec<f32>>, Vec<Vec<f32>>) = bands
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
                let opened = remove_spikes(p, spike_radius);
                let level = opened.iter().map(|v| (v - baseline).max(0.0)).collect();
                (smooth_gradient(&opened), level)
            })
            .unzip();
        let count = gradients.len();
        let across = |rows: &[Vec<f32>], i: usize| {
            let mut values = [0.0; 8];
            for (v, row) in values.iter_mut().zip(rows) {
                *v = row[i];
            }
            median(&mut values[..count])
        };
        (0..self.bins)
            .map(|i| (across(&gradients, i), across(&levels, i)))
            .unzip()
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
        let pixels = if subtract {
            let bg = background.ok_or_else(|| {
                anyhow::anyhow!("Background subtraction requested but no reference is loaded")
            })?;
            ensure!(
                bg.width == raw.width && bg.height == raw.height,
                "Background dimensions differ from camera frame"
            );
            Pixels::Difference(&raw.pixels, &bg.pixels)
        } else if self.config.tracking.dark_on_light {
            Pixels::Inverted(&raw.pixels)
        } else {
            Pixels::Raw(&raw.pixels)
        };
        let states: [PartState; 4] = std::array::from_fn(|i| self.track(i, pixels));
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
    fn track(&self, index: usize, pixels: Pixels) -> PartState {
        let s = self.config.sectors()[index];
        if !s.enabled {
            return PartState::default();
        }
        let grid = &self.grids[index];
        let bands = if s.method == Method::WingBlur {
            grid.radial.min(8)
        } else {
            0
        };
        let Profiles {
            profile,
            radii,
            bands,
        } = grid.profiles(pixels, s.method == Method::Centroid, bands);
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
                let (gradient, level) = if s.method == Method::WingBlur {
                    grid.consensus_gradient(bands, s.contrast_cap, spike_radius)
                } else {
                    (smooth_gradient(&profile), Vec::new())
                };
                // A bin is measurable only if its whole filter footprint is
                // inside the image (≥90% radial coverage).
                let reach = 2 + spike_radius * 2;
                let mut bad = vec![0usize; grid.bins + 1];
                for (i, c) in grid.coverage.iter().enumerate() {
                    bad[i + 1] = bad[i] + usize::from(*c < 0.9);
                }
                let valid_bins: Vec<bool> = (0..grid.bins)
                    .map(|i| {
                        let lo = i.saturating_sub(reach);
                        let hi = (i + reach).min(grid.bins - 1);
                        bad[hi + 1] == bad[lo]
                    })
                    .collect();
                let valid = |i: usize| valid_bins[i];
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
                if s.method == Method::WingBlur {
                    // A wing edge borders a broad shoulder of blur; a leg or
                    // line is narrow, even if as dark or as sharp. Rank edges
                    // by the shoulder behind them, not by their steepness;
                    // the gradient peak still gives the precise angle.
                    let bin_deg = s.span_deg / (grid.bins - 1) as f32;
                    let window = ((s.spike_width_deg * 4.0)
                        .clamp(12.0, 45.0)
                        .min(s.span_deg / 3.0)
                        / bin_deg)
                        .round()
                        .max(2.0) as usize;
                    let mean = |lo: usize, hi: usize| {
                        let (sum, n) = (lo..hi.min(grid.bins))
                            .filter(|&b| grid.coverage[b] >= 0.9)
                            .fold((0.0, 0), |(s, n), b| (s + level[b], n + 1));
                        (if n > 0 { sum / n as f32 } else { 0.0 }, n)
                    };
                    // Long windows on both sides: a narrow line, even a dark
                    // or sharp one, has little blur behind it. A window cut
                    // to under half by the sector end is weaker evidence, so
                    // an edge there cannot win on a few bins of contrast.
                    let shoulder = |i: usize| {
                        let (after, a) = mean(i + 1, i + 1 + window);
                        let (before, b) = mean(i.saturating_sub(window), i);
                        let complete = (a.min(b) as f32 * 2.0 / window as f32).min(1.0);
                        complete
                            * if gradient[i] > 0.0 {
                                after - before
                            } else {
                                before - after
                            }
                    };
                    // One candidate per edge: the steepest point of each
                    // same-sign gradient lobe gives the angle.
                    let lobe_end = |i: usize, step: isize| {
                        let mut j = i as isize;
                        while (0..grid.bins as isize).contains(&(j + step))
                            && gradient[(j + step) as usize] * gradient[i] > 0.0
                        {
                            j += step;
                        }
                        j as usize
                    };
                    let mut peaks: Vec<usize> = Vec::new();
                    for &i in &candidates {
                        let (lo, hi) = (lobe_end(i, -1), lobe_end(i, 1));
                        match peaks.iter_mut().find(|j| (lo..=hi).contains(*j)) {
                            Some(j) if gradient[i].abs() > gradient[*j].abs() => *j = i,
                            Some(_) => {}
                            None => peaks.push(i),
                        }
                    }
                    // Both must hold: a steep edge (not a wiggle near the
                    // wing) and a broad shoulder behind it (not a line).
                    let floor = 1e-3 * s.contrast_cap;
                    let mut ranked: Vec<(usize, f32)> = peaks
                        .iter()
                        .map(|&i| (i, gradient[i].abs() * (shoulder(i).max(0.0) + floor)))
                        .collect();
                    ranked.sort_unstable_by(|(a, sa), (b, sb)| {
                        sb.total_cmp(sa)
                            .then(gradient[*b].abs().total_cmp(&gradient[*a].abs()))
                    });
                    candidates = ranked.into_iter().map(|(i, _)| i).collect();
                    state.filtered = level.clone();
                } else {
                    candidates
                        .sort_unstable_by(|a, b| gradient[*b].abs().total_cmp(&gradient[*a].abs()));
                }
                // The leading edge is where the wing gives way to background
                // toward the head; the trailing edge is the reverse. Choosing
                // by polarity, not position, keeps a stray feature beyond the
                // wing (an antenna, the arena wall) from taking its place.
                let polarity = match s.primary_edge {
                    PrimaryEdge::Strongest => None,
                    PrimaryEdge::TowardHead => Some(-self.sense(index)),
                    PrimaryEdge::TowardTail => Some(self.sense(index)),
                };
                let by_polarity = polarity
                    .and_then(|sign| candidates.iter().position(|&i| gradient[i] * sign > 0.0));
                if let Some(k) = by_polarity {
                    let i = candidates.remove(k);
                    candidates.insert(0, i);
                }
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
                        _ if by_polarity.is_some() => 0,
                        // No edge of that polarity: the outermost one.
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fast_envelope_matches_a_direct_window_scan() {
        let profile: Vec<f32> = (0..97).map(|i| ((i * 37 % 23) as f32).sin()).collect();
        for radius in [1, 2, 5, 13, 60] {
            for minimum in [true, false] {
                let direct: Vec<f32> = (0..profile.len())
                    .map(|i| {
                        let lo = i.saturating_sub(radius);
                        let hi = (i + radius).min(profile.len() - 1);
                        let w = profile[lo..=hi].iter().copied();
                        if minimum {
                            w.fold(f32::INFINITY, f32::min)
                        } else {
                            w.fold(f32::NEG_INFINITY, f32::max)
                        }
                    })
                    .collect();
                assert_eq!(envelope(&profile, radius, minimum), direct);
            }
        }
    }
}
