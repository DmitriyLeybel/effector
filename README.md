# Effector

Effector is a local-first Chrome control surface for agent harnesses. It pairs a
Manifest V3 extension with a small native bridge so MCP clients can inventory a
user's real Chrome windows, tab groups, and tabs without transferring ownership
of the browser to an automation framework. Controlled mutations are planned.

The extension, native bridge, and read-only MCP server are implemented. The
complete path has been validated with Windows Chrome and an MCP client running
in WSL2. Production packaging and broader platform validation remain.

## Project map

- [`extension/`](extension/) — Chrome extension prototype.
- [`AGENTS.md`](AGENTS.md) — repository-specific guidance for coding agents.
- [`docs/`](docs/) — architecture, decisions, research, roadmap, and progress.
- [`docs/architecture/overview.md`](docs/architecture/overview.md) — reviewed
  system architecture and component boundaries.
- [`docs/mcp-tools.md`](docs/mcp-tools.md) — current MCP tool contract.
- [`docs/troubleshooting.md`](docs/troubleshooting.md) — connection and setup
  diagnostics.
- [`docs/progress.md`](docs/progress.md) — current state and next work.
- [`docs/research/acp-harness-communication.md`](docs/research/acp-harness-communication.md)
  — recommended ACP side-panel and harness integration.

## Development setup

This is a development installation, not a production package. The installer
registers the exact executable path that runs it, and there is not yet an
uninstall command. Build on the same operating system as Chrome, or build the
matching target executable when Chrome runs on Windows and development happens
in WSL.

Prerequisites are a current stable Rust toolchain and Google Chrome. The
extension has no Node.js dependency or build step.

1. Build the native executable:

   ```bash
   cargo build --release
   ```

2. Open `chrome://extensions`, enable **Developer mode**, choose **Load
   unpacked**, and select this repository's `extension` directory.
3. Copy the extension ID shown by Chrome.
4. Register the native host, substituting that ID:

   ```bash
   target/release/effector install --extension-id <32-character-extension-id>
   ```

5. Reload the extension or restart Chrome. Keep Chrome running. The popup must
   report **Native broker connected** and show the MCP endpoint.
6. Verify the endpoint and extension from the shell:

   ```bash
   target/release/effector doctor
   ```

7. Configure the MCP client with Streamable HTTP, URL
   `http://127.0.0.1:37654/mcp`, and header
   `Authorization: Bearer <installation-token>`. Replace the placeholder with
   the token printed by `effector install`. Configuration syntax differs by
   client; keep the generated token private.
8. Start or restart the MCP client after Chrome and the native broker are
   connected. Clients that discover tools only during startup must be restarted
   after configuration, credential, networking, or installation changes.

The initial MCP tools are `browser.list` and `tabs.list`. There is no harness
selector: every Streamable HTTP client uses the same broker. Chrome owns the
broker process, so the endpoint is available only while the extension's Native
Messaging port is connected. See the [MCP tool reference](docs/mcp-tools.md) for
parameters, filtering, and pagination.

### Windows Chrome with an MCP client in WSL2

Windows and WSL2 have separate loopback networks under WSL's default NAT mode.
If Chrome and Effector run on Windows while the MCP client runs in WSL2, enable
mirrored networking so WSL's `127.0.0.1` reaches the Windows loopback broker.
Mirrored networking requires Windows 11 22H2 or later and a current WSL release.

Use a Windows `effector.exe` for Windows Chrome. Put it at a stable Windows path
and run `effector.exe install` in Windows PowerShell. Running the Linux
`effector install` command inside WSL registers a Linux host, not a host that
Windows Chrome can launch.

1. Create `%UserProfile%\.wslconfig` on Windows:

   ```ini
   [wsl2]
   networkingMode=mirrored
   ```

2. Apply the change from Windows PowerShell. Run `wsl --update` first if
   mirrored mode is unsupported. `wsl --shutdown` terminates all running WSL
   sessions:

   ```powershell
   wsl --shutdown
   ```

3. Restart WSL and verify the mode:

   ```bash
   wslinfo --networking-mode
   ```

   The command must print `mirrored`.
4. Start Windows Chrome and confirm the Effector popup reports **Native broker
   connected**.
5. Start or restart the MCP client in WSL.

Do not expose Effector on the Windows host or NAT gateway address. Effector
rejects non-loopback bind addresses by design.

### Windows build from WSL

Install the Rust Windows GNU target and a compatible MinGW-w64 or LLVM-MinGW
toolchain. A full Windows build needs the cross-linker on `PATH`; `cargo check`
alone does not exercise the linker:

```bash
rustup target add x86_64-pc-windows-gnu
PATH="<llvm-mingw-or-mingw-w64>/bin:$PATH" \
  cargo build --release --target x86_64-pc-windows-gnu
```

The resulting `target/x86_64-pc-windows-gnu/release/effector.exe` must be copied
to a stable Windows path before running `effector.exe install` in PowerShell.

## Remove a development installation

There is no automated uninstaller yet. Disable or remove the extension first,
then remove the Native Messaging registration for the operating system:

- Linux: `~/.config/google-chrome/NativeMessagingHosts/com.effector.browser.json`
- macOS: `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.effector.browser.json`
- Windows: registry key
  `HKCU\Software\Google\Chrome\NativeMessagingHosts\com.effector.browser`

The Windows manifest and persistent MCP token are stored under the user's
Effector configuration directory. Remove that directory only when the token and
local installation identity should also be deleted. Reload Chrome after
removing registration.

## Run the popup inventory directly

1. Open Effector from Chrome's Extensions menu.
2. Select **Read Chrome state**.

The current MCP and popup tools read metadata only. They do not activate,
reload, move, group, discard, or close tabs. Reports contain tab titles and
URLs; do not publish them without review.

## Troubleshooting

Start with [`docs/troubleshooting.md`](docs/troubleshooting.md). The shortest
healthy-path check is: Chrome is running, the popup reports **Native broker
connected**, `effector doctor` succeeds, and the MCP client was started after
those conditions became true.

## Scope

Effector targets Google Chrome and its documented extension APIs. Browser-
specific private APIs are outside the current scope.

## Project policies

- [Privacy](PRIVACY.md)
- [Security](SECURITY.md)
- [Contributing](CONTRIBUTING.md)
- [MIT License](LICENSE)
