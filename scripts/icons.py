#!/usr/bin/env python3
"""Rasterize assets/rekkefly.svg into every app icon format.

Requires rsvg-convert (librsvg) and Pillow:
    sudo apt install librsvg2-bin && python3 -m pip install pillow   # Ubuntu
    brew install librsvg && python3 -m pip install pillow            # macOS
Writes rekkefly.png (1024), rekkefly-256.png, rekkefly-64.png, rekkefly.ico (Windows)
and Rekkefly.icns (macOS) beside the SVG.
"""
import io
import struct
import subprocess
from pathlib import Path

from PIL import Image

ASSETS = Path(__file__).resolve().parent.parent / 'assets'
SVG = ASSETS / 'rekkefly.svg'


def render(size):
    png = subprocess.run(['rsvg-convert', '-w', str(size), '-h', str(size), str(SVG)],
                         check=True, capture_output=True).stdout
    return Image.open(io.BytesIO(png)).convert('RGBA')


def png_bytes(image):
    out = io.BytesIO()
    image.save(out, format='PNG', optimize=True)
    return out.getvalue()


def main():
    sizes = {s: render(s) for s in [16, 24, 32, 48, 64, 128, 256, 512, 1024]}
    sizes[1024].save(ASSETS / 'rekkefly.png', optimize=True)
    sizes[256].save(ASSETS / 'rekkefly-256.png', optimize=True)
    sizes[64].save(ASSETS / 'rekkefly-64.png', optimize=True)
    # Uncompressed entries load in every Windows resource compiler.
    sizes[256].save(ASSETS / 'rekkefly.ico', sizes=[(s, s) for s in [16, 24, 32, 48, 64, 128, 256]],
                    bitmap_format='bmp')
    # ICNS with PNG payloads: 1× and 2× (Retina) entries from 16 to 512 points.
    types = [('icp4', 16), ('icp5', 32), ('icp6', 64), ('ic07', 128), ('ic08', 256),
             ('ic09', 512), ('ic10', 1024), ('ic11', 32), ('ic12', 64), ('ic13', 256), ('ic14', 512)]
    body = b''.join(kind.encode() + struct.pack('>I', 8 + len(data)) + data
                    for kind, data in ((k, png_bytes(sizes[s])) for k, s in types))
    (ASSETS / 'Rekkefly.icns').write_bytes(b'icns' + struct.pack('>I', 8 + len(body)) + body)
    print('Wrote icons to', ASSETS)


if __name__ == '__main__':
    main()
