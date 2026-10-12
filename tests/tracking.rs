use rekke::{
    config::{Config, Method, Point, PrimaryEdge, SourceConfig},
    frame::GrayFrame,
    source::DemoSource,
    tracker::Tracker,
};

fn wedge(config: &Config, angle: f32, width: f32, background: u8, wing: u8) -> GrayFrame {
    let (w, h, _) = config.source.dimensions();
    let s = &config.tracking.right;
    let mut pixels = vec![background; w as usize * h as usize];
    for y in 0..h {
        for x in 0..w {
            let p = Point {
                x: x as f32,
                y: y as f32,
            };
            let r = s.hinge.distance(p);
            let a = rekke::tracker::wrap_angle(s.hinge.angle_to(p) - angle);
            if r >= s.inner_radius - 5.0 && r <= s.outer_radius + 5.0 && a.abs() < width / 2.0 {
                pixels[y as usize * w as usize + x as usize] = wing;
            }
        }
    }
    GrayFrame::new(w, h, pixels, 42, 1).unwrap()
}
#[test]
fn known_wing_edges_and_relative_angles() {
    let mut c = Config::demo();
    c.tracking.right.start_deg = -65.0;
    c.tracking.right.span_deg = 130.0;
    c.tracking.right.primary_edge = PrimaryEdge::TowardHead;
    let tracker = Tracker::new(&c, 640, 480);
    for angle in [-0.5_f32, 0.0, 0.4] {
        let frame = wedge(&c, angle, 0.30, 230, 20);
        let m = tracker.process(&frame, None, 0).unwrap();
        assert_eq!(m.right.angles_rad.len(), 2, "angle {angle}, {:?}", m.right);
        for expected in [angle - 0.15, angle + 0.15] {
            assert!(
                m.right
                    .image_angles
                    .iter()
                    .any(|&actual| (actual - expected).abs() < 0.035),
                "Expected {expected}, got {:?}",
                m.right.image_angles
            );
        }
        for (&relative, &image) in m.right.angles_rad.iter().zip(&m.right.image_angles) {
            assert!((relative + image).abs() < 1e-5);
        }
        assert!(
            (m.right.image_angle_rad.unwrap() - (angle - 0.15)).abs() < 0.035,
            "Primary edge must stay toward the head"
        );
    }
}
#[test]
fn background_reference_removes_static_edge() {
    let mut c = Config::demo();
    c.tracking.subtract_background = true;
    let bg = wedge(&c, -0.3, 0.35, 215, 80);
    let mut raw = bg.clone();
    let moving = wedge(&c, 0.5, 0.2, 215, 25);
    for (p, m) in raw.pixels.iter_mut().zip(&moving.pixels) {
        if *m == 25 {
            *p = 25;
        }
    }
    let tracker = Tracker::new(&c, 640, 480);
    let result = tracker.process(&raw, Some(&bg), 0).unwrap();
    assert!(result.background_subtracted);
    assert_eq!(result.right.image_angles.len(), 2);
    assert!(
        result.right.image_angles.iter().all(|a| *a > 0.3),
        "{:?}",
        result.right.image_angles
    );
    let empty = tracker.process(&bg, Some(&bg), 0).unwrap();
    assert!(empty.right.angle_rad.is_none());
    assert!(empty.left_minus_right_rad.is_none());
}
#[test]
fn subtraction_requires_matching_reference() {
    let mut c = Config::demo();
    c.tracking.subtract_background = true;
    let tracker = Tracker::new(&c, 640, 480);
    let raw = wedge(&c, 0.0, 0.2, 220, 40);
    assert!(tracker.process(&raw, None, 0).is_err());
    let wrong = GrayFrame::new(32, 32, vec![0; 1024], 0, 0).unwrap();
    assert!(tracker.process(&raw, Some(&wrong), 0).is_err());
}
#[test]
fn blank_and_disabled_regions_report_no_angle() {
    let mut c = Config::demo();
    c.tracking.left.enabled = false;
    let frame = GrayFrame::new(640, 480, vec![220; 640 * 480], 0, 0).unwrap();
    let result = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(!result.left.enabled);
    assert!(result.right.angle_rad.is_none());
    assert!(result.head.angle_rad.is_none());
    assert!(result.left_minus_right_rad.is_none());
    assert!(
        serde_json::to_string(&result)
            .unwrap()
            .contains("\"angle_rad\":null")
    );
}
#[test]
fn centroid_and_wraparound_sector_track_known_direction() {
    let mut c = Config::demo();
    c.tracking.right.method = Method::Centroid;
    c.tracking.right.start_deg = 300.0;
    c.tracking.right.span_deg = 120.0;
    let frame = wedge(&c, 0.12, 0.32, 230, 20);
    let result = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!((result.right.image_angle_rad.unwrap() - 0.12).abs() < 0.025);
    assert!(result.right.radius_px.unwrap() > c.tracking.right.inner_radius);
}
#[test]
fn clipping_sector_does_not_detect_image_boundary() {
    let mut c = Config::demo();
    c.tracking.right.hinge = Point { x: 630.0, y: 240.0 };
    let frame = GrayFrame::new(640, 480, vec![120; 640 * 480], 0, 0).unwrap();
    let result = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(result.right.coverage < 0.8);
    assert!(result.right.angle_rad.is_none());
}
#[test]
fn two_wings_and_body_bonus_work_on_simulated_insect() {
    let mut c = Config::demo();
    c.tracking.head.enabled = true;
    c.tracking.abdomen.enabled = true;
    let frame = DemoSource::new(640, 480, 120, 8.0).render(0.025, 0);
    let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(m.left.angle_rad.is_some());
    assert!(m.right.angle_rad.is_some());
    assert!(m.head.angle_rad.is_some());
    assert!(m.abdomen.angle_rad.is_some());
    assert!(m.left_minus_right_rad.is_some());
}
#[test]
fn config_roundtrip_and_invalid_geometry_are_checked() {
    let c = Config::demo();
    let roundtrip: Config = toml::from_str(&toml::to_string_pretty(&c).unwrap()).unwrap();
    assert_eq!(c, roundtrip);
    for change in [0, 1, 2, 3] {
        let mut bad = c.clone();
        match change {
            0 => bad.tracking.left.outer_radius = bad.tracking.left.inner_radius,
            1 => bad.tracking.angle_bins = 0,
            2 => bad.tail = bad.head,
            _ => bad.tracking.right.hinge.x = f32::NAN,
        }
        assert!(bad.validate().is_err());
    }
}

