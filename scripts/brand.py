#!/usr/bin/env python3
"""Generate Rekkefly's brand artwork from code.

    python3 scripts/brand.py            # writes the SVG/PNG/TIFF files below
    python3 scripts/icons.py            # then rasterizes the app icon formats

Needs rsvg-convert (librsvg), ImageMagick (for the DMG TIFF) and the Inter
and Noto Sans Kannada fonts. Outputs:
    assets/rekkefly.svg                      app icon (source for icons.py)
    docs/images/banner-light.png, -dark.png  README header
    docs/images/social-preview.png           GitHub social card (1280×640)
    assets/dmg-background.{svg,png,tiff}     Mac disk image background
"""
import math
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INK = '#122329'        # the fly: a backlit silhouette
WING = '#1f5c66'       # translucent stroke envelope
CORAL = '#ff7059'      # what Rekkefly measures: the leading edge
EYE = '#8a2e27'
SANS = "font-family='Inter Display, Inter, Helvetica Neue, Arial, sans-serif'"
KANNADA = "font-family='Noto Sans Kannada, Noto Serif Kannada, sans-serif'"


def point(cx, cy, r, deg):
    a = math.radians(deg)
    return cx + r * math.cos(a), cy + r * math.sin(a)


def fan(cx, cy, r, a0, a1):
    x0, y0 = point(cx, cy, r, a0)
    x1, y1 = point(cx, cy, r, a1)
    return f'M{cx:.1f} {cy:.1f}L{x0:.1f} {y0:.1f}A{r:.1f} {r:.1f} 0 0 1 {x1:.1f} {y1:.1f}Z'


def fly(cx, cy, s, uid, body=INK, wing=WING, edge=CORAL):
    """A tethered fly seen from above, with both wings' stroke envelopes."""
    out = []
    radius = 288 * s
    hinges = [(cx - 40 * s, cy), (cx + 40 * s, cy)]
    for side, ((hx, hy), front, back) in enumerate([(hinges[0], 232, 122), (hinges[1], -52, 58)]):
        gid = f'{uid}env{side}'
        out.append(
            f"<radialGradient id='{gid}' gradientUnits='userSpaceOnUse' cx='{hx:.1f}' cy='{hy:.1f}' r='{radius:.1f}'>"
            f"<stop offset='0' stop-color='{wing}' stop-opacity='0.04'/>"
            f"<stop offset='0.55' stop-color='{wing}' stop-opacity='0.26'/>"
            f"<stop offset='0.88' stop-color='{wing}' stop-opacity='0.30'/>"
            f"<stop offset='1' stop-color='{wing}' stop-opacity='0.10'/></radialGradient>")
        lo, hi = min(front, back), max(front, back)
        out.append(f"<path d='{fan(hx, hy, radius, lo, hi)}' fill='url(#{gid})'/>")
        # The wing dwells at the stroke reversals: darker bands at both ends.
        out.append(f"<path d='{fan(hx, hy, radius, hi - 9, hi)}' fill='url(#{gid})'/>")
        out.append(f"<path d='{fan(hx, hy, radius, lo, lo + 9)}' fill='url(#{gid})'/>")
    for (hx, hy), front in [(hinges[0], 232), (hinges[1], -52)]:
        x0, y0 = point(hx, hy, 120 * s, front)
        x1, y1 = point(hx, hy, radius + 14 * s, front)
        out.append(f"<path d='M{x0:.1f} {y0:.1f}L{x1:.1f} {y1:.1f}' stroke='{edge}' "
                   f"stroke-width='{13 * s:.1f}' stroke-linecap='round'/>")
    out.append(f"<ellipse cx='{cx}' cy='{cy + 128 * s:.1f}' rx='{40 * s:.1f}' ry='{96 * s:.1f}' fill='{body}'/>")
    for k in range(4):
        y = cy + (88 + 26 * k) * s
        w = (36 - 3 * k) * s
        out.append(f"<path d='M{cx - w:.1f} {y:.1f}Q{cx} {y + 10 * s:.1f} {cx + w:.1f} {y:.1f}' "
                   f"stroke='#ffffff' stroke-opacity='0.12' stroke-width='{6 * s:.1f}' fill='none'/>")
    out.append(f"<ellipse cx='{cx}' cy='{cy:.1f}' rx='{54 * s:.1f}' ry='{70 * s:.1f}' fill='{body}'/>")
    out.append(f"<ellipse cx='{cx}' cy='{cy - 100 * s:.1f}' rx='{50 * s:.1f}' ry='{38 * s:.1f}' fill='{body}'/>")
    for side in (-1, 1):
        out.append(f"<ellipse cx='{cx + side * 30 * s:.1f}' cy='{cy - 104 * s:.1f}' rx='{22 * s:.1f}' "
                   f"ry='{28 * s:.1f}' fill='{EYE}' opacity='0.9'/>")
    for hx, hy in hinges:
        out.append(f"<circle cx='{hx:.1f}' cy='{hy:.1f}' r='{11 * s:.1f}' fill='{edge}'/>")
    return '\n'.join(out)


