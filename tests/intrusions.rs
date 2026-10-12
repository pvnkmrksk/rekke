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
