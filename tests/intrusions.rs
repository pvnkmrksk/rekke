//! Detector accuracy on synthetic flies with known leading-edge angles,
//! including antennae and legs that intrude into the wing sectors.
use rekke::{
    config::{Config, Method, PrimaryEdge},
    synth::{Scene, SceneOptions},
    tracker::Tracker,
};

struct Score {
    frames: usize,
    missing: usize,
    /// Frames with more than 4° error.
    wrong: usize,
    median_deg: f32,
    worst_deg: f32,
}
fn configure(scene: &Scene, method: Method, wide: bool) -> Config {
    let mut c = Config::demo();
    scene.configure(&mut c);
    for (i, s) in [&mut c.tracking.left, &mut c.tracking.right]
        .into_iter()
        .enumerate()
    {
        // −45° to +85° (or +100° to include the antennae) around each wing.
        let reach = if wide { 100.0 } else { 85.0 };
        s.start_deg = if i == 0 { 135.0 } else { -reach };
        s.span_deg = 45.0 + reach;
        s.inner_radius = 55.0;
        s.outer_radius = 185.0;
        s.method = method;
        s.threshold = 0.0;
        s.max_edges = 2;
        s.primary_edge = PrimaryEdge::Strongest;
    }
    c.tracking.head.enabled = false;
    c.tracking.abdomen.enabled = false;
    c
}
fn score(options: SceneOptions, method: Method, wide: bool, subtract: bool) -> Score {
    score_at(options, method, wide, subtract, 640, 480)
}
fn score_at(
    options: SceneOptions,
    method: Method,
    wide: bool,
    subtract: bool,
    width: u32,
    height: u32,
) -> Score {
    let scene = Scene::new(width, height, options);
    let mut config = configure(&scene, method, wide);
    let scale = width as f32 / 640.0;
    for s in [&mut config.tracking.left, &mut config.tracking.right] {
        s.inner_radius *= scale;
        s.outer_radius *= scale;
    }
    if let rekke::config::SourceConfig::Demo {
        width: w,
        height: h,
        ..
    } = &mut config.source
    {
        *w = width;
        *h = height;
    }
    config.tracking.subtract_background = subtract;
    let tracker = Tracker::new(&config, width, height);
    let background = scene.empty();
    let mut errors = Vec::new();
    let mut missing = 0;
    for k in 0..360 {
        let t = k as f32 / 30.0;
        let frame = scene.render(t, k);
        let truth = scene.truth(t);
        let m = tracker.process(&frame, Some(&background), 0).unwrap();
        for (wing, part) in [&m.left, &m.right].into_iter().enumerate() {
            match part.angle_rad {
                Some(a) => errors.push((a - truth.front[wing]).abs().to_degrees()),
                None => missing += 1,
            }
        }
    }
    let wrong = errors.iter().filter(|e| **e > 4.0).count() + missing;
    errors.sort_by(f32::total_cmp);
    Score {
        frames: 720,
        missing,
        wrong,
        median_deg: errors.get(errors.len() / 2).copied().unwrap_or(f32::NAN),
        worst_deg: errors.last().copied().unwrap_or(f32::NAN),
    }
}
fn scenarios() -> Vec<(&'static str, SceneOptions, bool)> {
    let base = SceneOptions::default();
    vec![
        (
            "clean flight",
            SceneOptions {
                antennae: false,
                legs: false,
                leg_kicks: false,
                ..base.clone()
            },
            false,
        ),
        ("legs + kicks", base.clone(), false),
        (
            "antennae in sector",
            SceneOptions {
                leg_kicks: false,
                ..base.clone()
            },
            true,
        ),
        (
            "front legs reaching",
            SceneOptions {
                reaching_legs: true,
                ..base.clone()
            },
            true,
        ),
        (
            "faint wings + legs",
            SceneOptions {
                wing_contrast: 0.06,
                ..base.clone()
            },
            true,
        ),
        (
            "noisy faint + reaching",
            SceneOptions {
                wing_contrast: 0.07,
                noise: 3.0,
                reaching_legs: true,
                ..base.clone()
            },
            true,
        ),
        (
            "thick legs ×2.5",
            SceneOptions {
                leg_width: 2.5,
                reaching_legs: true,
                ..base.clone()
            },
            true,
        ),
        (
            "thick faint ×2.5",
            SceneOptions {
                leg_width: 2.5,
                wing_contrast: 0.06,
                ..base.clone()
            },
            true,
        ),
        (
            "wide stroke, big swing",
            SceneOptions {
                front_deg: 62.0,
                modulation_deg: 20.0,
                reaching_legs: true,
                ..base
            },
            true,
        ),
    ]
}
#[test]
#[ignore = "prints a comparison table; run with --ignored --nocapture"]
fn compare_detectors_on_intrusion_scenarios() {
    for (name, options, wide) in scenarios() {
        for subtract in [false, true] {
            for method in [Method::Edge, Method::WingBlur] {
                let s = score(options.clone(), method, wide, subtract);
                println!(
                    "{name:24} {:9} {:8} wrong {:4}/{} missing {:3} median {:5.2}° worst {:6.1}°",
                    format!("{method:?}"),
                    if subtract { "subtract" } else { "raw" },
                    s.wrong,
                    s.frames,
                    s.missing,
                    s.median_deg,
                    s.worst_deg
                );
            }
        }
    }
}
#[test]
#[ignore = "prints a comparison table; run with --ignored --nocapture"]
fn compare_detectors_across_resolutions() {
    for (width, height) in [(320, 240), (1280, 960)] {
        for (name, options, wide) in scenarios() {
            for method in [Method::Edge, Method::WingBlur] {
                let s = score_at(options.clone(), method, wide, false, width, height);
                println!(
                    "{width}×{height} {name:24} {:9} wrong {:4}/{} median {:5.2}° worst {:6.1}°",
                    format!("{method:?}"),
                    s.wrong,
                    s.frames,
                    s.median_deg,
                    s.worst_deg
                );
            }
        }
    }
}
#[test]
#[ignore = "prints a comparison table; run with --ignored --nocapture"]
fn find_the_limits_of_leg_rejection() {
    for leg_width in [3.5, 5.0, 7.0] {
        for reaching in [false, true] {
            let options = SceneOptions {
                leg_width,
                reaching_legs: reaching,
                ..SceneOptions::default()
            };
            let s = score(options, Method::WingBlur, true, false);
            println!(
                "legs ×{leg_width} reaching {reaching}: wrong {:4}/{} median {:5.2}° worst {:6.1}°",
                s.wrong, s.frames, s.median_deg, s.worst_deg
            );
        }
    }
}

