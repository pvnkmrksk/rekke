use rekke::{
    config::{Config, Method, Point},
    geometry::{self, Handle, move_handle},
    tracker::wrap_angle,
};
fn point(actual: Point, x: f32, y: f32) {
    assert!(
        (actual.x - x).abs() < 0.002 && (actual.y - y).abs() < 0.002,
        "{actual:?} != ({x}, {y})"
    );
}
fn angle(actual: f32, expected: f32) {
    assert!(
        wrap_angle((actual - expected).to_radians()).abs() < 0.0001,
        "{actual} != {expected}"
    );
}
fn symmetric(c: &Config) {
    let l = &c.tracking.left;
    let r = &c.tracking.right;
    let reflected = geometry::reflect(l.hinge, c.head, c.tail);
    point(r.hinge, reflected.x, reflected.y);
    let axis = c.tail.angle_to(c.head).to_degrees();
    angle(r.start_deg, 2.0 * axis - l.start_deg - l.span_deg);
    assert!((l.span_deg - r.span_deg).abs() < 0.001);
    assert!((l.inner_radius - r.inner_radius).abs() < 0.001);
    assert!((l.outer_radius - r.outer_radius).abs() < 0.001);
}
#[test]
fn moving_either_wing_hinge_mirrors_the_opposite_wing() {
    let mut c = Config::default();
    assert!(!c.tracking.asymmetric);
    move_handle(&mut c, Handle::Hinge(0), Point { x: 280.0, y: 220.0 }).unwrap();
    point(c.tracking.left.hinge, 280.0, 220.0);
    point(c.tracking.right.hinge, 360.0, 220.0);
    move_handle(&mut c, Handle::Hinge(1), Point { x: 367.0, y: 251.0 }).unwrap();
    point(c.tracking.left.hinge, 273.0, 251.0);
    symmetric(&c);
}
#[test]
fn corner_drags_change_radius_and_angle_together_and_mirror_both_boundaries() {
    for (i, handle, degrees, span) in [
        (0, Handle::Start(0), 105.0_f32, 100.0),
        (1, Handle::End(1), 75.0, 100.0),
    ] {
        let mut c = Config::default();
        let p = c.sectors()[i].hinge.at(150.0, degrees.to_radians());
        move_handle(&mut c, handle, p).unwrap();
        assert!((c.sectors()[i].outer_radius - 150.0).abs() < 0.001);
        assert!((c.sectors()[i].span_deg - span).abs() < 0.001);
        symmetric(&c);
    }
}
#[test]
fn inner_and_outer_radius_drags_mirror_even_a_disabled_wing() {
    let mut c = Config::default();
    c.tracking.right.enabled = false;
    c.tracking.right.method = Method::WingBlur;
    c.tracking.right.threshold = 0.03;
    for (handle, radius) in [(Handle::Inner(0), 75.0), (Handle::Outer(0), 160.0)] {
        let p = c.tracking.left.hinge.at(radius, 160.0_f32.to_radians());
        move_handle(&mut c, handle, p).unwrap();
        symmetric(&c);
    }
    assert!(!c.tracking.right.enabled);
    assert_eq!(c.tracking.right.method, Method::WingBlur);
    assert_eq!(c.tracking.right.threshold, 0.03);
}
#[test]
fn body_center_translates_all_anchors_and_regions_without_changing_shape() {
    let mut c = Config::default();
    let old = c.clone();
    let center = geometry::center(&c);
    move_handle(
        &mut c,
        Handle::Body,
        Point {
            x: center.x + 50.0,
            y: center.y + 30.0,
        },
    )
    .unwrap();
    point(c.head, 370.0, 205.0);
    point(c.tail, 370.0, 360.0);
    for (s, old) in c.sectors().into_iter().zip(old.sectors()) {
        point(s.hinge, old.hinge.x + 50.0, old.hinge.y + 30.0);
        assert_eq!(s.start_deg, old.start_deg);
        assert_eq!(s.span_deg, old.span_deg);
        assert_eq!(s.inner_radius, old.inner_radius);
        assert_eq!(s.outer_radius, old.outer_radius);
    }
    symmetric(&c);
}
#[test]
fn head_rotation_reorients_every_region_about_the_updated_midpoint() {
    let mut c = Config::default();
    let old = c.clone();
    move_handle(&mut c, Handle::Head, Point { x: 475.0, y: 330.0 }).unwrap();
    point(c.head, 475.0, 330.0);
    point(c.tail, 320.0, 330.0); // Other calibration endpoint stays fixed.
    point(geometry::center(&c), 397.5, 330.0);
    point(c.tracking.left.hinge, 410.0, 306.0);
    point(c.tracking.right.hinge, 410.0, 354.0);
    point(c.tracking.head.hinge, 445.0, 330.0);
    point(c.tracking.abdomen.hinge, 385.0, 330.0);
    for (s, old) in c.sectors().into_iter().zip(old.sectors()) {
        angle(s.start_deg, old.start_deg + 90.0);
        assert_eq!(s.span_deg, old.span_deg);
        assert_eq!(s.inner_radius, old.inner_radius);
        assert_eq!(s.outer_radius, old.outer_radius);
    }
    symmetric(&c);
}
#[test]
fn tail_rotation_moves_the_whole_frame_in_the_other_direction() {
    let mut c = Config::default();
    move_handle(&mut c, Handle::Tail, Point { x: 475.0, y: 175.0 }).unwrap();
    point(c.head, 320.0, 175.0);
    point(c.tail, 475.0, 175.0);
    point(c.tracking.left.hinge, 385.0, 199.0);
    point(c.tracking.right.hinge, 385.0, 151.0);
    point(c.tracking.head.hinge, 350.0, 175.0);
    point(c.tracking.abdomen.hinge, 410.0, 175.0);
    angle(c.tracking.left.start_deg, 25.0);
    symmetric(&c);
}
#[test]
fn wing_mirroring_uses_the_rotated_body_axis() {
    let mut c = Config::default();
    move_handle(&mut c, Handle::Head, Point { x: 475.0, y: 330.0 }).unwrap();
    move_handle(&mut c, Handle::Hinge(0), Point { x: 430.0, y: 280.0 }).unwrap();
    // The body now runs horizontally at y=330, so reflect in Y rather than X.
    point(c.tracking.right.hinge, 430.0, 380.0);
    let p = c.tracking.left.hinge.at(125.0, -145.0_f32.to_radians());
    move_handle(&mut c, Handle::Start(0), p).unwrap();
    symmetric(&c);
    assert!((c.tracking.right.outer_radius - 125.0).abs() < 0.001);
}
#[test]
fn asymmetric_mode_keeps_other_regions_in_place() {
    let mut c = Config::default();
    geometry::set_asymmetric(&mut c, true, 0).unwrap();
    let right = c.tracking.right.clone();
    move_handle(&mut c, Handle::Hinge(0), Point { x: 285.0, y: 235.0 }).unwrap();
    assert_eq!(c.tracking.right, right);
    let p = c.tracking.left.hinge.at(140.0, 105.0_f32.to_radians());
    move_handle(&mut c, Handle::Start(0), p).unwrap();
    assert_eq!(c.tracking.right, right);
    let regions = c.sectors().map(Clone::clone);
    move_handle(&mut c, Handle::Head, Point { x: 360.0, y: 180.0 }).unwrap();
    assert_eq!(c.sectors(), regions.each_ref());
    let center = geometry::center(&c);
    move_handle(
        &mut c,
        Handle::Body,
        Point {
            x: center.x + 5.0,
            y: center.y + 10.0,
        },
    )
    .unwrap();
    assert_eq!(c.sectors(), regions.each_ref());
    point(c.head, 365.0, 190.0);
}
#[test]
fn turning_asymmetry_off_restores_geometry_from_the_last_edited_wing() {
    let mut c = Config::default();
    geometry::set_asymmetric(&mut c, true, 0).unwrap();
    move_handle(&mut c, Handle::Hinge(1), Point { x: 370.0, y: 245.0 }).unwrap();
    let p = c.tracking.right.hinge.at(130.0, 75.0_f32.to_radians());
    move_handle(&mut c, Handle::End(1), p).unwrap();
    let right = c.tracking.right.clone();
    geometry::set_asymmetric(&mut c, false, 1).unwrap();
    assert_eq!(c.tracking.right, right);
    point(c.tracking.left.hinge, 270.0, 245.0);
    symmetric(&c);
}
#[test]
fn numeric_geometry_edits_follow_the_same_symmetry_rules() {
    let mut c = Config::default();
    let mut right = c.tracking.right.clone();
    right.hinge.x = 355.0;
    right.span_deg = 115.0;
    right.outer_radius = 170.0;
    assert!(geometry::edit_sector(&mut c, 1, right).unwrap());
    symmetric(&c);
    let mut right = c.tracking.right.clone();
    let left = c.tracking.left.clone();
    right.threshold = 0.02;
    assert!(!geometry::edit_sector(&mut c, 1, right).unwrap());
    assert_eq!(c.tracking.left, left); // Detector settings are independent.
}
#[test]
fn invalid_coupled_moves_are_atomic_and_translation_stops_at_the_image_border() {
    let mut c = Config::default();
    let old = c.clone();
    let tail = c.tail;
    assert!(move_handle(&mut c, Handle::Head, tail).is_err());
    assert_eq!(c, old);
    assert!(move_handle(&mut c, Handle::Hinge(0), Point { x: 320.0, y: 240.0 }).is_err());
    assert_eq!(c, old);
    move_handle(&mut c, Handle::Body, Point { x: 320.0, y: 479.0 }).unwrap();
    point(c.tail, 320.0, 479.0);
    // All six stored anchors moved by the same limited displacement of 149px.
    point(c.head, 320.0, 324.0);
    point(c.tracking.left.hinge, 296.0, 389.0);
    point(c.tracking.abdomen.hinge, 320.0, 414.0);
    symmetric(&c);
    c.validate().unwrap();
}
#[test]
fn asymmetric_mode_persists_and_older_configs_default_to_symmetric() {
    let mut c = Config::default();
    let old = toml::to_string_pretty(&c)
        .unwrap()
        .replace("asymmetric = false\n", "");
    assert!(!toml::from_str::<Config>(&old).unwrap().tracking.asymmetric);
    c.tracking.asymmetric = true;
    let saved = toml::to_string_pretty(&c).unwrap();
    assert!(
        toml::from_str::<Config>(&saved)
            .unwrap()
            .tracking
            .asymmetric
    );
}
