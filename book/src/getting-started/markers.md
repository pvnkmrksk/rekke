# Placing the markers

Good marker placement matters more than any detector setting. On the Bogong
moth, hinges placed 20 px from the wing bases doubled the noise in L − R, and a
sector that clipped the body caused large L − R spikes. This page explains what
each marker means, so you can place them right the first time.

<svg class="figure" viewBox="0 0 680 420" role="img" aria-labelledby="geom-title geom-desc" xmlns="http://www.w3.org/2000/svg">
<title id="geom-title">Sector geometry</title>
<desc id="geom-desc">A tethered insect seen from above with its head up. Each wing has an annular sector centred on its hinge, from the inner radius just past the body to the outer radius inside the wing blur. The leading edge of the blur, on the head side, is the reported wing angle, measured from the perpendicular to the body axis and positive toward the head.</desc>
<defs><radialGradient id="geom-blur" gradientUnits="userSpaceOnUse" cx="0" cy="0" r="165"><stop offset="0" stop-opacity="0.05" class="g-blur"/><stop offset="0.6" stop-opacity="0.32" class="g-blur"/><stop offset="1" stop-opacity="0.16" class="g-blur"/></radialGradient></defs>
<g transform="translate(300 200)"><path d="M-139.5 74.2A158 158 0 0 1 -105.7 -117.4L-5.4 -5.9A8 8 0 0 0 -7.1 3.8Z" fill="url(#geom-blur)"/></g>
<g transform="translate(380 200)"><path d="M105.7 -117.4A158 158 0 0 1 139.5 74.2L7.1 3.8A8 8 0 0 0 5.4 -5.9Z" fill="url(#geom-blur)"/></g>
<path class="g-sector g-left" d="M173.6 306.1A165 165 0 0 1 243.6 45.0L288.4 168.1A34 34 0 0 0 274.0 221.9Z"/>
<path class="g-sector g-right" d="M436.4 45.0A165 165 0 0 1 506.4 306.1L406.0 221.9A34 34 0 0 0 391.6 168.1Z"/>
<line class="g-axis" x1="340" y1="40" x2="340" y2="400"/>
<ellipse class="g-body" cx="340" cy="300" rx="22" ry="72"/>
<ellipse class="g-body" cx="340" cy="205" rx="40" ry="40"/>
<ellipse class="g-body" cx="340" cy="148" rx="24" ry="19"/>
<line class="g-zero" x1="300" y1="200" x2="123.0" y2="200.0"/>
<line class="g-zero" x1="380" y1="200" x2="557.0" y2="200.0"/>
<line class="g-edge" x1="277.2" y1="174.7" x2="189.6" y2="77.4"/>
<line class="g-edge" x1="402.8" y1="174.7" x2="490.4" y2="77.4"/>
<path class="g-angle" d="M182.0 200.0A118 118 0 0 1 221.0 112.3"/>
<text class="g-label g-strong" x="179.4" y="146.3" text-anchor="end">+φ</text>
<circle class="g-hinge" cx="300" cy="200" r="5"/>
<circle class="g-hinge" cx="380" cy="200" r="5"/>
<circle class="g-dot" cx="267.0" cy="208.2" r="3.5"/>
<circle class="g-dot" cx="139.9" cy="239.9" r="3.5"/>
<circle class="g-head" cx="340" cy="129" r="5.5"/>
<circle class="g-tail" cx="340" cy="368" r="5.5"/>
<text class="g-label" x="352" y="126" text-anchor="start">head</text>
<text class="g-label" x="352" y="380" text-anchor="start">tail</text>
<text class="g-label" x="346" y="52" text-anchor="start">body axis</text>
<text class="g-label g-coral" x="181.6" y="73.4" text-anchor="end">leading edge</text>
<text class="g-label" x="125.0" y="192" text-anchor="end">0°</text>
<text class="g-small" x="125.0" y="210" text-anchor="end">perpendicular</text>
<text class="g-small" x="125.0" y="224" text-anchor="end">to the body</text>
<text class="g-label" x="259.0" y="224.2" text-anchor="end">inner radius</text>
<text class="g-small" x="259.0" y="238.2" text-anchor="end">just past the body</text>
<text class="g-label" x="133.9" y="255.9" text-anchor="end">outer radius</text>
<text class="g-small" x="133.9" y="269.9" text-anchor="end">inside the blur</text>
<text class="g-label" x="386" y="216" text-anchor="start">hinge</text>
<text class="g-small" x="526.9" y="300.5" text-anchor="start">stroke envelope</text>
<text class="g-small" x="526.9" y="314.5" text-anchor="start">(motion blur)</text>
<text class="g-label" x="516.4" y="93.9" text-anchor="start">sector</text>
<text class="g-small" x="516.4" y="107.9" text-anchor="start">start → start + span</text>
<text class="g-label g-red" x="24" y="30" text-anchor="start">left wing</text>
<text class="g-label g-green" x="656" y="30" text-anchor="end">right wing</text>
</svg>