/// Fast regression over every leg kick: the default (robust) detector must
/// never be pulled onto a leg, while the classic detector demonstrably is.
fn sweep(options: SceneOptions, method: Option<Method>, subtract: bool) -> (usize, f32) {
    let scene = Scene::new(640, 480, options);
    let mut config = Config::default();
    scene.configure(&mut config);
    config.output.enabled = false;
    config.tracking.subtract_background = subtract;
    if let Some(method) = method {
        config.tracking.left.method = method;
        config.tracking.right.method = method;
    }
    let tracker = Tracker::new(&config, 640, 480);
    let background = scene.empty();
    let (mut wrong, mut worst) = (0, 0.0f32);
    for k in 0..36 {
        let t = 0.5 + k as f32 * 0.2;
        let m = tracker
            .process(&scene.render(t, k), Some(&background), 0)
            .unwrap();
        let truth = scene.truth(t);
        for (wing, part) in [&m.left, &m.right].into_iter().enumerate() {
            let error = part.angle_rad.map_or(f32::INFINITY, |a| {
                (a - truth.front[wing]).abs().to_degrees()
            });
            worst = worst.max(error);
            wrong += usize::from(error > 4.0);
        }
    }
    (wrong, worst)
}
#[test]
fn default_detector_ignores_legs_and_antennae_that_fool_the_classic_one() {
    assert_eq!(Config::default().tracking.left.method, Method::WingBlur);
    let reaching = SceneOptions {
        reaching_legs: true,
        ..SceneOptions::default()
    };
    for subtract in [false, true] {
        let (wrong, worst) = sweep(reaching.clone(), None, subtract);
        assert_eq!(wrong, 0, "robust, subtract {subtract}: worst {worst}°");
        assert!(worst < 2.0, "robust accuracy {worst}°");
        let (wrong, _) = sweep(reaching.clone(), Some(Method::Edge), subtract);
        assert!(wrong > 30, "classic detector should be fooled: {wrong}");
    }
    for options in [
        SceneOptions::default(),
        SceneOptions {
            wing_contrast: 0.06,
            leg_width: 2.5,
            noise: 2.5,
            ..SceneOptions::default()
        },
    ] {
        let (wrong, worst) = sweep(options, None, false);
        assert_eq!(wrong, 0, "worst {worst}°");
    }
}
#[test]
fn robust_detector_matches_the_classic_one_on_a_clean_fly() {
    let clean = SceneOptions {
        antennae: false,
        legs: false,
        leg_kicks: false,
        ..SceneOptions::default()
    };
    for method in [Method::Edge, Method::WingBlur] {
        let (wrong, worst) = sweep(clean.clone(), Some(method), false);
        assert_eq!(wrong, 0, "{method:?} worst {worst}°");
        assert!(worst < 1.0, "{method:?} accuracy {worst}°");
    }
}

