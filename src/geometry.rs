//! Camera-coordinate geometry edits. Coupling affects calibration, not detection.
use crate::config::{Config, Point, Sector};
use anyhow::{Context, Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    Head,
    Tail,
    Body,
    Hinge(usize),
    Start(usize),
    End(usize),
    Inner(usize),
    Outer(usize),
}
impl Handle {
    pub fn wing(self) -> Option<usize> {
        match self {
            Self::Hinge(i) | Self::Start(i) | Self::End(i) | Self::Inner(i) | Self::Outer(i)
                if i < 2 =>
            {
                Some(i)
            }
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Head => "Head anchor · drag to orient the body frame",
            Self::Tail => "Tail anchor · drag to orient the body frame",
            Self::Body => "Body center · drag to move the setup",
            Self::Hinge(_) => "Hinge · drag to move",
            Self::Start(_) | Self::End(_) => "Sector corner · drag angle and radius together",
            Self::Inner(_) => "Inner radius · drag to resize",
            Self::Outer(_) => "Outer radius · drag to resize",
        }
    }
}
pub fn center(config: &Config) -> Point {
    Point {
        x: (config.head.x + config.tail.x) * 0.5,
        y: (config.head.y + config.tail.y) * 0.5,
    }
}
fn wrap_degrees(a: f32) -> f32 {
    (a + 180.0).rem_euclid(360.0) - 180.0
}

