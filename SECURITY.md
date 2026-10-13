# Security policy

## Supported versions

Security fixes go into the latest release. Please update before reporting.

| Version | Supported |
| --- | --- |
| 0.6.x (Rekkefly) | Yes |
| 0.5.x (Rekke) and older | No |

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Report it
privately through GitHub instead: on the repository's
[Security tab](https://github.com/pvnkmrksk/rekkefly/security), choose
**Report a vulnerability**. Include what an attacker could do, the steps to
reproduce it, and the version and system you used.

You can expect an acknowledgement within a week. Once a fix is released,
the advisory is published with credit to you, unless you prefer otherwise.

## What Rekkefly exposes

Knowing what is reachable helps judge a report:

- **ZMQ publisher** (`output.zmq_bind`, default `tcp://*:9871`). It listens on
  every network interface so that other computers can subscribe, and it only
  publishes; it accepts no commands. Bind it to `tcp://127.0.0.1:9871` if no
  other computer needs it, and keep rigs on a trusted network.
- **UDP output** (`output.bind` → `output.target`). It sends measurements and
  never reads from the socket.
- **MCP server** (`rekkefly mcp`). It talks only over the standard input and
  output of the process that started it, never over the network. Its tools can
  write configs and files that the user running it can write. Give it to agents
  you trust as you would a terminal.
- **Files it reads**: TOML configs, AVI and other video files (other formats
  through FFmpeg if it is installed), and PNG backgrounds. Crashes or hangs
  caused by malformed files are bugs worth reporting.

The installers are not code-signed. Check downloads against `SHA256SUMS.txt`
from the same release.
