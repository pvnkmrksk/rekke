#!/usr/bin/env python3
"""Make the website's real-footage videos from the Bogong moth example.

    python3 scripts/web_media.py

Runs the ignored `web_media` test in tests/moth.rs, which tracks every frame of
samples/bogong-moth.avi and draws the tracking on the raw (never inverted)
frames, then puts the results in place:

  docs/media/moth-tracking.*      Rekkefly on the real footage (MP4 and WebM)
  docs/media/moth-antennae.*      painted antennae: classic (left), Rekkefly (right)
  docs/media/moth-*.jpg           their posters
  site/assets/moth-angles.json    the angles shown beside the videos

Needs Rust and FFmpeg with libx264 and libvpx-vp9 (or REKKEFLY_FFMPEG).
"""
import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FFMPEG = os.environ.get('REKKEFLY_FFMPEG', 'ffmpeg')


def poster(video: Path, frame: int, out: Path):
    subprocess.run([FFMPEG, '-hide_banner', '-loglevel', 'error', '-y', '-i', str(video),
                    '-vf', f'select=eq(n\\,{frame})', '-frames:v', '1', '-q:v', '3', str(out)], check=True)


def main():
    with tempfile.TemporaryDirectory(prefix='rekkefly-web-') as tmp:
        env = dict(os.environ, REKKEFLY_WEB_OUT=tmp)
        subprocess.run(['cargo', 'test', '--release', '--locked', '--test', 'moth', 'web_media', '--', '--ignored', '--exact'],
                       cwd=ROOT, env=env, check=True)
        tmp = Path(tmp)
        media = ROOT / 'docs/media'
        for name in ('moth-tracking', 'moth-antennae'):
            for kind in ('mp4', 'webm'):
                shutil.copyfile(tmp / f'{name}.{kind}', media / f'{name}.{kind}')
        angles = json.loads((tmp / 'moth-angles.json').read_text())
        (ROOT / 'site/assets/moth-angles.json').write_text(json.dumps(angles, separators=(',', ':')) + '\n')
    # Posters: an ordinary frame for the tracking video, and for the
    # comparison the first frame where the antennae pull the classic edge.
    poster(media / 'moth-tracking.mp4', 12, media / 'moth-tracking.jpg')
    painted, clean = angles['antennae'], angles['antennae']['clean']

    def moved(name):
        return [a is None or b is None or abs(a - b) > 5 for a, b in zip(painted[name], clean[name])]

    classic, robust = moved('classic'), moved('rekkefly')
    frame = next((k for k, (c, r) in enumerate(zip(classic, robust))
                  if c and not r and abs(painted['classic'][k] - clean['classic'][k]) > 40), 0)
    poster(media / 'moth-antennae.mp4', frame, media / 'moth-antennae.jpg')
    print(f'L − R moved > 5° by the antennae: classic {sum(classic)}, Rekkefly {sum(robust)} of {len(classic)} frames')
    for path in sorted(media.glob('moth-*')) + [ROOT / 'site/assets/moth-angles.json']:
        print(f'{path.relative_to(ROOT)}  {path.stat().st_size / 1e6:.2f} MB')
    print(f'Comparison poster: frame {frame}')


if __name__ == '__main__':
    main()