def tile(x, y, size, uid, shadow=True):
    r = size * 186 / 824
    parts = [f"<linearGradient id='{uid}tile' x1='0' y1='0' x2='0.6' y2='1'>"
             f"<stop offset='0' stop-color='#f4f8f5'/><stop offset='1' stop-color='#d3e3de'/></linearGradient>",
             f"<clipPath id='{uid}clip'><rect x='{x}' y='{y}' width='{size}' height='{size}' rx='{r:.1f}'/></clipPath>"]
    if shadow:
        parts.append(f"<filter id='{uid}shadow' x='-10%' y='-10%' width='120%' height='125%'>"
                     f"<feGaussianBlur in='SourceAlpha' stdDeviation='{size * 14 / 824:.1f}'/>"
                     f"<feOffset dy='{size * 14 / 824:.1f}'/><feComponentTransfer><feFuncA type='linear' slope='0.3'/>"
                     f"</feComponentTransfer><feMerge><feMergeNode/><feMergeNode in='SourceGraphic'/></feMerge></filter>")
    s = size / 824
    body = (f"<g{f' filter=\"url(#{uid}shadow)\"' if shadow else ''}><rect x='{x}' y='{y}' width='{size}' "
            f"height='{size}' rx='{r:.1f}' fill='url(#{uid}tile)'/></g>"
            f"<g clip-path='url(#{uid}clip)'>{fly(x + 412 * s, y + 400 * s, s, uid)}</g>"
            f"<rect x='{x + 2 * s:.1f}' y='{y + 2 * s:.1f}' width='{size - 4 * s:.1f}' height='{size - 4 * s:.1f}' "
            f"rx='{r - 2 * s:.1f}' fill='none' stroke='#000000' stroke-opacity='0.08' stroke-width='{4 * s:.1f}'/>")
    return '\n'.join(parts), body


def svg(width, height, defs, body, background=None):
    bg = f"<rect width='{width}' height='{height}' fill='{background}'/>" if background else ''
    return (f"<svg xmlns='http://www.w3.org/2000/svg' width='{width}' height='{height}' "
            f"viewBox='0 0 {width} {height}'><defs>{defs}</defs>{bg}{body}</svg>")


def render(text, path, scale=2, size=None):
    width = ['-w', str(size)] if size else ['-z', str(scale)]
    subprocess.run(['rsvg-convert', *width, '-o', str(path)], input=text.encode(), check=True)


