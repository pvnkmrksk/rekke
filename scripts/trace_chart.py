#!/usr/bin/env python3
"""Draw the README's L−R comparison chart (light and dark) from a trace CSV.

    REKKEFLY_TRACE=trace.csv cargo test --release --test intrusions trace -- --ignored
    python3 scripts/trace_chart.py trace.csv docs/images/lr-trace
Writes <prefix>-light.png and <prefix>-dark.png (needs rsvg-convert).
"""
import csv
import subprocess
import sys

THEMES = {
    'light': dict(surface='#fcfcfb', ink='#0b0b0b', ink2='#52514e', muted='#898781',
                  grid='#e1e0d9', axis='#c3c2b7', truth='#52514e', robust='#2a78d6', classic='#eb6834'),
    'dark': dict(surface='#1a1a19', ink='#f0efec', ink2='#c3c2b7', muted='#898781',
                 grid='#2c2c2a', axis='#383835', truth='#c3c2b7', robust='#3987e5', classic='#d95926'),
}
W, H = 1200, 560
L, R, T, B = 86, 1160, 150, 500
Y0, Y1 = -100.0, 120.0
X1 = 12.0
FONT = "font-family='Helvetica Neue, Helvetica, Arial, DejaVu Sans, sans-serif'"


def main():
    source, prefix = sys.argv[1], sys.argv[2]
    rows = list(csv.DictReader(open(source)))
    t = [float(r['t']) for r in rows]

    def series(key):
        return [float(r[key]) if r[key] else None for r in rows]

    truth, classic, robust = series('truth'), series('classic'), series('robust')
    wrong = lambda s: sum(1 for v, g in zip(s, truth) if v is None or abs(v - g) > 4)
    worst = lambda s: max(abs(v - g) for v, g in zip(s, truth) if v is not None)
    x = lambda v: L + (R - L) * v / X1
    y = lambda v: B - (B - T) * (min(max(v, Y0), Y1) - Y0) / (Y1 - Y0)

    def path(values):
        parts, pen = [], 'M'
        for ti, v in zip(t, values):
            if v is None:
                pen = 'M'
                continue
            parts.append(f'{pen}{x(ti):.1f} {y(v):.1f}')
            pen = 'L'
        return ' '.join(parts)

    for name, c in THEMES.items():
        g = []
        g.append(f"<rect width='{W}' height='{H}' rx='14' fill='{c['surface']}'/>")
        g.append(f"<text x='{L - 50}' y='52' {FONT} font-size='25' font-weight='600' fill='{c['ink']}'>L − R while legs, antennae and thin lines cross the wings</text>")
        g.append(f"<text x='{L - 50}' y='82' {FONT} font-size='16' fill='{c['ink2']}'>Synthetic tethered fly with known wing angles, 12 s at 30 fps. Same frames, two detectors.</text>")
        # Legend row.
        lx = L - 50
        for label, color, width, alpha in [('Ground truth', c['truth'], 8, 0.28),
                                           ('Rekkefly (shoulder detector, default)', c['robust'], 2.5, 1),
                                           ('Classic strongest edge (Kinefly-style)', c['classic'], 2.5, 1)]:
            g.append(f"<line x1='{lx}' y1='117' x2='{lx + 28}' y2='117' stroke='{color}' stroke-width='{width}' stroke-opacity='{alpha}' stroke-linecap='round'/>")
            g.append(f"<text x='{lx + 38}' y='122' {FONT} font-size='15' fill='{c['ink2']}'>{label}</text>")
            lx += 38 + len(label) * 7.6 + 36
        for v in range(-80, 121, 40):
            stroke = c['axis'] if v == 0 else c['grid']
            g.append(f"<line x1='{L}' y1='{y(v):.1f}' x2='{R}' y2='{y(v):.1f}' stroke='{stroke}' stroke-width='1'/>")
            g.append(f"<text x='{L - 12}' y='{y(v) + 5:.1f}' {FONT} font-size='14' fill='{c['muted']}' text-anchor='end'>{v:+d}°</text>".replace('+0°', '0°'))
        for s in range(0, 13, 2):
            g.append(f"<text x='{x(s):.1f}' y='{B + 26}' {FONT} font-size='14' fill='{c['muted']}' text-anchor='middle'>{s} s</text>")
        g.append(f"<text x='{L - 66}' y='{(T + B) / 2}' {FONT} font-size='14' fill='{c['muted']}' text-anchor='middle' transform='rotate(-90 {L - 66} {(T + B) / 2})'>L − R</text>")
        g.append(f"<path d='{path(truth)}' fill='none' stroke='{c['truth']}' stroke-opacity='0.28' stroke-width='8' stroke-linejoin='round' stroke-linecap='round'/>")
        g.append(f"<path d='{path(classic)}' fill='none' stroke='{c['classic']}' stroke-width='2' stroke-linejoin='round' stroke-linecap='round'/>")
        g.append(f"<path d='{path(robust)}' fill='none' stroke='{c['robust']}' stroke-width='2' stroke-linejoin='round' stroke-linecap='round'/>")
        # Summary, in text ink beside colour keys.
        summary = [(c['classic'], f'Classic: {wrong(classic)} of {len(t)} frames off by more than 4°, worst {worst(classic):.0f}°'),
                   (c['robust'], f'Rekkefly: {wrong(robust)} of {len(t)} frames off by more than 4°, worst {worst(robust):.1f}°')]
        for k, (color, text) in enumerate(summary):
            yy = B + 52 + k * 0
            xx = L - 50 + k * 560
            g.append(f"<circle cx='{xx + 5}' cy='{yy - 5}' r='5' fill='{color}'/>")
            g.append(f"<text x='{xx + 18}' y='{yy}' {FONT} font-size='15' fill='{c['ink']}'>{text}</text>")
        svg = f"<svg xmlns='http://www.w3.org/2000/svg' width='{W}' height='{H}' viewBox='0 0 {W} {H}'>{''.join(g)}</svg>"
        out = f'{prefix}-{name}.png'
        subprocess.run(['rsvg-convert', '-w', str(W * 2), '-o', out], input=svg.encode(), check=True)
        print(out)


if __name__ == '__main__':
    main()
