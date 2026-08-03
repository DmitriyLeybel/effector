# Troubleshooting

Status: current
Last updated: 2026-08-03

## Healthy startup order

1. Start Chrome with the Effector extension installed and enabled.
2. Open the Effector popup and confirm **Native broker connected**.
3. Run `effector doctor` on the operating system where Chrome and Effector are
   installed.
4. Start or restart the MCP client.

Chrome owns the native broker process. When Chrome or the extension's Native
Messaging connection stops, `http://127.0.0.1:37654/mcp` stops with it. Effector
does not launch Chrome.

## `effector doctor`

`effector doctor` loads the default or `EFFECTOR_MCP_ADDRESS` endpoint and loads
or creates the persistent token, connects to MCP, and calls `browser.list`. A
successful result proves that the broker, credential, Native Messaging
connection, extension, and browser request path are working. Supply the same
address override to `doctor` if the broker uses one for testing.

Run the executable installed for the same operating-system user as Chrome. For
Windows Chrome with an MCP client in WSL, run `effector.exe doctor` in Windows
PowerShell. A separate Linux build can read different installation state and is
not an equivalent test of the Windows installation.

## Connection refused

Nothing is listening at the configured endpoint.

1. Confirm Chrome is running.
2. Confirm the extension is enabled.
3. Open the popup and check its connection status.
4. Reload the extension or restart Chrome.
5. Run `effector doctor` again.

If Chrome started before the native host was installed or replaced, reload the
extension or restart Chrome so it reconnects.

## Popup reports a native connection error

Open `chrome://extensions`, locate Effector, and inspect its errors. Confirm that
the native host was registered with the exact extension ID shown by Chrome:

```bash
effector install --extension-id <32-character-extension-id>
```

Development registration points to the exact executable that ran `install`.
Moving or deleting that executable breaks the registration; run `install` again
from its stable location.

## Unauthorized MCP response

The client token does not match the installation token used by the running
broker. Replace the client's complete `Authorization` value with the value
printed by `effector install`, then restart the client.

Do not paste the real token into issues, logs, screenshots, or repository files.

## MCP connects but tools are absent

Restart the MCP client after Chrome and the broker are connected. Clients that
perform MCP discovery only during startup must also be restarted after changes
to MCP configuration, the token, WSL networking, or the Effector installation.
Verify that the server entry uses Streamable HTTP,
`http://127.0.0.1:37654/mcp`, and the bearer header shown in the README.

## Windows Chrome and WSL2 cannot connect

Mirrored networking requires Windows 11 22H2 or later and a current WSL release.
Run `wsl --update` from Windows PowerShell if the setting is unsupported.

Run `wslinfo --networking-mode` in WSL. If it does not print `mirrored`, create
`%UserProfile%\.wslconfig` on Windows:

```ini
[wsl2]
networkingMode=mirrored
```

Then run `wsl --shutdown` in Windows PowerShell, restart WSL, start Chrome, and
restart the MCP client. WSL's default NAT mode does not share Windows loopback.
Do not work around this by binding Effector to a LAN or NAT gateway address;
Effector intentionally accepts loopback addresses only.

If the mode is `mirrored` but TCP port `37654` is still unreachable, inspect the
Windows and Hyper-V firewall policy for the WSL connection. Do not open the port
to public or LAN interfaces.

## Port already in use

Effector currently uses the fixed port `37654` and supports one active Chrome
profile. Close the other Chrome profile or process using that port, then reload
the intended extension. Effector does not attach to or terminate an unknown
listener.

## Report diagnostics safely

Include the operating systems used by Chrome and the MCP client, the output of
`wslinfo --networking-mode` when applicable, and the error text. Remove bearer
tokens, tab titles, URLs, and other browser inventory from reports.
