# Angles and sectors

This page defines exactly what Rekkefly measures. The conventions are
Kinefly's, so numbers compare directly with Kinefly data.

## Wing angles

A wing's angle is the direction of its reported edge, seen from its hinge:

- **0** is the outward perpendicular to the body axis (head–tail line);
- **positive** is toward the head, **negative** toward the tail;
- the left and right wings use mirror-image conventions, so a symmetric stroke
  gives equal left and right angles.

From these:

| Value | Meaning |
| --- | --- |
| **L − R** (`left_minus_right_rad`, ZMQ `yaw`) | Left minus right. The usual steering signal: positive when the left wing beats further forward than the right |
| **L + R** (`left_plus_right_rad`) | Their sum. Follows overall stroke amplitude, for thrust |

All transmitted angles are in **radians**. Summaries, CSV files and MCP tool
results use **degrees**. Geometry settings (`start_deg`, `span_deg`) are degrees.

Because the angle is measured relative to the body axis, it does not depend on
how the insect sits in the image, and stays correct if the camera is rotated.

## Image angles

Sectors are defined in **image angles**: degrees clockwise from image-right
(0° points right, 90° points down, because the image's y axis points down).
Each measurement also reports `image_angle_rad`, the reported edge's direction
in this frame. You rarely need either: drag the markers instead.

A sector runs clockwise from `start_deg` to `start_deg + span_deg`, between
`inner_radius` and `outer_radius` pixels from its hinge. `span_deg` can be 5° to
355°. `start_deg` may be any value; angles wrap.

## Sampling

Each sector is sampled on a polar grid of `angle_bins` angles (240 by default)
by `radial_samples` radii (72). Pixel values are read with bilinear
interpolation from positions computed once, when the geometry changes. Moving a
marker rebuilds them; changing a threshold does not.

Pixel values are used as they are, inverted (`dark_on_light = true`, the default,
for a dark insect on a light background), or as the absolute difference from the
saved background (`subtract_background = true`). These transforms are applied
while sampling, so no copy of the frame is ever made.

## Coverage

Parts of a sector outside the image cannot be measured. `coverage` reports the
fraction of the sector inside the image, and angle bins with less than 90% of
their samples inside the image are never reported as edges. Keep sectors inside
the image.

## Missing values

An angle is **missing** (`null` in UDP and JSONL, an empty cell in CSV, `0.0` on
ZMQ for Kinefly compatibility) when the part is turned off, or when no edge is
stronger than the part's `threshold`. A missing value is never replaced by the
previous one.

## Head and abdomen

Head and abdomen sectors use the **centroid** method: the intensity-weighted
mean angle of the sector, and its mean radius (`radius_px`). Their angles
follow the body axis, with clockwise positive in the image. Both are off by
default; turn them on with **H** and **A** in the toolbar.
