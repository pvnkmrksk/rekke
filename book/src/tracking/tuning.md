# Tuning a rig

Most rigs need no tuning beyond [placing the markers](../getting-started/markers.md)
well. When L − R is noisy, spiky or biased, work through this page in order:
the first items fix most problems.

Open **Settings → Show signal diagnostics** first. Every step below is judged
by watching the L − R trace and the angular profile.

## 1. Image quality

| Symptom | Fix |
| --- | --- |
| The wing blur is barely visible | More backlight, a longer exposure, or a lens aperture that gives more contrast |
| The background is blown out (pure white) | Shorter exposure or less light: a saturated background has no edge |
| The blur flickers between frames | Lighting flicker (mains-powered LEDs or fluorescent tubes). Use DC lighting, or an exposure that is a multiple of the mains period |
| The image is noisy | More light rather than more camera gain |

## 2. Markers

| Symptom | Fix |
| --- | --- |
| L − R drifts with stroke amplitude | The hinges are off the wing bases. Find where leading edges meet ([how](../getting-started/markers.md#the-wing-hinges)) |
| Spikes when the insect moves its body | A sector clips the thorax or the head. Raise the inner radius or move the sector's tail-side limit |
| Spikes when the legs move | A sector reaches the legs. Pull its limits in, or rely on the robust detector (step 3) |
| The edge sits at a sector limit | The stroke goes beyond the sector. Widen the span |
| The edge is noisy far out | The outer radius is beyond the blur. Pull it in |

## 3. Detector

Use the robust detector (**Ignore legs** ticked, `method = "wing_blur"`) unless
you need to reproduce old Kinefly numbers exactly.

| Setting | Default | When to change it |
| --- | --- | --- |
| `primary_edge` | `"strongest"` | Set `"toward_head"` to always report the leading edge (stroke amplitude), as Kinefly does. Recommended |
| `spike_width_deg` (**Ignore features narrower than**) | 5° | Raise for thick or out-of-focus legs; keep it well below the width of the wing's blur. Lower it for very small insects at low magnification |
| `contrast_cap` | 0.12 | Lower it if opaque legs still pull the edge; raise it if a pale wing gives a weak profile. The value is a fraction of full brightness |
| `threshold` | 0 | Raise it so that a stopped wing reports *missing* instead of noise. Set it below the **peak** strength shown in Settings during flight |
| `max_edges` | 2 | Only affects how many extra edges are listed in `angles_rad` |

## 4. Background and polarity

| Setting | When |
| --- | --- |
| **Invert** off (`dark_on_light = false`) | A bright insect on a dark background (the Bogong moth) |
| **Save BG** + **Subtract** | A cluttered or uneven background: the tether, arena walls, a gradient. Save the background with the insect *out of view* |

## 5. Sampling

The defaults (240 angle bins × 72 radial samples) suit 640×480. For large sectors
at high resolution, more angle bins give finer angles at a small cost; see
[Performance](performance.md).

## Check the result

1. Watch L − R in the debug traces while the insect flies: no spikes when the
   legs or body move.
2. Compare left and right: with a symmetric stimulus they move together.
3. Record a minute, then compare detectors offline:

   ```sh
   rekkefly track trial.avi --config rig.toml --method robust  --csv robust.csv
   rekkefly track trial.avi --config rig.toml --method classic --csv classic.csv
   ```

4. Agents can do the same with `snapshot` and `measure`
   ([AI agents](../guide/agents.md)).
