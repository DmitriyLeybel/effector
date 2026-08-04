# Installation and packaging

Status: implementation draft
Last updated: 2026-08-03

## Product goal

The user installs two artifacts:

1. The Effector Chrome extension.
2. One native Effector package for their operating system.

There is no manually managed daemon and no separate language runtime. Chrome
starts the native broker when the extension opens a Native Messaging port. MCP
clients connect to the broker's authenticated loopback HTTP endpoint.

## Chrome registration constraint

A Chrome extension cannot install, locate, or execute an arbitrary native
application. A native installer must place a host manifest in an
operating-system-specific location. The manifest contains the stable absolute
executable path and an `allowed_origins` entry for the extension's ID.

The extension calls `connectNative("com.effector.browser")`; Chrome resolves the
registered path, verifies the extension origin, launches Effector, and owns its
stdin/stdout pipes.

## Executable modes

| Invocation | Started by | Role |
| --- | --- | --- |
| `effector` with Chrome's extension-origin argument | Chrome | Native Messaging broker and MCP HTTP server |
| `effector native-host` | Developer/test tooling | Explicit broker mode |
| `effector install --extension-id ...` | User or installer | Register the current binary and create MCP credentials |
| `effector doctor` | User/support tooling | Connect over MCP HTTP and call `browser.snapshot(detail="counts")` |

There is no harness-launched `effector mcp` process.

## Runtime startup

```text
Chrome starts or the profile loads
  -> extension calls connectNative("com.effector.browser")
  -> Chrome launches the installed Effector executable
  -> broker binds http://127.0.0.1:37654/mcp
  -> extension and broker complete their Native Messaging handshake

Harness session starts
  -> harness connects to the configured MCP URL with its bearer token
  -> browser tool calls route directly through the broker to the extension
```

Start the harness only after Chrome is running and the extension popup reports
**Native broker connected**. Clients that discover MCP tools only during startup
must be restarted after Chrome becomes available or after MCP configuration,
credentials, networking, or installation changes.

The HTTP server binds only to loopback. The per-installation bearer token is a
64-character random value stored in the user's Effector state directory. On
Unix, the token file is created with mode `0600`. Host and Origin validation are
also enforced.

## Harness configuration

`effector install` prints the transport, endpoint, and generated credential:

```text
Transport: Streamable HTTP
URL: http://127.0.0.1:37654/mcp
Authorization: Bearer <installation-token>
```

Configuration syntax is owned by each harness. The server does not identify or
special-case harness products. The credential must not be published in logs or
support reports.

## Development installation

1. Build with `cargo build --release`.
2. Load `extension/` as an unpacked extension in `chrome://extensions`.
3. Copy the extension's 32-character ID.
4. Run `target/release/effector install --extension-id <ID>`.
5. Reload the extension or restart Chrome and confirm the popup reports
   **Native broker connected**.
6. Run `target/release/effector doctor` while Chrome is open.
7. Add the printed authenticated HTTP configuration to the harness.
8. Start or restart the harness.

The development installer points Chrome at the exact executable that invoked
`install`. Moving or deleting that executable breaks registration. A production
package must install to a stable path before writing the host manifest.

## Production packaging

Shared requirements:

- Stable Chrome Web Store extension ID.
- Signed release artifacts and checksums.
- Stable native executable path.
- User-level registration by default.
- Atomic upgrades that keep the native-host manifest path valid.
- Persistent MCP credential across upgrades, removed on uninstall if requested.
- Diagnostics that never reveal the token or browser inventory accidentally.

### Windows

Install the executable at a stable per-user path, write the host manifest, and
register its location under
`HKCU\Software\Google\Chrome\NativeMessagingHosts`. The current path has
been exercised from WSL against Windows Chrome but still needs packaged
installer validation.

The WSL development build was validated with LLVM-MinGW. Install a compatible
LLVM-MinGW or MinGW-w64 toolchain and add its `bin` directory to `PATH` for the
full link step:

```bash
rustup target add x86_64-pc-windows-gnu
PATH="<llvm-mingw-or-mingw-w64>/bin:$PATH" \
  cargo build --release --target x86_64-pc-windows-gnu
```

The Rust Windows target and linker toolchain are separate prerequisites.
`cargo check --target x86_64-pc-windows-gnu` can succeed even when the linker or
`dlltool` is unavailable. Copy the linked `effector.exe` to a stable Windows
path before registration; cleaning `target/` must not remove the registered
executable.

#### Windows Chrome with a WSL2 client

When Chrome and Effector run on Windows but the MCP client runs in WSL2, WSL's
default NAT mode isolates its loopback interface from Windows loopback. Enable
mirrored networking in `%UserProfile%\.wslconfig` on Windows. This requires
Windows 11 22H2 or later and a current WSL release.

Windows Chrome must use a Windows `effector.exe` installed from Windows
PowerShell at a stable Windows path. A Linux `effector install` invocation in
WSL writes Linux registration and cannot register a host for Windows Chrome.

```ini
[wsl2]
networkingMode=mirrored
```

Run `wsl --update` in Windows PowerShell first if mirrored mode is unsupported.
Apply the setting with `wsl --shutdown`, restart WSL, and verify that
`wslinfo --networking-mode` prints `mirrored`. Then start Windows Chrome,
confirm the popup connection, and start or restart the WSL MCP client.

Using the Windows host or NAT gateway address is not an alternative. The broker
deliberately rejects non-loopback bind addresses to preserve its local-only
security boundary.

If mirrored mode is active but port `37654` is unreachable, inspect Windows and
Hyper-V firewall policy for the WSL connection without exposing the port on a
public or LAN interface.

### macOS

Use a signed and notarized package. Write the per-user host manifest beneath
Chrome's `NativeMessagingHosts` directory. The current registration code has
not yet been validated on macOS.

### Linux

The per-user manifest is written beneath
`~/.config/google-chrome/NativeMessagingHosts`. Release archives should install
the executable to a stable per-user application path before registration.

## Development uninstall

The current CLI has no uninstall command. Disable or remove the extension, then
remove the registration written by `install`:

- Linux:
  `~/.config/google-chrome/NativeMessagingHosts/com.effector.browser.json`
- macOS:
  `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.effector.browser.json`
- Windows: registry key
  `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.effector.browser`

On Windows, the manifest is also stored in the Effector state directory. The
state directory contains the MCP token on every platform; remove it only when
the credential should be revoked. Reload or restart Chrome after unregistering
the host.

## Current limitations

- The fixed endpoint supports one active Chrome profile at a time.
- A client cannot connect or discover tools while Chrome and the broker are
  stopped.
- WSL2 clients connecting to a Windows broker require mirrored networking.
- Harness support for Streamable HTTP and authorization headers must be checked.
- Production installers and uninstallers remain to be built.
- Chrome restart and extension service-worker recovery need live-browser tests.
