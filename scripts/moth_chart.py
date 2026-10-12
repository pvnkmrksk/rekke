#!/usr/bin/env python3
"""Draw the README's real-footage antenna chart (light and dark): how far
painted antennae move each detector's L − R on the Bogong moth example.

    REKKEFLY_MOTH_TRACE=moth.csv cargo test --release --test moth antenna_trace -- --ignored
    python3 scripts/moth_chart.py moth.csv docs/images/moth-antennae
Writes <prefix>-light.png and <prefix>-dark.png (needs rsvg-convert).
"""
import csv
import subprocess
import sys

THEMES = {
    'light': dict(surface='#fcfcfb', ink='#0b0b0b', ink2='#52514e', muted='#898781',
                  grid='#e1e0d9', axis='#c3c2b7', robust='#2a78d6', classic='#eb6834'),
    'dark': dict(surface='#1a1a19', ink='#f0efec', ink2='#c3c2b7', muted='#898781',
                 grid='#2c2c2a', axis='#4a4a46', robust='#3987e5', classic='#d95926'),
}
W, H = 1200, 680
L, R = 96, 1160
PANELS = [(170, 330), (420, 580)]
LIMIT = 120.0
FONT = "font-family='Helvetica Neue, Helvetica, Arial, DejaVu Sans, sans-serif'"


def main():
    source, prefix = sys.argv[1], sys.argv[2]
    rows = list(csv.DictReader(open(source)))
    t = [float(r['t']) for r in rows]
    end = t[-1] + (t[1] - t[0])

    def change(key, reference):
        out = []
        for r in rows:
            a, b = r[key], r[reference]
            out.append(None if not a or not b else float(a) - float(b))
        return out

    classic = change('classic', 'classic_clean')
    robust = change('robust', 'clean')
    moved = lambda s: sum(1 for v in s if v is None or abs(v) > 5)
    x = lambda v: L + (R - L) * v / end

    for name, c in THEMES.items():
        g = [f"<rect width='{W}' height='{H}' rx='14' fill='{c['surface']}'/>"]
        g.append(f"<text x='{L - 60}' y='52' {FONT} font-size='25' font-weight='600' fill='{c['ink']}'>Real moth, painted antennae: how far each detector's L − R moves</text>")
        g.append(f"<text x='{L - 60}' y='82' {FONT} font-size='16' fill='{c['ink2']}'>Bogong moth example, 30 s at 30 fps. Two thin bright lines sweep from the head through both wing sectors;</text>")
        g.append(f"<text x='{L - 60}' y='104' {FONT} font-size='16' fill='{c['ink2']}'>each detector is compared with its own result on the same frame without them. 0° means unaffected.</text>")
        for (top, bottom), series, color, label in [
            (PANELS[0], classic, c['classic'], f"Classic strongest edge (Kinefly-style): L − R moved more than 5° in {moved(classic)} of {len(t)} frames"),
            (PANELS[1], robust, c['robust'], f"Rekkefly (default): L − R moved more than 5° in {moved(robust)} of {len(t)} frames"),
        ]:
            y = lambda v, top=top, bottom=bottom: (top + bottom) / 2 - (bottom - top) / 2 * max(-1, min(1, v / LIMIT))
            g.append(f"<circle cx='{L - 54}' cy='{top - 26}' r='6' fill='{color}'/>")
            g.append(f"<text x='{L - 40}' y='{top - 20}' {FONT} font-size='16' font-weight='600' fill='{c['ink']}'>{label}</text>")
            for v in (-LIMIT, -60, 0, 60, LIMIT):
                stroke = c['axis'] if v == 0 else c['grid']
                g.append(f"<line x1='{L}' y1='{y(v):.1f}' x2='{R}' y2='{y(v):.1f}' stroke='{stroke}' stroke-width='1'/>")
                text = f"{v:+.0f}°".replace('+0°', '0°')
                g.append(f"<text x='{L - 12}' y='{y(v) + 5:.1f}' {FONT} font-size='13' fill='{c['muted']}' text-anchor='end'>{text}</text>")
            parts, pen = [], 'M'
            for ti, v in zip(t, series):
                if v is None:
                    pen = 'M'
                    continue
                parts.append(f"{pen}{x(ti):.1f} {y(v):.1f}")
                pen = 'L'
            g.append(f"<path d='{' '.join(parts)}' fill='none' stroke='{color}' stroke-width='1.6' stroke-linejoin='round'/>")
        for s in range(0, int(end) + 1, 5):
            g.append(f"<text x='{x(s):.1f}' y='{PANELS[1][1] + 26}' {FONT} font-size='13' fill='{c['muted']}' text-anchor='middle'>{s} s</text>")
        g.append(f"<text x='{L - 60}' y='{H - 22}' {FONT} font-size='13' fill='{c['muted']}'>Values beyond ±{LIMIT:.0f}° are drawn at the edge. Reproduce: REKKEFLY_MOTH_TRACE=moth.csv cargo test --release --test moth antenna_trace -- --ignored</text>")
        svg = f"<svg xmlns='http://www.w3.org/2000/svg' width='{W}' height='{H}' viewBox='0 0 {W} {H}'>{''.join(g)}</svg>"
        out = f'{prefix}-{name}.png'
        subprocess.run(['rsvg-convert', '-w', str(W * 2), '-o', out], input=svg.encode(), check=True)
        print(out)


if __name__ == '__main__':
    main()