def main():
    images = ROOT / 'docs/images'
    images.mkdir(parents=True, exist_ok=True)
    defs, body = tile(100, 100, 824, 'i')
    (ROOT / 'assets/rekkefly.svg').write_text(svg(1024, 1024, defs, body))

    for mode, ink, muted in [('light', '#122329', '#5b6b6e'), ('dark', '#eef4f2', '#9fb1b3')]:
        defs, body = tile(24, 28, 264, 'b', shadow=True)
        text = (f"<text x='328' y='150' {SANS} font-size='108' font-weight='600' letter-spacing='-2' fill='{ink}'>Rekkefly</text>"
                f"<text x='332' y='212' {SANS} font-size='38' font-weight='500' fill='{CORAL}'>The new Kinefly</text>"
                f"<text x='332' y='262' {SANS} font-size='25' fill='{muted}'>Real-time wing tracking for tethered insects · Mac · Windows · Linux</text>"
                f"<text x='332' y='300' {KANNADA} font-size='22' fill='{muted}'>ರೆಕ್ಕೆ</text>"
                f"<text x='384' y='300' {SANS} font-size='22' fill='{muted}'>(rekke, Kannada for wing) + fly, rooted in Kinefly</text>")
        render(svg(1320, 330, defs, body + text), images / f'banner-{mode}.png')

    defs, body = tile(96, 140, 360, 's', shadow=False)
    social = (f"<linearGradient id='bg' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='#163640'/>"
              f"<stop offset='1' stop-color='#0a171c'/></linearGradient>{defs}")
    lines = ['Ignores legs and antennae', 'Kinefly-compatible ZMQ output', 'Many rigs from one command',
             'Cameras or video files, no ROS']
    features = ''.join(
        f"<circle cx='552' cy='{392 + 44 * k}' r='6' fill='{CORAL}'/>"
        f"<text x='572' y='{400 + 44 * k}' {SANS} font-size='27' fill='#d7e4e2'>{line}</text>"
        for k, line in enumerate(lines))
    text = (f"<text x='540' y='228' {SANS} font-size='104' font-weight='600' letter-spacing='-2' fill='#f2f7f5'>Rekkefly</text>"
            f"<text x='544' y='296' {SANS} font-size='42' font-weight='500' fill='{CORAL}'>The new Kinefly</text>"
            f"<text x='544' y='342' {SANS} font-size='25' fill='#9fb1b3'>Real-time wing tracking for tethered insects</text>"
            f"{features}")
    render(svg(1280, 640, social, f"<rect width='1280' height='640' fill='url(#bg)'/>{body}{text}"),
           images / 'social-preview.png', size=1280)

    defs, body = tile(64, 30, 80, 'd', shadow=False)
    dmg = (f"<linearGradient id='bg' x2='1' y2='1'><stop stop-color='#fbfdfc'/><stop offset='1' stop-color='#e6f0eb'/>"
           f"</linearGradient>{defs}")
    dmg_body = (f"<rect width='640' height='420' fill='url(#bg)'/>{body}"
                f"<text x='160' y='70' {SANS} font-size='34' font-weight='600' fill='{INK}'>Rekkefly</text>"
                f"<text x='162' y='97' {SANS} font-size='15' fill='#5b6b6e'>The new Kinefly · wing tracking for tethered insects</text>"
                "<circle cx='180' cy='207' r='79' fill='#fff' opacity='.58'/>"
                "<circle cx='460' cy='207' r='79' fill='#fff' opacity='.58'/>"
                "<path d='M296 208H342M328 193L344 208L328 223' fill='none' stroke='#7a9d8e' stroke-width='5' "
                "stroke-linecap='round' stroke-linejoin='round'/>"
                f"<text x='320' y='352' text-anchor='middle' {SANS} font-size='18' font-weight='500' fill='#294c41'>Drag Rekkefly to Applications</text>"
                f"<text x='320' y='378' text-anchor='middle' {SANS} font-size='13' fill='#647b73'>Then open Rekkefly from your Applications folder</text>")
    dmg_svg = svg(640, 420, dmg, dmg_body)
    assets = ROOT / 'assets'
    (assets / 'dmg-background.svg').write_text(dmg_svg)
    render(dmg_svg, assets / 'dmg-background.png', scale=1)
    render(dmg_svg, assets / 'dmg-background@2x.png', scale=2)
    # Two-page TIFF (1× at 72 dpi, 2× at 144 dpi) for sharp Retina DMGs.
    subprocess.run(['convert', '-units', 'PixelsPerInch',
                    '(', str(assets / 'dmg-background.png'), '-density', '72', ')',
                    '(', str(assets / 'dmg-background@2x.png'), '-density', '144', ')',
                    '-compress', 'lzw', str(assets / 'dmg-background.tiff')], check=True)
    (assets / 'dmg-background@2x.png').unlink()
    print('Brand artwork written')


if __name__ == '__main__':
    main()