/// Regenerate the README figure:
/// REKKE_FIGURE=docs/images/legs-classic-vs-robust.png cargo test --release --test intrusions figure -- --ignored
#[test]
#[ignore = "writes the README figure when REKKE_FIGURE is set"]
fn figure_classic_versus_robust() {
    let Some(out) = std::env::var_os("REKKE_FIGURE") else {
        return;
    };
    let scene = Scene::new(
        640,
        480,
        SceneOptions {
            reaching_legs: true,
            ..SceneOptions::default()
        },
    );
    let t = 1.0;
    let frame = scene.render(t, 30);
    let truth = scene.truth(t);
    let (x0, y0, w, h) = (40u32, 30u32, 560u32, 420u32);
    let mut figure = image::RgbImage::from_pixel(w * 2 + 16, h, image::Rgb([255, 255, 255]));
    for (panel, method) in [Method::Edge, Method::WingBlur].into_iter().enumerate() {
        let mut config = Config::default();
        scene.configure(&mut config);
        config.tracking.left.method = method;
        config.tracking.right.method = method;
        let m = Tracker::new(&config, 640, 480)
            .process(&frame, None, 0)
            .unwrap();
        let ox = panel as u32 * (w + 16);
        for y in 0..h {
            for x in 0..w {
                let v = frame.pixels[((y + y0) * 640 + x + x0) as usize];
                figure.put_pixel(ox + x, y, image::Rgb([v, v, v]));
            }
        }
        let mut line = |a: rekke::config::Point,
                        b: rekke::config::Point,
                        color: [u8; 3],
                        width: f32,
                        dash: bool| {
            let length = a.distance(b);
            let steps = (length * 4.0) as usize;
            for k in 0..=steps {
                let f = k as f32 / steps as f32;
                if dash && ((f * length / 9.0) as usize).is_multiple_of(2) {
                    continue;
                }
                let (px, py) = (
                    a.x + (b.x - a.x) * f - x0 as f32,
                    a.y + (b.y - a.y) * f - y0 as f32,
                );
                let r = width * 0.5;
                for dy in -(r.ceil() as i32)..=(r.ceil() as i32) {
                    for dx in -(r.ceil() as i32)..=(r.ceil() as i32) {
                        let (x, y) = (px + dx as f32, py + dy as f32);
                        if (dx * dx + dy * dy) as f32 <= r * r + 0.5
                            && x >= 0.0
                            && y >= 0.0
                            && x < w as f32
                            && y < h as f32
                        {
                            figure.put_pixel(ox + x as u32, y as u32, image::Rgb(color));
                        }
                    }
                }
            }
        };
        let colors = [[247, 93, 99], [87, 216, 115]];
        for (wing, sector) in [&config.tracking.left, &config.tracking.right]
            .into_iter()
            .enumerate()
        {
            let hinge = sector.hinge;
            for a in [sector.start_deg, sector.start_deg + sector.span_deg] {
                let a = a.to_radians();
                line(
                    hinge.at(sector.inner_radius, a),
                    hinge.at(sector.outer_radius, a),
                    colors[wing],
                    1.0,
                    false,
                );
            }
            for k in 0..120 {
                let a0 = (sector.start_deg + sector.span_deg * k as f32 / 120.0).to_radians();
                let a1 = (sector.start_deg + sector.span_deg * (k + 1) as f32 / 120.0).to_radians();
                for r in [sector.inner_radius, sector.outer_radius] {
                    line(hinge.at(r, a0), hinge.at(r, a1), colors[wing], 1.0, false);
                }
            }
            // Ground truth (dashed white) and the detector's primary edge.
            let part = if wing == 0 { &m.left } else { &m.right };
            let sense = if wing == 0 { 1.0 } else { -1.0 };
            let origin = if wing == 0 { std::f32::consts::PI } else { 0.0 };
            let true_image = origin + sense * truth.front[wing];
            line(
                hinge.at(sector.inner_radius, true_image),
                hinge.at(sector.outer_radius + 12.0, true_image),
                [255, 255, 255],
                3.0,
                true,
            );
            if let Some(a) = part.image_angle_rad {
                line(
                    hinge.at(sector.inner_radius, a),
                    hinge.at(sector.outer_radius + 12.0, a),
                    [255, 160, 40],
                    3.0,
                    false,
                );
            }
        }
    }
    figure.save(out).unwrap();
}
