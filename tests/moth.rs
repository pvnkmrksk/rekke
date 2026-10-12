//! The Bogong moth example: real footage of a tethered moth whose antennae
//! and legs reach into the wing sectors, filmed at 30 fps so every frame
//! catches a different part of the wingbeat.
use rekkefly::{
    avi::AviReader,
    config::{Config, Method},
    frame::GrayFrame,
    tracker::{Measurement, Tracker},
};
use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
fn config(method: Method) -> Config {
    let mut c = Config::load(&root().join("configs/bogong-moth.toml")).unwrap();
    c.tracking.left.method = method;
    c.tracking.right.method = method;
    c
}
/// Every frame of the bundled example, decoded natively.
fn frames() -> impl Iterator<Item = GrayFrame> {
    let mut reader = AviReader::open(&root().join("samples/bogong-moth.avi")).unwrap();
    let (width, height) = (reader.info().width, reader.info().height);
    let (mut chunk, mut pixels) = (Vec::new(), Vec::new());
    let mut sequence = 0;
    std::iter::from_fn(move || {
        reader
            .next_frame(&mut chunk, &mut pixels)
            .unwrap()
            .then(|| {
                sequence += 1;
                GrayFrame::new(width, height, pixels.clone(), 0, sequence - 1).unwrap()
            })
    })
}
fn degrees(a: Option<f32>) -> String {
    a.map(|a| format!("{:.2}", a.to_degrees()))
        .unwrap_or_default()
}
fn edges(angles: &[f32]) -> String {
    angles
        .iter()
        .map(|a| format!("{:.1}", a.to_degrees()))
        .collect::<Vec<_>>()
        .join(";")
}
/// Per-frame angles from both detectors, for charts and overlays:
/// `REKKEFLY_MOTH=moth.csv cargo test --release --test moth csv -- --ignored`
#[test]
#[ignore]
fn csv_of_both_detectors() {
    let Some(path) = std::env::var_os("REKKEFLY_MOTH") else {
        return;
    };
    let robust = config(Method::WingBlur);
    let classic = config(Method::Edge);
    let (w, h, _) = robust.source.dimensions();
    let (tr, tc) = (Tracker::new(&robust, w, h), Tracker::new(&classic, w, h));
    let mut out = String::from(
        "frame,robust_left,robust_right,robust_lr,classic_left,classic_right,classic_lr,\
         robust_left_image,robust_right_image,classic_left_image,classic_right_image,robust_us,\
         robust_left_edges,robust_right_edges,classic_left_edges,classic_right_edges\n",
    );
    for frame in frames() {
        let r: Measurement = tr.process(&frame, None, 0).unwrap();
        let c = tc.process(&frame, None, 0).unwrap();
        out += &format!(
            "{},{},{},{},{},{},{},{},{},{},{},{:.1},{},{},{},{}\n",
            frame.sequence,
            degrees(r.left.angle_rad),
            degrees(r.right.angle_rad),
            degrees(r.left_minus_right_rad),
            degrees(c.left.angle_rad),
            degrees(c.right.angle_rad),
            degrees(c.left_minus_right_rad),
            degrees(r.left.image_angle_rad),
            degrees(r.right.image_angle_rad),
            degrees(c.left.image_angle_rad),
            degrees(c.right.image_angle_rad),
            r.processing_us,
            edges(&r.left.image_angles),
            edges(&r.right.image_angles),
            edges(&c.left.image_angles),
            edges(&c.right.image_angles),
        );
    }
    std::fs::write(path, out).unwrap();
}
/// L−R noise: deviation from a 9-frame running median, in degrees, and how
/// well left and right move together (the wings beat in phase).
#[derive(Debug, Clone, Copy)]
struct Agreement {
    correlation: f32,
    median: f32,
    p95: f32,
    over_15: usize,
    over_30: usize,
    missing: usize,
}
fn agreement(left: &[Option<f32>], right: &[Option<f32>]) -> Agreement {
    let pairs: Vec<(f32, f32)> = left
        .iter()
        .zip(right)
        .filter_map(|(l, r)| Some((l.as_ref()?.to_degrees(), r.as_ref()?.to_degrees())))
        .collect();
    let n = pairs.len() as f32;
    let (ml, mr) = pairs
        .iter()
        .fold((0.0, 0.0), |(a, b), (l, r)| (a + l / n, b + r / n));
    let (mut sl, mut sr, mut slr) = (0.0, 0.0, 0.0);
    for (l, r) in &pairs {
        sl += (l - ml) * (l - ml);
        sr += (r - mr) * (r - mr);
        slr += (l - ml) * (r - mr);
    }
    let difference: Vec<f32> = pairs.iter().map(|(l, r)| l - r).collect();
    let mut deviation: Vec<f32> = (0..difference.len())
        .map(|i| {
            let lo = i.saturating_sub(4);
            let hi = (i + 5).min(difference.len());
            let mut window = difference[lo..hi].to_vec();
            window.sort_by(f32::total_cmp);
            (difference[i] - window[window.len() / 2]).abs()
        })
        .collect();
    deviation.sort_by(f32::total_cmp);
    let at = |q: f32| deviation[((deviation.len() - 1) as f32 * q) as usize];
    Agreement {
        correlation: slr / (sl * sr).sqrt(),
        median: at(0.5),
        p95: at(0.95),
        over_15: deviation.iter().filter(|d| **d > 15.0).count(),
        over_30: deviation.iter().filter(|d| **d > 30.0).count(),
        missing: left.len() - pairs.len(),
    }
}
fn evaluate(config: &Config, frames: &[GrayFrame]) -> Agreement {
    let (w, h, _) = config.source.dimensions();
    let tracker = Tracker::new(config, w, h);
    let (left, right): (Vec<_>, Vec<_>) = frames
        .iter()
        .map(|f| {
            let m = tracker.process(f, None, 0).unwrap();
            (m.left.angle_rad, m.right.angle_rad)
        })
        .unzip();
    agreement(&left, &right)
}
/// `cargo test --release --test moth sweep -- --ignored --nocapture`
#[test]
#[ignore]
fn sweep_detector_settings() {
    let frames: Vec<GrayFrame> = frames().collect();
    for method in [Method::WingBlur, Method::Edge] {
        for (inner, outer) in [(50.0, 115.0), (50.0, 120.0), (55.0, 120.0), (60.0, 120.0)] {
            for end in [50.0, 60.0, 70.0, 80.0, 90.0] {
                let mut c = config(method);
                for s in [&mut c.tracking.left, &mut c.tracking.right] {
                    s.inner_radius = inner;
                    s.outer_radius = outer;
                }
                c.tracking.left.span_deg = end + 75.0;
                c.tracking.right.span_deg = end + 75.0;
                c.tracking.right.start_deg = 180.0 - end;
                let a = evaluate(&c, &frames);
                println!(
                    "{method:?} inner {inner} outer {outer} end {end}: corr {:.2} dev median {:.1} p95 {:.1} >15 {} >30 {} missing {}",
                    a.correlation, a.median, a.p95, a.over_15, a.over_30, a.missing
                );
            }
        }
    }
}
