# Effector

![Status: Developer Preview](https://img.shields.io/badge/status-developer_preview-6f42c1)
![Chrome](https://img.shields.io/badge/browser-Google_Chrome-4285F4)
[![License: MIT](https://img.shields.io/badge/license-MIT-2ea44f)](LICENSE)

**Give AI tools a live view of the Chrome session you are already using.**

Your browser is where the work already is. Effector gives local MCP clients a
structured view of your open windows, tab groups, and tabs, without launching a
second automation browser or taking over your current one.

> [!NOTE]
> Effector is a developer preview. The current release is read-only and requires
> a source build.

## Why Effector?

- **Keep your context.** Agents can understand the tabs and groups you already
  have open.
- **Stay in control.** Reads do not focus tabs, reload pages, or wake discarded
  tabs.
- **Run locally.** The bridge listens only on authenticated loopback HTTP.
- **Bring your own client.** Any MCP client that supports Streamable HTTP can
  connect.
- **Keep Chrome lightweight.** The extension has no dependencies or build step.

Unlike page automation tools, Effector focuses on browser-level context: what is
open, where it is organized, and which browser instance it belongs to.

## What Can It Do?

| Tool | Purpose |
| --- | --- |
| `browser.snapshot` | Read a stable hierarchical browser snapshot with opaque references and immutable pagination. |
| `browser.list` | Identify the connected Chrome instance and summarize its windows and tabs. |
| `tabs.list` | Browse live tab pages with legacy filters for window, group, active state, and discarded state. |

`browser.list` and `tabs.list` remain temporarily for migration. Results are
bounded and paginated, making them practical for large real-world sessions.

## Quick Start

You need [Google Chrome](https://www.google.com/chrome/) and a current stable
[Rust toolchain](https://rustup.rs/).

1. Build Effector from the repository root:

   ```bash
   cargo build --release
   ```

2. Open `chrome://extensions`, enable **Developer mode**, choose **Load
   unpacked**, and select the repository's `extension` directory.
3. Copy the extension ID shown by Chrome.
4. Register the native bridge:

   ```bash
   target/release/effector install --extension-id <extension-id>
   ```

5. Reload the extension. Its popup should report **Native broker connected**.
6. Confirm the connection:

   ```bash
   target/release/effector doctor
   ```

7. Add a Streamable HTTP MCP server to your client using the URL and bearer
   token printed by the installer.

> [!IMPORTANT]
> Build and install the executable for the operating system running Chrome. For
> Windows Chrome with an MCP client in WSL2, follow the
> [WSL2 connection guide](docs/troubleshooting.md#windows-chrome-and-wsl2-cannot-connect).

## How It Fits

```text
MCP client  <->  local Effector broker  <->  Chrome extension  <->  your Chrome
```

Chrome starts and owns the broker, so Effector is available only while Chrome
and the extension are connected. Browser metadata travels through a local,
authenticated connection to the MCP client you choose.

## Safety By Default

- Read-only tools cannot close, move, reload, or activate tabs.
- Discarded tabs remain discarded.
- Incognito access is disabled.
- The MCP endpoint is restricted to loopback and protected by an installation
  token.
- Chrome permissions are limited to tab metadata, tab groups, local storage,
  and Native Messaging.

See [Privacy](PRIVACY.md) and [Security](SECURITY.md) for the complete model.

## Project Status

The end-to-end path is working with Windows Chrome and an MCP client in WSL2.
Production packaging, broader platform validation, and carefully designed
browser actions are future work.

Follow the [current progress](docs/progress.md) and [roadmap](docs/roadmap.md)
for what is implemented versus planned.

## Learn More

- [MCP tool reference](docs/mcp-tools.md)
- [Installation and packaging](docs/architecture/installation-and-packaging.md)
- [Architecture overview](docs/architecture/overview.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Full documentation index](docs/README.md)

## Contributing

Issues and contributions are welcome. Start with
[CONTRIBUTING.md](CONTRIBUTING.md) for development checks and project
conventions.

Effector is available under the [MIT License](LICENSE).
