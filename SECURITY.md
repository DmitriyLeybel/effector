# Security policy

## Supported versions

Effector is pre-release software. Security fixes are applied to the latest code
on the default branch; no older release line is currently supported.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting for this repository when it is
available. If it is unavailable, open a public issue that asks the maintainers
for a private contact channel without including vulnerability details.

Do not include MCP bearer tokens, browser inventory, tab titles, URLs, extension
IDs from private builds, or proof-of-concept data from a real browser profile in
public reports.

Useful non-sensitive context includes the operating systems used by Chrome and
the MCP client, Effector and extension versions, whether WSL is involved, and a
redacted description of the failing boundary.

## Security model

Effector's current security boundary depends on all of the following:

- Chrome launches and owns the native broker process.
- The Native Messaging manifest allowlists one exact extension origin.
- MCP binds only to loopback and requires a random bearer token.
- MCP validates Host and Origin headers.
- Native Messaging protocol version 3 binds responses to the connected browser
  identity and strictly validates capability, request, and response envelopes.
- New browser snapshots expose random process-local references rather than
  Chrome numeric IDs and retain bounded state only in broker memory.
- Global capability settings can be changed only through extension UI. Any
  future capability enabled there is shared by every client holding the
  installation bearer token; there is no per-client authority layer.
- Incognito access, page content, arbitrary Chrome API forwarding, and shell
  execution are not available.

Changes that weaken any of these properties require explicit security review.
