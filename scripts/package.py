#!/usr/bin/env python3
"""Package a native release: Mac app/DMG, Ubuntu deb/tarball, Windows setup/ZIP."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import shutil
import stat
import struct
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
    shutil.copytree(ROOT / 'configs', destination / 'configs', ignore=shutil.ignore_patterns('*.png', '*.jsonl', 'recordings', '.*.lock'))
    shutil.copytree(ROOT / 'examples', destination / 'examples', ignore=shutil.ignore_patterns('__pycache__'))
    # The sample plays when no camera is connected, and for first-time users.
    shutil.copytree(ROOT / 'samples', destination / 'samples')
    (destination / 'THIRD_PARTY_NOTICES.txt').write_text(notices, encoding='utf-8')


def dependency_notices():
    cargo = os.environ.get('REKKEFLY_CARGO', 'cargo')
    metadata = json.loads(subprocess.check_output([cargo, 'metadata', '--locked', '--offline', '--format-version', '1', '--manifest-path', str(ROOT / 'Cargo.toml')], text=True, encoding='utf-8'))
    notices = ['Rekkefly third-party dependency notices\nFull Cargo dependency set, including target-specific dependencies.\n']
    for dependency in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if dependency['name'] == 'rekkefly':
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


def verify_windows_runtime(binary):
    """Reject a release that needs a separately installed MSVC runtime DLL."""
    data = binary.read_bytes()
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    if data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('Windows release is not a PE executable')
    count = struct.unpack_from('<H', data, pe + 6)[0]
    optional_size = struct.unpack_from('<H', data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from('<H', data, optional)[0]
    if magic != 0x20b:
        raise ValueError('Expected a 64-bit Windows executable')
    imports_rva = struct.unpack_from('<I', data, optional + 112 + 8)[0]
    sections = [struct.unpack_from('<IIII', data, optional + optional_size + i * 40 + 8)
                for i in range(count)]

    def offset(rva):
        for virtual_size, address, raw_size, raw in sections:
            if address <= rva < address + max(virtual_size, raw_size):
                return raw + rva - address
        raise ValueError(f'Invalid PE address: {rva}')

    cursor = offset(imports_rva)
    names = []
    while True:
        entry = struct.unpack_from('<IIIII', data, cursor)
        if not any(entry):
            break
        name = offset(entry[3])
        names.append(data[name:data.index(b'\0', name)].decode('ascii'))
        cursor += 20
    external = [name for name in names if name.upper().startswith(('VCRUNTIME', 'MSVCP', 'CONCRT'))]
    if external:
        raise ValueError(f'Windows release requires {external}; rebuild with +crt-static and an explicit target')
    print('Windows runtime imports verified: ' + ', '.join(names))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--platform', required=True, choices=['macos-arm64', 'macos-x64', 'linux-x64', 'windows-x64'])
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--out', type=Path, default=ROOT / 'dist')
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f'Missing executable: {binary}')
    if args.platform.startswith('windows'):
        verify_windows_runtime(binary)
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=True)
    notices = dependency_notices()
    with tempfile.TemporaryDirectory(prefix='rekkefly-package-') as tmp:
        stage = Path(tmp)
        package = stage / f'Rekkefly-{VERSION}-{args.platform}'
        package.mkdir()
        executable = 'rekkefly.exe' if args.platform.startswith('windows') else 'rekkefly'
        shutil.copy2(binary, package / executable)
        common(package, notices)
        shutil.copy2(ROOT / 'assets/rekkefly-256.png', package / 'rekkefly.png')
        if args.platform.startswith('macos'):
            app = package / 'Rekkefly.app'
            macos = app / 'Contents/MacOS'
            macos.mkdir(parents=True)
            shutil.copy2(binary, macos / 'rekkefly')
            resources = app / 'Contents/Resources'
            common(resources, notices)
            shutil.copy2(ROOT / 'assets/Rekkefly.icns', resources / 'Rekkefly.icns')
            with (app / 'Contents/Info.plist').open('wb') as stream:
                plistlib.dump({
                    'CFBundleName': 'Rekkefly', 'CFBundleDisplayName': 'Rekkefly',
                    'CFBundleIdentifier': 'io.github.pvnkmrksk.rekkefly', 'CFBundleExecutable': 'rekkefly',
                    'CFBundlePackageType': 'APPL', 'CFBundleIconFile': 'Rekkefly.icns',
                    'CFBundleVersion': VERSION, 'CFBundleShortVersionString': VERSION,
                    'LSMinimumSystemVersion': '12.0', 'NSHighResolutionCapable': True,
                    'NSCameraUsageDescription': 'Rekkefly uses your selected camera to track insect wings and body movement.',
                }, stream)
            for label, arguments in [('Demo', '--config configs/demo.toml'), ('Bogong Moth Example', '--config configs/bogong-moth.toml'), ('Camera A', '--config configs/camera-a.toml'), ('Camera B', '--config configs/camera-b.toml'), ('All Rigs', '--rigs configs/rigs.toml')]:
                script = package / f'Launch {label}.command'
                script.write_text(f'#!/bin/sh\ncd "$(dirname "$0")" || exit 1\nexec ./Rekkefly.app/Contents/MacOS/rekkefly {arguments}\n')
                script.chmod(0o755)
            subprocess.run(['xattr', '-cr', str(app)], check=True)
            subprocess.run(['codesign', '--force', '--sign', '-', str(app)], check=True)
            subprocess.run(['xattr', '-cr', str(app)], check=True)
            subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
            import dmgbuild
            dmg = output / f'Rekkefly-{VERSION}-{args.platform}.dmg'
            dmgbuild.build_dmg(str(dmg), f'Rekkefly {VERSION}', settings={
                'files': [str(app), (str(ROOT / 'INSTALL.txt'), 'Install help.txt')],
                'symlinks': {'Applications': '/Applications'}, 'format': 'UDZO', 'filesystem': 'HFS+',
                'background': str(ROOT / 'assets/dmg-background.tiff'), 'icon': str(ROOT / 'assets/Rekkefly.icns'),
                'window_rect': ((150, 150), (640, 420)), 'default_view': 'icon-view',
                'icon_size': 96, 'text_size': 13, 'label_pos': 'bottom', 'arrange_by': None,
                'icon_locations': {'Rekkefly.app': (180, 200), 'Applications': (460, 200), 'Install help.txt': (560, 300)},
                'hide_extensions': ['Install help.txt'],
                'show_status_bar': False, 'show_tab_view': False, 'show_toolbar': False,
                'show_pathbar': False, 'show_sidebar': False,
            })
            subprocess.run(['hdiutil', 'verify', str(dmg)], check=True)
            # Verify the copied bundle, not only the staging directory: Finder
            # attributes added by a DMG tool can invalidate an app signature.
            with tempfile.TemporaryDirectory(prefix='rekkefly-dmg-check-') as mount:
                subprocess.run(['hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', mount, str(dmg)], check=True)
                try:
                    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(Path(mount) / 'Rekkefly.app')], check=True)
                finally:
                    subprocess.run(['hdiutil', 'detach', mount], check=True)
            print(dmg)
        elif args.platform.startswith('windows'):
            shutil.copy2(ROOT / 'assets/rekkefly.ico', package / 'rekkefly.ico')
            defaults = package / 'defaults'
            shutil.copytree(package / 'configs', defaults / 'configs')
            for label, arguments in [('Demo', '--config configs/demo.toml'), ('Bogong Moth Example', '--config configs/bogong-moth.toml'), ('Camera A', '--config configs/camera-a.toml'), ('Camera B', '--config configs/camera-b.toml'), ('All Rigs', '--rigs configs/rigs.toml')]:
                (package / f'Launch {label}.cmd').write_text(f'@echo off\r\ncd /d "%~dp0"\r\n{executable} {arguments}\r\n', newline='')
            setup = output / f'Rekkefly-{VERSION}-windows-x64-setup.exe'
            command = os.environ.get('REKKEFLY_MAKENSIS', 'makensis')
            subprocess.run([command, f'/DVERSION={VERSION}', f'/DPACKAGE={package}', f'/DOUTPUT={setup}', str(ROOT / 'scripts/windows-installer.nsi')], check=True)
            print(setup)
        else:
            for label, arguments in [('demo', '--config configs/demo.toml'), ('bogong-moth-example', '--config configs/bogong-moth.toml'), ('all-rigs', '--rigs configs/rigs.toml')]:
                script = package / f'launch-{label}.sh'
                script.write_text(f'#!/bin/sh\ncd "$(dirname "$0")" || exit 1\nexec ./{executable} {arguments}\n')
                script.chmod(0o755)
            tarball = output / f'Rekkefly-{VERSION}-linux-x64.tar.gz'
            with tarfile.open(tarball, 'w:gz') as tar:
                tar.add(package, arcname=package.name)
            print(tarball)
            deb = stage / 'debian'
            (deb / 'DEBIAN').mkdir(parents=True)
            (deb / 'usr/bin').mkdir(parents=True)
            shutil.copy2(binary, deb / 'usr/bin/rekkefly')
            common(deb / 'usr/share/rekkefly', notices)
            icons = deb / 'usr/share/icons/hicolor/256x256/apps'
            icons.mkdir(parents=True)
            shutil.copy2(ROOT / 'assets/rekkefly-256.png', icons / 'rekkefly.png')
            desktop = deb / 'usr/share/applications'
            desktop.mkdir(parents=True)
            (desktop / 'rekkefly.desktop').write_text('[Desktop Entry]\nType=Application\nName=Rekkefly\nComment=The new Kinefly: wing tracking for tethered insects\nKeywords=kinefly;insect;wing;tracking;drosophila;flight;\nExec=rekkefly %f\nIcon=rekkefly\nTerminal=false\nCategories=Science;Education;\nMimeType=video/x-msvideo;video/avi;\nStartupWMClass=rekkefly\nActions=rigs;sample;\n\n[Desktop Action rigs]\nName=Start all rigs (rigs.toml)\nExec=rekkefly --rigs\n\n[Desktop Action sample]\nName=Open the Bogong moth example\nExec=rekkefly /usr/share/rekkefly/samples/bogong-moth.avi --loop\n')
            (deb / 'DEBIAN/control').write_text(f'Package: rekkefly\nVersion: {VERSION}\nSection: science\nPriority: optional\nArchitecture: amd64\nMaintainer: pvnkmrksk <pvnkmrksk@users.noreply.github.com>\nDepends: libc6 (>= 2.35), libgcc-s1, libv4l-0, libx11-6, libxcb1, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libegl1, libgl1, libxcursor1, libxi6, libxrandr2\nReplaces: rekke\nConflicts: rekke\nHomepage: https://pvnkmrksk.github.io/rekkefly/\nDescription: The new Kinefly: real-time wing tracking for tethered insects\n Native successor to Kinefly and Kinefly Docker. Tracks wing stroke\n envelopes from cameras or video files, ignores legs and antennae,\n publishes Kinefly-compatible ZMQ and runs many rigs from one command.\n')
            destination = output / f'rekkefly_{VERSION}_amd64.deb'
            subprocess.run(['dpkg-deb', '--build', '--root-owner-group', str(deb), str(destination)], check=True)
            print(destination)
        archive_zip(package, output)


if __name__ == '__main__':
    main()
