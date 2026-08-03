# ADR 0001: Target Chrome and documented Chromium APIs

Status: accepted
Date: 2026-07-30

## Context

Browser-specific private APIs create invasive installation requirements,
unstable compatibility, and an architecture that cannot be distributed as a
normal Chrome extension.

## Decision

Effector targets Google Chrome and documented Chrome extension APIs. The core
model includes Chrome windows, standard Tab Groups, and tabs.

## Consequences

- The extension has a conventional Manifest V3 installation path.
- Standard Chrome API behavior is the compatibility contract.
- Vendor-private grouping or workspace systems are not represented.
- Browser expansion, if reconsidered, requires a new decision record and an
  adapter based on documented public APIs.