#[test]
fn defaults_open_first_camera_and_accept_any_measurable_edge() {
    let c = Config::default();
    assert!(matches!(c.source, SourceConfig::Camera { ref device, .. } if device == "auto"));
    assert_eq!(c.tracking.right.threshold, 0.0);
    assert_eq!(c.tracking.right.primary_edge, PrimaryEdge::Strongest);
    c.validate().unwrap();
}
#[test]
fn one_level_faint_signal_is_detected_and_live_threshold_can_reject_it() {
    let mut c = Config::demo();
    let frame = wedge(&c, 0.5, 0.3, 230, 229);
    let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(m.right.angle_rad.is_some());
    assert!(m.right.strength > 0.0 && m.right.strength < 0.035);
    assert_eq!(
        m.right.image_angle_rad,
        m.right.image_angles.first().copied()
    );
    assert!(
        m.right
            .gradients
            .windows(2)
            .all(|g| g[0].abs() >= g[1].abs())
    );
    c.tracking.right.threshold = m.right.peak_gradient + 0.0001;
    let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(m.right.angle_rad.is_none());
    assert!(m.right.peak_gradient > 0.0);
}
#[test]
fn uniform_signals_never_create_edges_at_zero_threshold() {
    for method in [Method::Edge, Method::WingBlur, Method::Centroid] {
        let mut c = Config::demo();
        c.tracking.right.method = method;
        for level in [0, 1, 120, 220, 255] {
            let frame = GrayFrame::new(640, 480, vec![level; 640 * 480], 0, 0).unwrap();
            let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
            assert!(m.right.angle_rad.is_none(), "{method:?}, level {level}");
        }
    }
}
#[test]
fn strongest_edge_at_sector_boundary_is_not_lost() {
    let mut c = Config::demo();
    c.tracking.right.start_deg = 0.0;
    c.tracking.right.span_deg = 60.0;
    let frame = wedge(&c, 0.0, 0.012, 230, 150);
    let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(m.right.image_angle_rad.is_some());
    assert!(m.right.image_angle_rad.unwrap().abs() < 0.05);
}
#[test]
fn radial_consensus_rejects_a_strong_short_obstruction_over_a_faint_wing() {
    let mut c = Config::demo();
    let mut frame = wedge(&c, 0.7, 0.25, 230, 226);
    let s = &c.tracking.right;
    for y in 0..480 {
        for x in 0..640 {
            let p = Point {
                x: x as f32,
                y: y as f32,
            };
            let r = s.hinge.distance(p);
            let a = s.hinge.angle_to(p);
            if (80.0..103.0).contains(&r) && (a - 0.0).abs() < 0.06 {
                frame.pixels[y * 640 + x] = 10;
            }
        }
    }
    let raw = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(raw.right.image_angle_rad.unwrap().abs() < 0.12);
    c.tracking.right.method = Method::WingBlur;
    let filtered = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(
        (0.5..0.9).contains(&filtered.right.image_angle_rad.unwrap()),
        "{:?}",
        filtered.right.image_angles
    );
}
#[test]
fn blurred_faint_wing_tracks_a_gradient_around_its_known_angle() {
    let mut c = Config::demo();
    let mut pixels = vec![230; 640 * 480];
    let s = &c.tracking.right;
    for y in 0..480 {
        for x in 0..640 {
            let p = Point {
                x: x as f32,
                y: y as f32,
            };
            let r = s.hinge.distance(p);
            if r >= s.inner_radius - 5.0 && r <= s.outer_radius + 5.0 {
                let delta = rekke::tracker::wrap_angle(s.hinge.angle_to(p) - 0.55);
                let blur = 8.0 * (-0.5 * (delta / 0.12).powi(2)).exp();
                pixels[y * 640 + x] -= blur.round() as u8;
            }
        }
    }
    let mut frame = GrayFrame::new(640, 480, pixels, 0, 0).unwrap();
    let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    let a = m.right.image_angle_rad.unwrap();
    assert!(
        ((a - 0.55).abs() - 0.12).abs() < 0.05,
        "blur gradient angle {a}"
    );
    assert!(m.right.strength > 0.0 && m.right.strength < 0.035);
    c.tracking.right.method = Method::WingBlur;
    // A narrow dark-on-light obstruction becomes a bright pulse in foreground.
    let hinge = c.tracking.right.hinge;
    for y in 0..480 {
        for x in 0..640 {
            let p = Point {
                x: x as f32,
                y: y as f32,
            };
            if hinge.distance(p) >= 40.0
                && hinge.distance(p) <= 195.0
                && (hinge.angle_to(p) - 0.0).abs() < 0.012
            {
                frame.pixels[y * 640 + x] = 0;
            }
        }
    }
    let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    let a = m.right.image_angle_rad.unwrap();
    assert!(
        ((a - 0.55).abs() - 0.12).abs() < 0.06,
        "blur with bright pulse {a}"
    );
}
#[test]
fn radial_consensus_keeps_faint_signal_when_other_angles_are_outside_the_image() {
    let mut c = Config::demo();
    c.tracking.right.method = Method::WingBlur;
    c.tracking.right.hinge = Point { x: 610.0, y: 240.0 };
    c.tracking.right.start_deg = 0.0;
    c.tracking.right.span_deg = 145.0;
    let frame = wedge(&c, 105.0_f32.to_radians(), 0.25, 210, 206);
    let m = Tracker::new(&c, 640, 480).process(&frame, None, 0).unwrap();
    assert!(m.right.coverage < 0.9);
    let angle = m.right.image_angle_rad.unwrap().to_degrees();
    assert!((95.0..115.0).contains(&angle), "clipped blur angle {angle}");
}

