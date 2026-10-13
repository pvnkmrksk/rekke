# The detectors

Rekkefly has two wing detectors. The **robust** detector (`method = "wing_blur"`,
**Ignore legs** in the toolbar) is the default. The **classic** detector
(`method = "edge"`) is Kinefly's approach, kept for comparison and for existing
setups.

## The problem: narrow things in the wing sector

A beating wing photographed over many wingbeats is a translucent **stroke
envelope**: a broad *shoulder* of blur tens of degrees wide. Its leading edge is
what wingbeat amplitude means.

A leg, an antenna, a bristle or the arista is **narrow**. It is localized in
angle, and usually in radius, and it can sit inside the blur or outside it. It
need not be darker than the wing: because it is in focus, its edges are
*sharper* than the wing's soft edge. A detector that reports the sharpest edge
therefore jumps onto it, and **L − R spikes** by tens of degrees, exactly when a
closed-loop experiment can least afford it.

![Front legs reaching forward. White dashes: the true leading edge. Orange: the detected edge. Classic detector (left) vs. Rekkefly (right).](../images/legs-classic-vs-robust.png)

*Front legs reaching forward. White dashes: the true leading edge. Orange: the
detected edge. Classic detector (left) and Rekkefly (right).*

## Classic: the strongest edge

1. Average the sector along each ray into one **angular profile**: intensity
   against angle.
2. Smooth it (three-bin box) and take its derivative (central difference over
   ±2 bins).
3. Report the steepest edges, strongest first.

This is fast and precise on a clean wing, and fails as soon as something sharp
enters the sector.

## Robust: the edge of the blur shoulder

The robust detector tracks the shoulder rather than the sharpest edge:

1. **Bands.** The sector is split into up to 8 radial bands, each giving its own
   angular profile.
2. **Contrast is capped** at `contrast_cap` above each band's background, so an
   opaque leg cannot outweigh a translucent wing.
3. **Narrow features are removed.** A morphological opening deletes anything
   narrower than `spike_width_deg` (default 5°) in each band, however dark it
   is. A broad shoulder passes through unchanged, edges included.
4. **Median across bands.** Something present at only some radii, as a leg or
   an antenna usually is, is out-voted.
5. **The shoulder decides.** Each remaining edge is scored by its steepness
   *times the blur shoulder behind it*: the mean level over a wide window on
   either side (four times `spike_width_deg`, between 12° and 45°: 20° by
   default). A narrow line has almost no shoulder, even where it touches or
   crosses the wing; the wing's true edge has the whole envelope behind it. An
   edge whose window is cut short by the end of the sector counts for less.

The angle itself still comes from the steepest point of the winning edge, so the
robust detector is as precise as the classic one: the median error on synthetic
flies with known angles is about 0.25°.

The debug traces show both stages: in the angular profile panel, thin is the
raw profile and bold is what the robust detector ranks edges on, with legs and
antennae removed.

## Which edge is reported

A blurred wing has two edges in its sector: the leading edge on the head side,
and the trailing edge toward the tail. `primary_edge` chooses:

| `primary_edge` | Reports |
| --- | --- |
| `"toward_head"` | The **leading edge**: where the wing gives way to background on the head side. Kinefly's stroke-amplitude measure |
| `"toward_tail"` | The trailing edge |
| `"strongest"` (default for new configs) | Whichever edge ranks first |

The leading and trailing edges are told apart by **polarity** (which side is
wing and which is background), not by position. A stray feature beyond the
wing, such as an antenna or the arena wall, therefore cannot take the leading
edge's place. If no edge of that polarity is found, the outermost edge in that
direction is reported.

Up to `max_edges` edges (default 2) are reported in `angles_rad`, the primary
one first. Edges closer than 4 bins, or on the same derivative lobe, are reported once.

## Threshold

`threshold` is the minimum edge strength (the absolute derivative of the
normalized profile, 0–1). At **0**, the default, the strongest edge is always
reported, however faint. Raise it so that frames with no wing visible (the
insect stopped flying) report a missing angle instead of noise. The live **peak**
strength in Settings shows what a good edge scores on your rig.

## Centroid

`method = "centroid"` reports the intensity-weighted mean angle of the sector.
It is meant for the head and the abdomen, which are solid and move as a whole.

## Limits

- Features **wider** than `spike_width_deg`, such as a badly out-of-focus leg
  10 px wide, can still win now and then. Raise **Ignore features narrower
  than** for very thick legs, keeping it narrower than the wing's blur.
- A feature that touches the leading edge along its whole length is
  indistinguishable from the wing itself.
- **Marker placement matters most.** No detector fixes a hinge in the wrong
  place or a sector that clips the body; see
  [Placing the markers](../getting-started/markers.md).

The evidence for these claims, on synthetic flies with known angles and on the
real Bogong moth, is in [Evidence](evidence.md).
