#!/usr/bin/env python3
"""Draw the README's throughput chart (light and dark).

    cargo test --release --test moth throughput -- --ignored --nocapture
    python3 scripts/perf_chart.py docs/images/throughput 970 2040 3960 3770
The four numbers are total frames per second for 1, 2, 4 and 8 rigs.
Writes <prefix>-light.png and <prefix>-dark.png (needs rsvg-convert).
"""
import subprocess
import sys

THEMES = {
    'light': dict(surface='#fcfcfb', ink='#0b0b0b', ink2='#52514e', muted='#898781',
                  grid='#e1e0d9', axis='#c3c2b7', bar='#2a78d6'),
    'dark': dict(surface='#1a1a19', ink='#f0efec', ink2='#c3c2b7', muted='#898781',
                 grid='#2c2c2a', axis='#4a4a46', bar='#3987e5'),
}
W, H = 1200, 520
L, R, T, B = 110, 1150, 140, 440
TOP = 4500
FONT = "font-family='Helvetica Neue, Helvetica, Arial, DejaVu Sans, sans-serif'"


def main():
    prefix = sys.argv[1]
    values = [float(v) for v in sys.argv[2:6]]
    rigs = [1, 2, 4, 8]
    y = lambda v: B - (B - T) * v / TOP
    slot = (R - L) / len(rigs)
    for name, c in THEMES.items():
        g = [f"<rect width='{W}' height='{H}' rx='14' fill='{c['surface']}'/>"]
        g.append(f"<text x='{L - 70}' y='52' {FONT} font-size='25' font-weight='600' fill='{c['ink']}'>Frames per second, every rig decoding and tracking its own Bogong moth video</text>")
        g.append(f"<text x='{L - 70}' y='82' {FONT} font-size='16' fill='{c['ink2']}'>640×480 MJPEG, robust detector, as fast as possible, on a 4-core 2.8 GHz Xeon. Totals across rigs; ±15% between runs.</text>")
        g.append(f"<line x1='{L - 70}' y1='105' x2='{L - 40}' y2='105' stroke='{c['ink2']}' stroke-width='1.5' stroke-dasharray='6 5'/>")
        g.append(f"<text x='{L - 30}' y='110' {FONT} font-size='16' fill='{c['ink2']}'>240 frames/s: what eight cameras at 30 fps need</text>")
        for v in range(0, TOP + 1, 1000):
            stroke = c['axis'] if v == 0 else c['grid']
            g.append(f"<line x1='{L}' y1='{y(v):.1f}' x2='{R}' y2='{y(v):.1f}' stroke='{stroke}' stroke-width='1'/>")
            g.append(f"<text x='{L - 12}' y='{y(v) + 5:.1f}' {FONT} font-size='14' fill='{c['muted']}' text-anchor='end'>{v:,}</text>")
        need = 8 * 30
        g.append(f"<line x1='{L}' y1='{y(need):.1f}' x2='{R}' y2='{y(need):.1f}' stroke='{c['ink2']}' stroke-width='1.5' stroke-dasharray='6 5'/>")
        for k, (n, v) in enumerate(zip(rigs, values)):
            cx = L + slot * (k + 0.5)
            bw = slot * 0.46
            top = y(v)
            g.append(f"<path d='M{cx - bw / 2:.1f} {B} V{top + 4:.1f} Q{cx - bw / 2:.1f} {top:.1f} {cx - bw / 2 + 4:.1f} {top:.1f} H{cx + bw / 2 - 4:.1f} Q{cx + bw / 2:.1f} {top:.1f} {cx + bw / 2:.1f} {top + 4:.1f} V{B} Z' fill='{c['bar']}'/>")
            g.append(f"<text x='{cx:.1f}' y='{top - 12:.1f}' {FONT} font-size='17' font-weight='600' fill='{c['ink']}' text-anchor='middle'>{v:,.0f}</text>")
            per = f"{v / n:,.0f} per rig"
            g.append(f"<text x='{cx:.1f}' y='{B + 30}' {FONT} font-size='16' fill='{c['ink']}' text-anchor='middle'>{n} rig{'s' if n > 1 else ''}</text>")
            g.append(f"<text x='{cx:.1f}' y='{B + 52}' {FONT} font-size='14' fill='{c['muted']}' text-anchor='middle'>{per}</text>")
        svg = f"<svg xmlns='http://www.w3.org/2000/svg' width='{W}' height='{H}' viewBox='0 0 {W} {H}'>{''.join(g)}</svg>"
        out = f'{prefix}-{name}.png'
        subprocess.run(['rsvg-convert', '-w', str(W * 2), '-o', out], input=svg.encode(), check=True)
        print(out)


if __name__ == '__main__':
    main()
