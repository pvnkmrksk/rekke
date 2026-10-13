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
    let frames: Vec<GrayFrame> = frames().step_by(2).collect();
    let painted: Vec<GrayFrame> = {
        let c = config(Method::WingBlur);
        frames
            .iter()
            .enumerate()
            .map(|(k, f)| antennae(f, &c, k as f32 * 0.067, 2.0, 150))
            .collect()
    };
    let pairs: Vec<(f32, f32)> = std::env::var("RADII")
        .unwrap_or("20:150".into())
        .split(',')
        .map(|p| {
            let (a, b) = p.split_once(':').unwrap();
            (a.parse().unwrap(), b.parse().unwrap())
        })
        .collect();
    let spans: Vec<(f32, f32)> = std::env::var("SPANS")
        .unwrap_or("-75:55".into())
        .split(',')
        .map(|p| {
            let (a, b) = p.split_once(':').unwrap();
            (a.parse().unwrap(), b.parse().unwrap())
        })
        .collect();
    for &(inner, outer) in &pairs {
        for &(start, end) in &spans {
            for method in [Method::WingBlur, Method::Edge] {
                let mut c = config(method);
                for s in [&mut c.tracking.left, &mut c.tracking.right] {
                    s.inner_radius = inner;
                    s.outer_radius = outer;
                    s.span_deg = end - start;
                }
                c.tracking.left.start_deg = start;
                c.tracking.right.start_deg = 180.0 - end;
                let a = evaluate(&c, &frames);
                let tracker = Tracker::new(&c, 640, 480);
                let stuck = frames
                    .iter()
                    .filter(|f| {
                        let m = tracker.process(f, None, 0).unwrap();
                        let near = |s: &rekkefly::tracker::PartState, start: f32| {
                            s.angle_rad
                                .is_some_and(|a| (a.to_degrees() - start).abs() < 7.0)
                        };
                        near(&m.left, start) || near(&m.right, start)
                    })
                    .count();
                let moved = frames
                    .iter()
                    .zip(&painted)
                    .filter(|(f, p)| {
                        let a = tracker.process(f, None, 0).unwrap().left_minus_right_rad;
                        let b = tracker.process(p, None, 0).unwrap().left_minus_right_rad;
                        a.zip(b)
                            .is_none_or(|(a, b)| (a - b).abs().to_degrees() > 5.0)
                    })
                    .count();
                println!(
                    "{method:?} inner {inner} outer {outer} {start}..{end}: corr {:.2} dev median {:.1} p95 {:.1} >30 {} at start {stuck} | antennae moved {moved}/{}",
                    a.correlation,
                    a.median,
                    a.p95,
                    a.over_30,
                    frames.len()
                );
            }
        }
    }
}
/// Thin bright lines from the head that sweep through both wing sectors,
/// like antennae or legs: `width` px wide at gray level `level`, crossing
/// each side at an angle that drifts over time. Image angles, degrees.
fn antennae(frame: &GrayFrame, config: &Config, t: f32, width: f32, level: u8) -> GrayFrame {
    let mut out = frame.clone();
    let origin = config.head;
    for (side, phase) in [(1.0_f32, 0.0_f32), (-1.0, 1.7)] {
        // From pointing along the body toward the head (90°) out to the sides.
        let sweep = 35.0 + 30.0 * (t * 1.9 + phase).sin();
        let angle = (90.0 - side * (90.0 - sweep)).to_radians() + std::f32::consts::PI;
        let (dx, dy) = (angle.cos(), angle.sin());
        let length = 260.0;
        let steps = (length * 2.0) as usize;
        for k in 0..steps {
            let s = k as f32 / 2.0;
            let (cx, cy) = (origin.x + dx * s, origin.y + dy * s);
            let r = width / 2.0 + 0.5;
            for y in (cy - r).floor() as i64..=(cy + r).ceil() as i64 {
                for x in (cx - r).floor() as i64..=(cx + r).ceil() as i64 {
                    if x < 0 || y < 0 || x >= frame.width as i64 || y >= frame.height as i64 {
                        continue;
                    }
                    let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                    let cover = (width / 2.0 + 0.5 - d).clamp(0.0, 1.0);
                    let i = y as usize * frame.width as usize + x as usize;
                    let v = out.pixels[i] as f32;
                    out.pixels[i] = v.max(v + (level as f32 - v) * cover) as u8;
                }
            }
        }
    }
    out
}
/// `cargo test --release --test moth stress_table -- --ignored --nocapture`
#[test]
#[ignore]
fn stress_table() {
    let frames: Vec<GrayFrame> = frames().collect();
    for (name, width, level) in [
        ("antennae 2 px, gray 150", 2.0, 150u8),
        ("antennae 3 px, gray 200", 3.0, 200),
        ("antennae 1.5 px, gray 110", 1.5, 110),
        ("legs 5 px, gray 170", 5.0, 170),
    ] {
        for method in [Method::WingBlur, Method::Edge] {
            let c = config(method);
            let tracker = Tracker::new(&c, 640, 480);
            let other = Tracker::new(
                &config(if method == Method::Edge {
                    Method::WingBlur
                } else {
                    Method::Edge
                }),
                640,
                480,
            );
            let mut changed = 0;
            let mut clear_changed = 0;
            let mut clear = 0;
            let mut worst: f32 = 0.0;
            let mut total = 0;
            let mut bad = Vec::new();
            for (k, f) in frames.iter().enumerate() {
                let clean = tracker.process(f, None, 0).unwrap().left_minus_right_rad;
                let second = other.process(f, None, 0).unwrap().left_minus_right_rad;
                let unambiguous = matches!((clean, second), (Some(a), Some(b)) if (a - b).abs().to_degrees() < 5.0);
                clear += usize::from(unambiguous);
                let dirty = antennae(f, &c, k as f32 / 30.0, width, level);
                let lr = tracker
                    .process(&dirty, None, 0)
                    .unwrap()
                    .left_minus_right_rad;
                let d = match (clean, lr) {
                    (Some(a), Some(b)) => (a - b).abs().to_degrees(),
                    _ => 180.0,
                };
                total += 1;
                changed += usize::from(d > 5.0);
                if unambiguous && d > 5.0 {
                    clear_changed += 1;
                    bad.push(k);
                }
                worst = worst.max(d);
            }
            println!(
                "{name:28} {method:?}: L−R moved >5° in {changed}/{total}, worst {worst:.1}°; on {clear} unambiguous frames {clear_changed} {:?}",
                &bad[..bad.len().min(8)]
            );
        }
    }
}
/// `REKKEFLY_MOTH_FRAMES=68,132 REKKEFLY_MOTH_OUT=dir cargo test --release --test moth dump -- --ignored`
#[test]
#[ignore]
fn dump_stress_overlays() {
    let (Some(list), Some(out)) = (
        std::env::var_os("REKKEFLY_MOTH_FRAMES"),
        std::env::var_os("REKKEFLY_MOTH_OUT"),
    ) else {
        return;
    };
    let wanted: Vec<usize> = list
        .to_string_lossy()
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let out = std::path::PathBuf::from(out);
    for (k, f) in frames().enumerate().filter(|(k, _)| wanted.contains(k)) {
        for method in [Method::WingBlur, Method::Edge] {
            let c = config(method);
            let tracker = Tracker::new(&c, 640, 480);
            for (tag, frame) in [
                ("clean", f.clone()),
                ("antennae", antennae(&f, &c, k as f32 / 30.0, 2.0, 150)),
            ] {
                let m = tracker.process(&frame, None, 0).unwrap();
                let png = rekkefly::overlay::render(&frame, &c, Some(&m))
                    .to_png()
                    .unwrap();
                std::fs::write(out.join(format!("{k}-{method:?}-{tag}.png")), png).unwrap();
            }
        }
    }
}
/// The README's real-footage chart: L − R with sweeping antennae added,
/// from both detectors, beside the clean robust trace.
/// `REKKEFLY_MOTH_TRACE=moth.csv cargo test --release --test moth antenna_trace -- --ignored`
#[test]
#[ignore]
fn antenna_trace() {
    let Some(path) = std::env::var_os("REKKEFLY_MOTH_TRACE") else {
        return;
    };
    let (robust, classic) = (config(Method::WingBlur), config(Method::Edge));
    let (tr, tc) = (
        Tracker::new(&robust, 640, 480),
        Tracker::new(&classic, 640, 480),
    );
    let mut out = String::from("t,clean,robust,classic,classic_clean\n");
    for (k, f) in frames().enumerate() {
        let t = k as f32 / 30.0;
        let dirty = antennae(&f, &robust, t, 2.0, 150);
        let lr = |tracker: &Tracker, frame: &GrayFrame| {
            degrees(
                tracker
                    .process(frame, None, 0)
                    .unwrap()
                    .left_minus_right_rad,
            )
        };
        out += &format!(
            "{t:.4},{},{},{},{}\n",
            lr(&tr, &f),
            lr(&tr, &dirty),
            lr(&tc, &dirty),
            lr(&tc, &f)
        );
    }
    std::fs::write(path, out).unwrap();
}
/// A deterministic perturbation of a real frame.
fn perturb(frame: &GrayFrame, kind: &str, seed: u64) -> GrayFrame {
    let mut out = frame.clone();
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut noise = || {
        // Sum of four uniforms: close enough to Gaussian, σ ≈ 1.
        let mut s = 0.0;
        for _ in 0..4 {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            s += (state >> 40) as f32 / (1u64 << 24) as f32 - 0.5;
        }
        s * 1.732
    };
    match kind {
        "darker" | "brighter" => {
            let gain = if kind == "darker" { 0.75 } else { 1.3 };
            for p in &mut out.pixels {
                *p = (*p as f32 * gain).round().clamp(0.0, 255.0) as u8;
            }
        }
        "noise" => {
            for p in &mut out.pixels {
                *p = (*p as f32 + 4.0 * noise()).round().clamp(0.0, 255.0) as u8;
            }
        }
        "recompressed" => {
            let mut jpeg = Vec::new();
            rekkefly::avi::encode(
                rekkefly::avi::VideoFormat::Mjpeg,
                40,
                false,
                frame.width,
                frame.height,
                &frame.pixels,
                &mut jpeg,
            )
            .unwrap();
            let image = image::load_from_memory(&jpeg).unwrap().to_luma8();
            out.pixels = image.into_raw();
        }
        "blurred" => {
            let image = image::GrayImage::from_raw(frame.width, frame.height, frame.pixels.clone())
                .unwrap();
            out.pixels = image::imageops::blur(&image, 1.5).into_raw();
        }
        _ => unreachable!(),
    }
    out
}
/// `CAPS=0.12,0.25 cargo test --release --test moth perturbation_table -- --ignored --nocapture`
#[test]
#[ignore]
fn perturbation_table() {
    let frames: Vec<GrayFrame> = frames().step_by(3).collect();
    let caps: Vec<f32> = std::env::var("CAPS")
        .unwrap_or("0.12".into())
        .split(',')
        .map(|v| v.parse().unwrap())
        .collect();
    for (method, cap) in caps
        .iter()
        .map(|&c| (Method::WingBlur, c))
        .chain([(Method::Edge, 0.12)])
    {
        let mut c = config(method);
        c.tracking.left.contrast_cap = cap;
        c.tracking.right.contrast_cap = cap;
        println!("cap {cap}");
        let tracker = Tracker::new(&c, 640, 480);
        let clean: Vec<_> = frames
            .iter()
            .map(|f| tracker.process(f, None, 0).unwrap())
            .collect();
        let (l, r): (Vec<_>, Vec<_>) = clean
            .iter()
            .map(|m| (m.left.angle_rad, m.right.angle_rad))
            .unzip();
        println!("{method:?} clean: {:?}", agreement(&l, &r));
        let other = Tracker::new(
            &config(if method == Method::Edge {
                Method::WingBlur
            } else {
                Method::Edge
            }),
            640,
            480,
        );
        let clear: Vec<bool> = frames
            .iter()
            .zip(&clean)
            .map(|(f, m)| {
                let o = other.process(f, None, 0).unwrap();
                matches!((m.left_minus_right_rad, o.left_minus_right_rad), (Some(a), Some(b)) if (a - b).abs().to_degrees() < 5.0)
            })
            .collect();
        for kind in [
            "darker",
            "brighter",
            "noise",
            "recompressed",
            "blurred",
            "antennae",
        ] {
            let mut moved = 0;
            let mut clear_moved = 0;
            for (k, (f, m)) in frames.iter().zip(&clean).enumerate() {
                let g = if kind == "antennae" {
                    antennae(f, &c, k as f32 * 0.1, 2.0, 150)
                } else {
                    perturb(f, kind, k as u64)
                };
                let n = tracker.process(&g, None, 0).unwrap();
                let d = match (m.left_minus_right_rad, n.left_minus_right_rad) {
                    (Some(a), Some(b)) => (a - b).abs().to_degrees(),
                    _ => 180.0,
                };
                moved += usize::from(d > 5.0);
                clear_moved += usize::from(d > 5.0 && clear[k]);
            }
            println!(
                "  {kind:13} L−R moved >5° in {moved}/{}; on {} unambiguous frames {clear_moved}",
                frames.len(),
                clear.iter().filter(|c| **c).count()
            );
        }
    }
}