fn bright_pulse(frame: &mut GrayFrame, c: &Config, center: f32, width: f32, lo: f32, hi: f32) {
    let hinge = c.tracking.right.hinge;
    for y in 0..frame.height {
        for x in 0..frame.width {
            let p = Point {
                x: x as f32,
                y: y as f32,
            };
            let r = hinge.distance(p);
            if r >= lo
                && r <= hi
                && rekke::tracker::wrap_angle(hinge.angle_to(p) - center).abs() < width * 0.5
            {
                frame.pixels[y as usize * frame.width as usize + x as usize] = 255;
            }
        }
    }
}
#[test]
fn broad_faint_shoulder_survives_bright_thin_pulses_inside_outside_and_near_edge() {
    let mut c = Config::demo();
    c.tracking.dark_on_light = false;
    c.tracking.right.method = Method::WingBlur;
    c.tracking.right.spike_width_deg = 5.0;
    c.tracking.right.primary_edge = PrimaryEdge::TowardHead;
    for level in [1, 4, 12] {
        for pulse in [-0.3, 0.2, 0.49, 0.8] {
            let mut raw = wedge(&c, 0.2, 0.6, 0, level);
            bright_pulse(&mut raw, &c, pulse, 0.018, 40.0, 195.0);
            let m = Tracker::new(&c, 640, 480).process(&raw, None, 0).unwrap();
            let a = m
                .right
                .image_angle_rad
                .expect("faint shoulder should remain");
            assert!(
                (a + 0.1).abs() < 0.05,
                "contrast {level}, pulse {pulse}, wing edge {a}"
            );
            assert!(m.right.angles_rad.len() <= 2);
        }
    }
}
#[test]
fn only_narrow_bright_intrusions_do_not_count_as_a_wing_in_blur_mode() {
    let mut c = Config::demo();
    c.tracking.dark_on_light = false;
    c.tracking.right.method = Method::WingBlur;
    let mut raw = wedge(&c, 0.2, 0.6, 0, 0);
    for angle in [-0.25, 0.35, 0.9] {
        bright_pulse(&mut raw, &c, angle, 0.018, 40.0, 195.0);
    }
    let m = Tracker::new(&c, 640, 480).process(&raw, None, 0).unwrap();
    assert!(
        m.right.angle_rad.is_none(),
        "spikes produced {:?}",
        m.right.angles_rad
    );
    c.tracking.right.method = Method::Edge;
    assert!(
        Tracker::new(&c, 640, 480)
            .process(&raw, None, 0)
            .unwrap()
            .right
            .angle_rad
            .is_some()
    );
}
#[test]
fn broad_shoulder_filter_works_after_saved_background_subtraction_and_live_tuning() {
    let mut c = Config::demo();
    c.tracking.subtract_background = true;
    c.tracking.right.method = Method::WingBlur;
    let bg = wedge(&c, 0.2, 0.6, 80, 80);
    let mut raw = wedge(&c, 0.2, 0.6, 80, 84);
    bright_pulse(&mut raw, &c, 0.85, 0.025, 40.0, 195.0);
    let mut tracker = Tracker::new(&c, 640, 480);
    let m = tracker.process(&raw, Some(&bg), 0).unwrap();
    assert!((m.right.image_angle_rad.unwrap().abs() - 0.1).abs() < 0.05);
    c.tracking.right.threshold = 1.0;
    tracker.update(&c);
    assert!(
        tracker
            .process(&raw, Some(&bg), 0)
            .unwrap()
            .right
            .angle_rad
            .is_none()
    );
    c.tracking.right.threshold = 0.0;
    c.tracking.right.spike_width_deg = 0.0;
    tracker.update(&c);
    let m = tracker.process(&raw, Some(&bg), 0).unwrap();
    assert!((m.right.image_angle_rad.unwrap() - 0.85).abs() < 0.06);
}
#[test]
fn asymmetric_hinge_angle_zero_uses_kinefly_body_axis_projection() {
    let mut c = Config::demo();
    c.tracking.dark_on_light = false;
    c.tracking.right.hinge.y = 275.0;
    let raw = wedge(&c, 0.2, 0.4, 0, 20);
    let m = Tracker::new(&c, 640, 480).process(&raw, None, 0).unwrap();
    assert!((m.right.angle_rad.unwrap() + m.right.image_angle_rad.unwrap()).abs() < 1e-5);
}
