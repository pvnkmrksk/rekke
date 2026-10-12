Rekke (ರೆಕ್ಕೆ, Kannada for wing) is a native Rust insect-wing tracker inspired by Kinefly.

This release includes native webcam capture, directly draggable symmetric tracking sectors, persistent marker/background state, Kinefly Docker-compatible ZMQ PUB output, a multi-rig launcher, lossless grayscale video recording, an optional broad-shoulder wing filter, and a separate live plot with Pause, zoom, pan and hover values.

The Windows release now links the Visual C++ runtime statically so no separate runtime installation is required.

Download the installer for your computer:

- macOS Apple Silicon: `Rekke-0.5.1-macos-arm64.dmg`.
- macOS Intel: `Rekke-0.5.1-macos-x64.dmg`.
- Ubuntu 22.04 or later, x86-64: `rekke_0.5.1_amd64.deb`.
- Windows 10/11, x64: `Rekke-0.5.1-windows-x64-setup.exe`.
- Portable ZIPs and a Linux tarball are also available.

On Mac, drag **Rekke.app** onto **Applications**, then eject the image. If macOS blocks this unsigned/unnotarized release, the README and DMG's Install help include the scoped Terminal command:

```sh
xattr -dr com.apple.quarantine "/Applications/Rekke.app"
```

Use this only for a release whose origin you trust. Windows builds are unsigned and may show SmartScreen. User config folders are writable and survive upgrades/uninstall. Existing explicit Wingtrace TOMLs remain compatible; a previous Mac Application Support configuration and its baseline migrate automatically.

The spatial wing filter is experimental and needs real insect footage for biological validation. Video recording requires enough storage bandwidth; queue/disk failures are reported. Physical camera behavior on each OS depends on its driver. These builds report per-frame angles, not wingbeat frequency or stopped-flight classification.

See README for camera mappings, ZMQ fields, multiple rigs, recording and build instructions. Checksums are in `SHA256SUMS.txt`.
