# Files and folders

## Where configs live

| Installed as | Configs folder |
| --- | --- |
| Mac app | `~/Library/Application Support/Rekkefly/configs/` |
| Windows installer | `%APPDATA%\Rekkefly\configs\` |
| Ubuntu `.deb` | `~/.config/rekkefly/configs/` (or `$XDG_CONFIG_HOME/rekkefly/configs/`) |
| Portable folder | `configs/` inside the folder |
| A clone of the source | `configs/` in the clone |

Installed apps create the folder on first launch, with `camera-a.toml`,
`camera-b.toml`, `demo.toml`, `bogong-moth.toml`, `network.toml` and
`rigs.toml`. Settings from Rekke 0.5 (and earlier Wingtrace installs) are copied
over once. Uninstalling keeps the folder.

## How Rekkefly picks a config

Without `--config`, Rekkefly looks, in order, for:

1. In a Mac app: the user configs folder above.
2. In the current folder: `rekkefly.toml`, then `configs/camera-a.toml`.
3. Beside the program: `configs/camera-a.toml` (portable folders).
4. In an installed app: the user configs folder above.
5. Otherwise `rekkefly.toml` in the current folder.

If that config is already open in another window, the next free
`camera-*.toml` beside it is used, and a new one (`camera-c.toml`, …) is created
when all are open. The MCP server looks up config names in the same folder;
`rekkefly mcp --config-dir DIR` uses another.

## Beside each config

| File | Written by | Contents |
| --- | --- | --- |
| `camera-a.toml` | You, and the GUI's autosave | The rig's settings |
| `rig-a.background.png` | **Save BG** | The background image (`tracking.background_path`) |
| `.camera-a.toml.lock` | Rekkefly, while the config is open | An operating-system file lock that stops two windows or rigs using the same config. The system releases it when Rekkefly exits or crashes, so there is nothing to clean up |
| `recordings/<label>-<time>-<pid>/` | **Record** | One recording session ([contents](../guide/recording.md)) |
| `output.jsonl_path`, if set | The JSONL output | One measurement per line |

All paths inside a config are relative to the config file.

## Samples

| Location | Purpose |
| --- | --- |
| `samples/bogong-moth.avi` (in the app or portable folder) | The [Bogong moth example](../getting-started/bogong-moth.md) |
| `<user folder>/samples/sample.avi`, or `REKKEFLY_SAMPLE` | Your own example, played instead of the moth when no camera opens |

The user folder is the parent of the configs folder, for example
`~/Library/Application Support/Rekkefly/` on a Mac.

## Writes are safe

Configs are written atomically: the new contents go to a temporary file that
replaces the old one in a single step, so a crash or power loss can never leave
a half-written config. Comments and layout in the file are kept. Rekkefly never
overwrites a recording, a clip or a JSONL file that already exists.