/// Reflect a point in the line through the head and tail (not the screen's X axis).
pub fn reflect(point: Point, head: Point, tail: Point) -> Point {
    let dx = head.x - tail.x;
    let dy = head.y - tail.y;
    let projection = ((point.x - tail.x) * dx + (point.y - tail.y) * dy) / (dx * dx + dy * dy);
    Point {
        x: 2.0 * (tail.x + projection * dx) - point.x,
        y: 2.0 * (tail.y + projection * dy) - point.y,
    }
}
/// Copy only geometry. Enable flags, algorithms and thresholds remain independent.
pub fn mirror_wing(config: &mut Config, source: usize) {
    let source = source.min(1);
    let s = config.sectors()[source].clone();
    let reflected = reflect(s.hinge, config.head, config.tail);
    let start = wrap_degrees(
        2.0 * config.tail.angle_to(config.head).to_degrees() - s.start_deg - s.span_deg,
    );
    let target = config.sector_mut(1 - source);
    target.hinge = reflected;
    target.start_deg = start;
    target.span_deg = s.span_deg;
    target.inner_radius = s.inner_radius;
    target.outer_radius = s.outer_radius;
}
pub fn set_asymmetric(config: &mut Config, asymmetric: bool, source_wing: usize) -> Result<()> {
    let mut next = config.clone();
    next.tracking.asymmetric = asymmetric;
    if !asymmetric {
        mirror_wing(&mut next, source_wing);
    }
    next.validate()
        .context("Cannot restore symmetry at this body position")?;
    *config = next;
    Ok(())
}
fn same_geometry(a: &Sector, b: &Sector) -> bool {
    a.hinge == b.hinge
        && a.start_deg == b.start_deg
        && a.span_deg == b.span_deg
        && a.inner_radius == b.inner_radius
        && a.outer_radius == b.outer_radius
}
/// Apply numeric edits through the same coupling rules as dragging handles.
pub fn edit_sector(config: &mut Config, index: usize, sector: Sector) -> Result<bool> {
    let geometry_changed = !same_geometry(config.sectors()[index], &sector);
    let mut next = config.clone();
    *next.sector_mut(index) = sector;
    if geometry_changed && index < 2 && !next.tracking.asymmetric {
        mirror_wing(&mut next, index);
    }
    next.validate().context("Invalid region edit")?;
    *config = next;
    Ok(geometry_changed)
}
fn move_body_anchor(config: &mut Config, handle: Handle, p: Point) {
    let old_center = center(config);
    let old_angle = config.tail.angle_to(config.head);
    match handle {
        Handle::Head => config.head = p,
        Handle::Tail => config.tail = p,
        _ => unreachable!(),
    }
    if config.tracking.asymmetric {
        return;
    }
    let new_center = center(config);
    let rotation = config.tail.angle_to(config.head) - old_angle;
    let (sin, cos) = rotation.sin_cos();
    for i in 0..4 {
        let s = config.sector_mut(i);
        let x = s.hinge.x - old_center.x;
        let y = s.hinge.y - old_center.y;
        s.hinge = Point {
            x: new_center.x + cos * x - sin * y,
            y: new_center.y + sin * x + cos * y,
        };
        s.start_deg = wrap_degrees(s.start_deg + rotation.to_degrees());
    }
}
fn translate_body(config: &mut Config, p: Point) {
    let c = center(config);
    let (w, h, _) = config.source.dimensions();
    let mut points = vec![config.head, config.tail];
    if !config.tracking.asymmetric {
        points.extend(config.sectors().iter().map(|s| s.hinge));
    }
    // Limit the shared translation, never clamp individual points out of symmetry.
    let min_x = points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min);
    let max_x = points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max);
    let min_y = points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min);
    let max_y = points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max);
    let dx = (p.x - c.x).clamp(-min_x, w as f32 - 1.0 - max_x);
    let dy = (p.y - c.y).clamp(-min_y, h as f32 - 1.0 - max_y);
    config.head.x += dx;
    config.head.y += dy;
    config.tail.x += dx;
    config.tail.y += dy;
    if !config.tracking.asymmetric {
        for i in 0..4 {
            let s = config.sector_mut(i);
            s.hinge.x += dx;
            s.hinge.y += dy;
        }
    }
}
/// Commit a valid edit atomically; an invalid reflected/transformed anchor cannot
/// leave the preview geometry different from what the processing worker uses.
pub fn move_handle(config: &mut Config, handle: Handle, p: Point) -> Result<()> {
    ensure!(
        p.x.is_finite() && p.y.is_finite(),
        "Invalid pointer position"
    );
    let mut next = config.clone();
    let (w, h, _) = config.source.dimensions();
    let p = Point {
        x: p.x.clamp(0.0, w as f32 - 1.0),
        y: p.y.clamp(0.0, h as f32 - 1.0),
    };
    match handle {
        Handle::Head | Handle::Tail => move_body_anchor(&mut next, handle, p),
        Handle::Body => translate_body(&mut next, p),
        Handle::Hinge(i) => next.sector_mut(i).hinge = p,
        Handle::Inner(i) => {
            let s = next.sector_mut(i);
            s.inner_radius = s.hinge.distance(p).clamp(0.0, s.outer_radius - 2.0);
        }
        Handle::Outer(i) => {
            let s = next.sector_mut(i);
            s.outer_radius = s
                .hinge
                .distance(p)
                .clamp(s.inner_radius + 2.0, (w as f32).hypot(h as f32));
        }
        Handle::Start(i) | Handle::End(i) => {
            let s = next.sector_mut(i);
            let a = s.hinge.angle_to(p).to_degrees();
            let end = s.start_deg + s.span_deg;
            if matches!(handle, Handle::Start(_)) {
                let unwrapped = a + 360.0 * ((s.start_deg - a) / 360.0).round();
                s.start_deg = unwrapped.clamp(end - 355.0, end - 5.0);
                s.span_deg = end - s.start_deg;
            } else {
                let unwrapped = a + 360.0 * ((end - a) / 360.0).round();
                s.span_deg = (unwrapped - s.start_deg).clamp(5.0, 355.0);
            }
            // Corner controls have two degrees of freedom, as in Kinefly.
            s.outer_radius = s
                .hinge
                .distance(p)
                .clamp(s.inner_radius + 2.0, (w as f32).hypot(h as f32));
        }
    }
    if let Some(wing) = handle.wing()
        && !next.tracking.asymmetric
    {
        mirror_wing(&mut next, wing);
    }
    next.validate().context(
        "Move would place a calibration point outside the image or collapse the body/wing axes",
    )?;
    *config = next;
    Ok(())
}
