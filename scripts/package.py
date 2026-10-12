#!/usr/bin/env python3
"""Package a native release: Mac app/DMG, Ubuntu deb/tarball, Windows setup/ZIP."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
VERSION = tomllib.loads((ROOT / 'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
DOCS = ['README.md', 'INSTALL.txt', 'VALIDATION.md', 'LICENSE']


def common(destination, notices):
    destination.mkdir(parents=True, exist_ok=True)
    for name in DOCS:
        shutil.copy2(ROOT / name, destination / name)
    shutil.copytree(ROOT / 'configs', destination / 'configs', ignore=shutil.ignore_patterns('*.png', '*.jsonl', 'recordings'))
    shutil.copytree(ROOT / 'examples', destination / 'examples', ignore=shutil.ignore_patterns('__pycache__'))
    (destination / 'THIRD_PARTY_NOTICES.txt').write_text(notices, encoding='utf-8')


def dependency_notices():
    cargo = os.environ.get('REKKE_CARGO', 'cargo')
    metadata = json.loads(subprocess.check_output([cargo, 'metadata', '--locked', '--offline', '--format-version', '1', '--manifest-path', str(ROOT / 'Cargo.toml')], text=True, encoding='utf-8'))
    notices = ['Rekke third-party dependency notices\nFull Cargo dependency set, including target-specific dependencies.\n']
    for dependency in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if dependency['name'] == 'rekke':
            continue
        notices.append(f"\n{'=' * 72}\n{dependency['name']} {dependency['version']}\nLicense: {dependency.get('license')}\nRepository: {dependency.get('repository')}\n")
        directory = Path(dependency['manifest_path']).parent
        for path in sorted(directory.rglob('*')):
            if path.is_file() and any(t in path.name.lower() for t in ['license', 'licence', 'copying', 'notice', 'copyright']) and path.stat().st_size < 1_000_000:
                try:
                    notices.append(f"\n--- {path.relative_to(directory)} ---\n{path.read_text(encoding='utf-8')}\n")
                except UnicodeDecodeError:
                    pass
    return '\n'.join(notices)


def archive_zip(package, output):
    path = output / (package.name + '.zip')
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED, compresslevel=6) as zipped:
        for item in sorted(package.rglob('*')):
            if item.is_file():
                info = zipfile.ZipInfo.from_file(item, item.relative_to(package.parent))
                info.compress_type = zipfile.ZIP_DEFLATED
                info.external_attr = (stat.S_IFREG | (item.stat().st_mode & 0o777)) << 16
                zipped.writestr(info, item.read_bytes())
    print(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', required=True, choices=['macos-arm64', 'macos-x64', 'linux-x64', 'windows-x64'])
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f'Missing executable: {binary}')
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    notices = dependency_notices()
    with tempfile.TemporaryDirectory(prefix='rekke-package-') as tmp:
        stage = Path(tmp)
        package = stage / f'Rekke-{VERSION}-{args.platform}'
        package.mkdir()
        executable = 'rekke.exe' if args.platform.startswith('windows') else 'rekke'
        shutil.copy2(binary, package / executable)
        common(package, notices)
        shutil.copy2(ROOT / 'assets/rekke-256.png', package / 'rekke.png')
        if args.platform.startswith('macos'):
            app = package / 'Rekke.app'
            macos = app / 'Contents/MacOS'
            macos.mkdir(parents=True)
            shutil.copy2(binary, macos / 'rekke')
            resources = app / 'Contents/Resources'
            common(resources, notices)
            shutil.copy2(ROOT / 'assets/Rekke.icns', resources / 'Rekke.icns')
            with (app / 'Contents/Info.plist').open('wb') as stream:
                plistlib.dump({
                    'CFBundleName': 'Rekke', 'CFBundleDisplayName': 'Rekke',
                    'CFBundleIdentifier': 'io.github.pvnkmrksk.rekke', 'CFBundleExecutable': 'rekke',
                    'CFBundlePackageType': 'APPL', 'CFBundleIconFile': 'Rekke.icns',
                    'CFBundleVersion': VERSION, 'CFBundleShortVersionString': VERSION,
                    'LSMinimumSystemVersion': '12.0', 'NSHighResolutionCapable': True,
                    'NSCameraUsageDescription': 'Rekke uses your selected camera to track insect wings and body movement.',
                }, stream)
            for label, arguments in [('Demo', '--config configs/demo.toml'), ('Camera A', '--config configs/camera-a.toml'), ('Camera B', '--config configs/camera-b.toml'), ('All Rigs', '--rigs configs/rigs.toml')]:
                script = package / f'Launch {label}.command'
                script.write_text(f'#!/bin/sh\ncd "$(dirname "$0")" || exit 1\nexec ./Rekke.app/Contents/MacOS/rekke {arguments}\n')
                script.chmod(0o755)
            subprocess.run(['xattr', '-cr', str(app)], check=True)
            subprocess.run(['codesign', '--force', '--sign', '-', str(app)], check=True)
            subprocess.run(['xattr', '-cr', str(app)], check=True)
            subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
            import dmgbuild
            dmg = output / f'Rekke-{VERSION}-{args.platform}.dmg'
            dmgbuild.build_dmg(str(dmg), f'Rekke {VERSION}', settings={
                'files': [str(app), (str(ROOT / 'INSTALL.txt'), 'Install help.txt')],
                'symlinks': {'Applications': '/Applications'}, 'format': 'UDZO', 'filesystem': 'HFS+',
                'background': str(ROOT / 'assets/dmg-background.tiff'), 'icon': str(ROOT / 'assets/Rekke.icns'),
                'window_rect': ((150, 150), (640, 420)), 'default_view': 'icon-view',
                'icon_size': 96, 'text_size': 13, 'label_pos': 'bottom', 'arrange_by': None,
                'icon_locations': {'Rekke.app': (180, 200), 'Applications': (460, 200), 'Install help.txt': (560, 340)},
                'hide_extensions': ['Rekke.app', 'Install help.txt'],
                'show_status_bar': False, 'show_tab_view': False, 'show_toolbar': False,
                'show_pathbar': False, 'show_sidebar': False,
            })
            subprocess.run(['hdiutil', 'verify', str(dmg)], check=True)
            print(dmg)
        elif args.platform.startswith('windows'):
            shutil.copy2(ROOT / 'assets/rekke.ico', package / 'rekke.ico')
            defaults = package / 'defaults'
            shutil.copytree(package / 'configs', defaults / 'configs')
            for label, arguments in [('Demo', '--config configs/demo.toml'), ('Camera A', '--config configs/camera-a.toml'), ('Camera B', '--config configs/camera-b.toml'), ('All Rigs', '--rigs configs/rigs.toml')]:
                (package / f'Launch {label}.cmd').write_text(f'@echo off\r\ncd /d "%~dp0"\r\nrekke.exe {arguments}\r\n', newline='')
            setup = output / f'Rekke-{VERSION}-windows-x64-setup.exe'
            command = os.environ.get('REKKE_MAKENSIS', 'makensis')
            subprocess.run([command, f'/DVERSION={VERSION}', f'/DPACKAGE={package}', f'/DOUTPUT={setup}', str(ROOT / 'scripts/windows-installer.nsi')], check=True)
            print(setup)
        else:
            for label, arguments in [('demo', '--config configs/demo.toml'), ('all-rigs', '--rigs configs/rigs.toml')]:
                script = package / f'launch-{label}.sh'
                script.write_text(f'#!/bin/sh\ncd "$(dirname "$0")" || exit 1\nexec ./rekke {arguments}\n')
                script.chmod(0o755)
            tarball = output / f'Rekke-{VERSION}-linux-x64.tar.gz'
            with tarfile.open(tarball, 'w:gz') as tar:
                tar.add(package, arcname=package.name)
            print(tarball)
            deb = stage / 'debian'
            (deb / 'DEBIAN').mkdir(parents=True)
            (deb / 'usr/bin').mkdir(parents=True)
            shutil.copy2(binary, deb / 'usr/bin/rekke')
            common(deb / 'usr/share/rekke', notices)
            icons = deb / 'usr/share/icons/hicolor/256x256/apps'
            icons.mkdir(parents=True)
            shutil.copy2(ROOT / 'assets/rekke-256.png', icons / 'rekke.png')
            desktop = deb / 'usr/share/applications'
            desktop.mkdir(parents=True)
            (desktop / 'rekke.desktop').write_text('[Desktop Entry]\nType=Application\nName=Rekke\nComment=Insect wing tracking\nExec=rekke\nIcon=rekke\nTerminal=false\nCategories=Science;Education;\nStartupWMClass=rekke\n')
            (deb / 'DEBIAN/control').write_text(f'Package: rekke\nVersion: {VERSION}\nSection: science\nPriority: optional\nArchitecture: amd64\nMaintainer: pvnkmrksk <pvnkmrksk@users.noreply.github.com>\nDepends: libc6 (>= 2.35), libgcc-s1, libv4l-0, libx11-6, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libegl1, libgl1, libxcursor1, libxi6, libxrandr2\nHomepage: https://github.com/pvnkmrksk/rekke\nDescription: Native Rust insect wing tracking with ZMQ output\n Directly editable tracking sectors, saved backgrounds, independent rigs\n and lossless video recording.\n')
            destination = output / f'rekke_{VERSION}_amd64.deb'
            subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(deb), str(destination)], check=True)
            print(destination)
        archive_zip(package, output)


if __name__ == '__main__':
    main()
