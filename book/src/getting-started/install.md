# Install

Rekkefly is one program with no dependencies to install. Download the file for
your computer from the [latest release](https://github.com/pvnkmrksk/rekkefly/releases/latest):

| Computer | Download | Install |
| --- | --- | --- |
| Mac, Apple silicon (M1 and later) | `Rekkefly-<version>-macos-arm64.dmg` | Open it and drag **Rekkefly** onto **Applications** |
| Mac, Intel | `Rekkefly-<version>-macos-x64.dmg` | Open it and drag **Rekkefly** onto **Applications** |
| Windows 10 or 11, 64-bit | `Rekkefly-<version>-windows-x64-setup.exe` | Run it. No administrator rights needed |
| Ubuntu 22.04 or newer, x86-64 | `rekkefly_<version>_amd64.deb` | `sudo apt install ./rekkefly_<version>_amd64.deb` |
| Any of these, without installing | `Rekkefly-<version>-<system>.zip` or `-linux-x64.tar.gz` | Extract the folder and run `rekkefly` inside it |

Not sure which Mac you have? Choose **Apple menu → About This Mac**: a "Chip"
line (Apple M1, M2, …) means Apple silicon; a "Processor" line (Intel) means Intel.

Every package includes the [Bogong moth example](bogong-moth.md), the example
configs and the example receivers in Python.

## Mac

1. Open the DMG and drag **Rekkefly** onto the **Applications** shortcut.
2. Eject the disk image and open Rekkefly from **Applications**.
3. Allow camera access when macOS asks.

**If macOS says Rekkefly "cannot be opened".** Rekkefly is free software
without a paid Apple signature, so the first launch can be blocked. If you
trust the download, run once in Terminal:

```sh
xattr -dr com.apple.quarantine "/Applications/Rekkefly.app"
```

This clears the download flag on Rekkefly only; it does not turn off
Gatekeeper. Alternatively, try to open Rekkefly once, then choose **System
Settings → Privacy & Security → Open Anyway**
([Apple's explanation](https://support.apple.com/en-au/102445)).

**If the camera stays black,** check **System Settings → Privacy & Security →
Camera** and turn Rekkefly on.

From a terminal, the program inside the app is
`/Applications/Rekkefly.app/Contents/MacOS/rekkefly`. Every command in this
book works with that path in place of `rekkefly`. To type just `rekkefly`, add
a link once:

```sh
sudo mkdir -p /usr/local/bin
sudo ln -sf /Applications/Rekkefly.app/Contents/MacOS/rekkefly /usr/local/bin/rekkefly
```

## Windows

Run `Rekkefly-<version>-windows-x64-setup.exe`. It installs for your user account
under `%LOCALAPPDATA%\Programs\Rekkefly`, with Start-menu and desktop shortcuts
and a **Rekkefly - start all rigs** shortcut. The build is not code-signed, so
SmartScreen may ask you to confirm: choose **More info → Run anyway**.

You can drag a video file onto the Rekkefly shortcut to open it. Uninstall from
**Settings → Apps**; your configs in `%APPDATA%\Rekkefly\configs` are kept.

## Ubuntu and other Linux

```sh
sudo apt install ./rekkefly_<version>_amd64.deb
```

This puts `rekkefly` on your `PATH` and adds Rekkefly to the app menu, with
**Start all rigs** and **Open the Bogong moth example** actions (right-click the
icon). Builds target Ubuntu 22.04 and newer on x86-64; other distributions can
use the tarball.

To use a camera, your user must be in the `video` group:

```sh
sudo usermod -aG video $USER     # then log out and back in
```

## Portable folders

The ZIP files and the Linux tarball contain `rekkefly` (or `rekkefly.exe`),
`configs/`, `samples/`, `examples/` and launchers you can double-click:

| Launcher | Starts |
| --- | --- |
| **Launch Demo** | A simulated fly, no camera needed |
| **Launch Bogong Moth Example** | The real moth video with its markers |
| **Launch Camera A / B** | The first or second camera, with its own ports |
| **Launch All Rigs** | Every rig in `configs/rigs.toml` |

Launchers are `.command` files on a Mac, `.cmd` on Windows and `.sh` on Linux. A
portable folder reads and saves its configs in its own `configs/` folder, so you
can keep several independent copies, or carry one on a USB stick.

## Check the download

Each release has a `SHA256SUMS.txt`. To check a file:

```sh
shasum -a 256 Rekkefly-<version>-macos-arm64.dmg          # Mac
sha256sum rekkefly_<version>_amd64.deb                    # Linux
certutil -hashfile Rekkefly-<version>-windows-x64-setup.exe SHA256   # Windows
```

The result must match the line for that file in `SHA256SUMS.txt`.

## Upgrading

Install the new version over the old one. Configs, backgrounds and recordings
are kept. Rekkefly 0.6 replaced Rekke 0.5: on first launch it copies your Rekke
settings, markers and backgrounds into the Rekkefly folders. On Windows the two
install side by side; remove Rekke in **Settings → Apps**. On Ubuntu the
`rekkefly` package replaces `rekke`.

Where your settings live is listed in [Files and folders](../reference/files.md).

## Optional: FFmpeg

AVI files play without any extra software. To open MP4, MOV, M4V and other
formats, or network streams, install [FFmpeg](https://ffmpeg.org/download.html)
and make sure `ffmpeg` is on your `PATH`, or set `REKKEFLY_FFMPEG` to its full
path.

## Build it yourself

See [Building from source](../development/building.md).
