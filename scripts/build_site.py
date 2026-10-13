#!/usr/bin/env python3
"""Build the Rekkefly website: the landing page (site/) and the book (book/).

    python3 scripts/build_site.py _site
    python3 -m http.server -d _site 8000

The landing page is copied as it is, the book is built with mdBook into
docs/, a sitemap is written, and every link and image in the result is
checked: a missing file or a missing #anchor fails the build.
"""
import argparse
import html.parser
import os
import shutil
import subprocess
import sys
import urllib.parse
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_URL = 'https://pvnkmrksk.github.io/rekkefly/'


class Links(html.parser.HTMLParser):
    """Collects ids and the URLs a page refers to."""

    def __init__(self):
        super().__init__()
        self.ids = set()
        self.urls = []

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        for key in ('id', 'name'):
            if attrs.get(key) and (key == 'id' or tag == 'a'):
                self.ids.add(attrs[key])
        for key in ('href', 'src', 'poster'):
            if attrs.get(key) and not (tag == 'link' and attrs.get('rel') in ('preconnect', 'dns-prefetch')):
                self.urls.append(attrs[key])
        if attrs.get('srcset'):
            self.urls += [part.strip().split(' ')[0] for part in attrs['srcset'].split(',')]


def check(out: Path, url_root: str) -> tuple:
    """Broken links and missing anchors, and every file the pages use."""
    base_path = urllib.parse.urlsplit(url_root).path
    pages = {}
    for page in out.rglob('*.html'):
        parser = Links()
        parser.feed(page.read_text(encoding='utf-8'))
        pages[page.resolve()] = parser
    errors, used = [], set()
    for page, parser in pages.items():
        for url in parser.urls:
            if url.startswith(url_root):
                url = base_path + url[len(url_root):]
            parts = urllib.parse.urlsplit(url)
            if parts.scheme or parts.netloc or url.startswith(('mailto:', 'data:', 'javascript:')):
                continue
            path = urllib.parse.unquote(parts.path)
            if path.startswith('/'):
                if not path.startswith(base_path):
                    errors.append(f'{page.relative_to(out)}: {url} is outside the site')
                    continue
                target = out / path[len(base_path):]
            elif path:
                target = page.parent / path
            else:
                target = page
            if target.is_dir() or path.endswith('/'):
                target = target / 'index.html'
            target = target.resolve()
            used.add(target)
            if not target.exists():
                errors.append(f'{page.relative_to(out)}: broken link {url}')
            elif parts.fragment and target in pages and parts.fragment not in pages[target].ids:
                errors.append(f'{page.relative_to(out)}: missing anchor {url}')
    return errors, used


def prune(out: Path, used: set, folders: list) -> int:
    """Remove images and media that no page uses (the README's GIF, for one)."""
    removed = 0
    for folder in folders:
        for path in list((out / folder).rglob('*')):
            if path.is_file() and path.resolve() not in used:
                path.unlink()
                removed += 1
    return removed


def sitemap(out: Path, url: str):
    entries = []
    for page in sorted(out.rglob('*.html')):
        relative = page.relative_to(out).as_posix()
        if relative in ('404.html', 'docs/404.html', 'docs/print.html', 'docs/toc.html'):
            continue
        if relative.endswith('index.html'):
            relative = relative[: -len('index.html')]
        entries.append(f'  <url><loc>{url}{relative}</loc></url>')
    (out / 'sitemap.xml').write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        + '\n'.join(entries) + '\n</urlset>\n', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('out', type=Path, help='output folder (replaced)')
    parser.add_argument('--url', default=DEFAULT_URL, help=f'published address (default {DEFAULT_URL})')
    parser.add_argument('--mdbook', default=os.environ.get('MDBOOK', 'mdbook'), help='mdBook program')
    args = parser.parse_args()
    url = args.url if args.url.endswith('/') else args.url + '/'
    base_path = urllib.parse.urlsplit(url).path
    out = args.out.resolve()
    if out == ROOT or ROOT.is_relative_to(out):
        parser.error('refusing to replace the repository')
    if out.exists():
        shutil.rmtree(out)
    shutil.copytree(ROOT / 'site', out)
    env = dict(os.environ, MDBOOK_OUTPUT__HTML__SITE_URL=base_path + 'docs/')
    subprocess.run([args.mdbook, 'build', str(ROOT / 'book'), '--dest-dir', str(out / 'docs')], check=True, env=env)
    sitemap(out, url)
    (out / 'robots.txt').write_text(f'User-agent: *\nAllow: /\nSitemap: {url}sitemap.xml\n', encoding='utf-8')
    (out / '.nojekyll').write_text('', encoding='utf-8')
    errors, used = check(out, url)
    # Styles and scripts refer to fonts and images too; keep everything outside
    # the copied image and media folders, and prune only those.
    prune(out, used, ['docs/images', 'docs/media'])
    for error in errors:
        print(error, file=sys.stderr)
    if errors:
        sys.exit(f'{len(errors)} broken links')
    files = sum(1 for p in out.rglob('*') if p.is_file())
    size = sum(p.stat().st_size for p in out.rglob('*') if p.is_file())
    print(f'Built {out} ({files} files, {size / 1e6:.1f} MB) for {url}')


if __name__ == '__main__':
    main()