/// How often `perturbed` moves L − R by more than 5° from the clean frame.
fn moved(
    method: Method,
    frames: &[GrayFrame],
    perturbed: impl Fn(usize, &GrayFrame, &Config) -> GrayFrame,
) -> usize {
    let c = config(method);
    let tracker = Tracker::new(&c, 640, 480);
    frames
        .iter()
        .enumerate()
        .filter(|(k, f)| {
            let clean = tracker.process(f, None, 0).unwrap().left_minus_right_rad;
            let dirty = tracker
                .process(&perturbed(*k, f, &c), None, 0)
                .unwrap()
                .left_minus_right_rad;
            match (clean, dirty) {
                (Some(a), Some(b)) => (a - b).abs().to_degrees() > 5.0,
                _ => true,
            }
        })
        .count()
}
// Fast checks on every third frame of the real footage. Bounds leave room
// for float differences between platforms, which can flip an ambiguous frame.
#[test]
fn the_example_follows_both_wings_of_the_real_moth() {
    let frames: Vec<GrayFrame> = frames().step_by(3).collect();
    assert_eq!(frames.len(), 300);
    let a = evaluate(&config(Method::WingBlur), &frames);
    // Both wings beat together, so L − R barely changes from frame to frame
    // although each frame catches a different part of the stroke.
    assert_eq!(a.missing, 0, "{a:?}");
    assert!(a.median < 9.0, "{a:?}");
    assert!(a.p95 < 40.0, "{a:?}");
    assert!(a.over_30 <= 25, "{a:?}");
    // With the markers on the wing bases the classic edge often takes the
    // body and legs beside them instead.
    let classic = evaluate(&config(Method::Edge), &frames);
    assert!(classic.over_30 > a.over_30 * 2, "{classic:?} vs {a:?}");
}
#[test]
fn painted_antennae_hardly_move_rekkefly_but_take_the_classic_edge() {
    let frames: Vec<GrayFrame> = frames().step_by(3).collect();
    let painted = |k: usize, f: &GrayFrame, c: &Config| antennae(f, c, k as f32 * 0.1, 2.0, 150);
    let robust = moved(Method::WingBlur, &frames, painted);
    let classic = moved(Method::Edge, &frames, painted);
    assert!(robust <= 12, "Rekkefly moved on {robust}/300 frames");
    assert!(classic >= 120, "classic moved on only {classic}/300 frames");
}
#[test]
fn blur_noise_and_recompression_leave_most_frames_alone() {
    let frames: Vec<GrayFrame> = frames().step_by(9).collect();
    for (kind, limit) in [
        ("blurred", 8),
        ("noise", 8),
        ("recompressed", 8),
        ("darker", 8),
    ] {
        let n = moved(Method::WingBlur, &frames, |k, f, _| {
            perturb(f, kind, k as u64)
        });
        assert!(n <= limit, "{kind}: L−R moved on {n}/100 frames");
    }
}
/// Many rigs on one computer: each its own engine, all at full speed.
#[test]
fn eight_rigs_track_the_moth_at_once() {
    use rekkefly::{
        config::SourceConfig,
        engine::{Engine, StartOptions},
    };
    let dir = tempfile::tempdir().unwrap();
    let engines: Vec<Engine> = (0..8)
        .map(|k| {
            let mut c = config(Method::WingBlur);
            c.instance = format!("rig-{k}");
            c.output.enabled = false;
            c.output.zmq_enabled = false;
            c.source = SourceConfig::Video {
                path: root()
                    .join("samples/bogong-moth.avi")
                    .to_string_lossy()
                    .into_owned(),
                width: 640,
                height: 480,
                fps: 30,
                repeat: true,
                realtime: false,
            };
            let options = StartOptions {
                limit: Some(150),
                ..Default::default()
            };
            Engine::start_with(c, dir.path().join(format!("rig-{k}.toml")), options).unwrap()
        })
        .collect();
    for engine in engines {
        while !engine.is_finished() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let state = engine.state();
        assert!(state.error.is_none(), "{:?}", state.error);
        assert_eq!(state.frames, 150);
        let m = &state.latest.unwrap().measurement;
        assert!(m.left.angle_rad.is_some() && m.right.angle_rad.is_some());
    }
}
/// Frames per second for 1–8 rigs at once on the moth, each decoding and
/// tracking its own copy of the video as fast as it can.
/// `cargo test --release --test moth throughput -- --ignored --nocapture`
#[test]
#[ignore]
fn throughput_table() {
    use rekkefly::{
        config::SourceConfig,
        engine::{Engine, StartOptions},
    };
    let dir = tempfile::tempdir().unwrap();
    for rigs in [1, 2, 4, 8] {
        let start = std::time::Instant::now();
        let engines: Vec<Engine> = (0..rigs)
            .map(|k| {
                let mut c = config(Method::WingBlur);
                c.instance = format!("rig-{k}");
                c.output.enabled = false;
                c.output.zmq_enabled = false;
                c.source = SourceConfig::Video {
                    path: root()
                        .join("samples/bogong-moth.avi")
                        .to_string_lossy()
                        .into_owned(),
                    width: 640,
                    height: 480,
                    fps: 30,
                    repeat: false,
                    realtime: false,
                };
                Engine::start_with(
                    c,
                    dir.path().join(format!("rig-{k}.toml")),
                    StartOptions::default(),
                )
                .unwrap()
            })
            .collect();
        let mut frames = 0;
        for engine in engines {
            while !engine.is_finished() {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            frames += engine.state().frames;
        }
        let seconds = start.elapsed().as_secs_f64();
        println!(
            "{rigs} rigs: {frames} frames in {seconds:.2} s = {:.0} frames/s in total, {:.0} per rig",
            frames as f64 / seconds,
            frames as f64 / seconds / rigs as f64
        );
    }
}
/// The website's real-footage videos and the angles shown beside them, made
/// from the raw frames (never inverted) with the tracking drawn on top:
/// `moth-tracking` (Rekkefly on the clean footage) and `moth-antennae`
/// (painted antennae: the classic detector left, Rekkefly right), each as
/// MP4 and WebM, and `moth-angles.json`. Needs FFmpeg with libx264 and
/// libvpx-vp9; `python3 scripts/web_media.py` runs this and puts the files in place.
/// `REKKEFLY_WEB_OUT=dir cargo test --release --test moth web_media -- --ignored`
#[test]
#[ignore]
fn web_media() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let Some(out) = std::env::var_os("REKKEFLY_WEB_OUT") else {
        return;
    };
    let out = std::path::PathBuf::from(out);
    let ffmpeg = std::env::var_os("REKKEFLY_FFMPEG").unwrap_or_else(|| "ffmpeg".into());
    // Each video as H.264 MP4 and, for browsers without H.264, VP9 WebM.
    let encoder = |name: &str, width: u32, height: u32, codec: &[&str]| {
        Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "-r", "30"])
            .args(["-s", &format!("{width}x{height}"), "-i", "-"])
            .args(codec)
            .args(["-pix_fmt", "yuv420p", "-an"])
            .arg(out.join(name))
            .stdin(Stdio::piped())
            .spawn()
            .expect("FFmpeg with libx264 and libvpx-vp9")
    };
    let h264: Vec<&str> = "-c:v libx264 -preset veryslow -crf 27 -movflags +faststart"
        .split(' ')
        .collect();
    let vp9: Vec<&str> = "-c:v libvpx-vp9 -crf 46 -b:v 0 -row-mt 1 -deadline good -cpu-used 2"
        .split(' ')
        .collect();
    let pair = |name: &str, width: u32, height: u32| {
        [
            encoder(&format!("{name}.mp4"), width, height, &h264),
            encoder(&format!("{name}.webm"), width, height, &vp9),
        ]
    };
    let (robust, classic) = (config(Method::WingBlur), config(Method::Edge));
    let (tr, tc) = (
        Tracker::new(&robust, 640, 480),
        Tracker::new(&classic, 640, 480),
    );
    let gap = 8;
    let mut tracking = pair("moth-tracking", 640, 480);
    let mut compare = pair("moth-antennae", 640 * 2 + gap, 480);
    let feed = |children: &mut [std::process::Child; 2], pixels: &[u8]| {
        for child in children.iter_mut() {
            child.stdin.as_mut().unwrap().write_all(pixels).unwrap();
        }
    };
    let round = |a: Option<f32>| a.map(|a| (a.to_degrees() * 10.0).round() / 10.0);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    // L − R with the antennae, and each detector's own result without them.
    let (mut lr_classic, mut lr_robust, mut clean_classic, mut clean_robust) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (k, f) in frames().enumerate() {
        let m = tr.process(&f, None, 0).unwrap();
        left.push(round(m.left.angle_rad));
        right.push(round(m.right.angle_rad));
        clean_robust.push(round(m.left_minus_right_rad));
        clean_classic.push(round(tc.process(&f, None, 0).unwrap().left_minus_right_rad));
        let canvas = rekkefly::overlay::render(&f, &robust, Some(&m));
        feed(&mut tracking, &canvas.pixels);
        // The same painted antennae as the README's chart.
        let dirty = antennae(&f, &robust, k as f32 / 30.0, 2.0, 150);
        let (mc, mr) = (
            tc.process(&dirty, None, 0).unwrap(),
            tr.process(&dirty, None, 0).unwrap(),
        );
        lr_classic.push(round(mc.left_minus_right_rad));
        lr_robust.push(round(mr.left_minus_right_rad));
        let a = rekkefly::overlay::render(&dirty, &classic, Some(&mc));
        let b = rekkefly::overlay::render(&dirty, &robust, Some(&mr));
        let row = (640 * 2 + gap) as usize * 3;
        let mut both = vec![11u8; row * 480];
        for y in 0..480 {
            let (src, dst) = (y * 640 * 3, y * row);
            both[dst..dst + 640 * 3].copy_from_slice(&a.pixels[src..src + 640 * 3]);
            let at = dst + (640 + gap as usize) * 3;
            both[at..at + 640 * 3].copy_from_slice(&b.pixels[src..src + 640 * 3]);
        }
        feed(&mut compare, &both);
    }
    for mut child in tracking.into_iter().chain(compare) {
        drop(child.stdin.take());
        assert!(child.wait().unwrap().success());
    }
    let json = serde_json::json!({
        "fps": 30,
        "tracking": {"left": left, "right": right},
        "antennae": {
            "classic": lr_classic,
            "rekkefly": lr_robust,
            "clean": {"classic": clean_classic, "rekkefly": clean_robust},
        },
    });
    std::fs::write(out.join("moth-angles.json"), json.to_string()).unwrap();
}