## The body axis: head and tail

Drag the **magenta** dot onto the head and the **cyan** dot onto the tail (the
tip of the abdomen). The line between them is the body axis. Every wing angle is
measured relative to it, so it does not matter how the insect is rotated in the
image. Put the dots on the midline, as far apart as the insect allows: a longer
axis is a more precise one.

## The wing hinges

The red (left) and green (right) dots are the **wing hinges**: the points each
wing turns about, at its base. The sector is drawn around the hinge, and every
edge angle is measured from it.

A hinge in the wrong place does not make the trace look wrong, it makes it
*biased*: if the hinge sits away from the true pivot, the edge you measure moves
in angle as the wing sweeps, and the error changes with the stroke. To find the
true hinge, watch the leading edge across several frames. Straight lines along
it all pass through one point, and that point is the hinge. (For the Bogong
moth example, lines fitted to 190 frames of one wing passed within 0.7 px of one
point; see [The Bogong moth example](bogong-moth.md).)

While **Asymmetric** is off, the wings mirror each other across the body axis:
drag one hinge and the other follows. Turn it on for insects that sit at an
angle or rigs where the two wings look different.

## The sectors

Each wing is tracked inside an **annular sector**: a ring segment around its
hinge, bounded by two radii and two angles.

| Setting | What to aim for |
| --- | --- |
| **Inner radius** (`inner_radius`) | Just past the body. Closer in, the hinge region and the thorax edge dominate the profile, and every angle bin covers only a few pixels |
| **Outer radius** (`outer_radius`) | Inside the wing blur. Beyond the blur there is only background and noise; a little short of the wing tip is ideal |
| **Start and span** (`start_deg`, `span_deg`) | Cover the whole stroke, from the most backward to the most forward position, with a few degrees of margin. Stay clear of the body, the head, the legs and the antennae |

To edit, drag the **corner dots** (angle and outer radius at once), the
**middle dot of an arc** (only that radius), or the **white centre dot** (the
whole setup). Precise values can be typed in **Settings**.

Sector angles in the config are image angles: degrees clockwise from
image-right, as the image's y axis points down. You rarely need to type them;
drag instead.

## What gets measured

Inside the sector, Rekkefly averages the image along each ray from the hinge
and builds an **angular profile**: brightness against angle. The blurred wing
is a broad plateau in that profile. Its edge toward the head is the **leading
edge**, and its angle is the wing angle:

- **0°** is the outward perpendicular to the body axis (the dashed line);
- angles are **positive toward the head**, negative toward the tail;
- **L − R** is the left angle minus the right angle; **L + R** is their sum.

These are Kinefly's conventions, so values compare directly with Kinefly data.
How the edge is found, and how legs and antennae are ignored, is described in
[The detectors](../tracking/detectors.md).

## Head and abdomen

Two more sectors track the head and the abdomen (toolbar **H** and **A**; off
by default). Their hinge is the pivot (the neck, or the petiole), and the
sector should cover the part's range of motion. They use the intensity
**centroid** rather than an edge, and report an angle along the body axis,
clockwise positive in the image.

## A checklist

1. The head and tail dots are on the midline.
2. Each hinge is on the wing base, where the leading edges meet.
3. The inner radius is just past the body: no part of the thorax inside the sector.
4. The outer radius ends inside the wing blur.
5. The sectors cover the full stroke, plus a few degrees, and miss the legs and antennae.
6. With **Settings → Show signal diagnostics**, L − R is smooth while the insect flies.

Agents can do the same check: the MCP `snapshot` tool returns the frame with
the sectors and edges drawn on it ([AI agents](../guide/agents.md)).
