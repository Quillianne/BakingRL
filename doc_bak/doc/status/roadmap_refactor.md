# BakingRL v2 Refactor Status

This document replaces the original long-form execution plan. The v2 package
refactor is now the active architecture.

## Completed

- v1 plugin runtime and mono-role sample plugins removed.
- Host app, SDK, and first-party plugin source split into separate Git
  repositories.
- v2 package manifests implemented with visuals, components, services,
  connectors, permissions, imports, assets, and page templates.
- `.brlp` bundle inspection, safe extraction, hash validation, and optional
  Ed25519 signature verification implemented.
- Package installation supports local files, HTTPS URLs, website deep links,
  and Git bundle sources through staged confirmation.
- Dashboard reworked around package management.
- Overlay layouts migrated to ordered layers with a dedicated event layer.
- Layout-specific OBS/browser-source routes added.
- Shared renderer supports runtime overlays, layout editor, and custom pages.
- Service and connector runtimes run behind host-mediated APIs.
- Component imports and service calls are routed through the host gateway.

## Repository Responsibilities

- `BakingRL`: host application, runtime, dashboard, package installer, overlay
  renderer, OBS gateway, and app documentation.
- `BakingRLSDK`: SDK types, scaffolder, package helper CLI, and public
  plugin-author documentation.
- `BakingRLPlugins`: maintained plugin packages developed by the project.

## Next Work

1. Design package marketplace/source discovery.
2. Improve dashboard diagnostics for denied package operations.
3. Add release automation for signed `.brlp` artifacts.
4. Publish `@bakingrl/plugin-sdk` and `create-bakingrl-plugin`.
5. Expand page-template authoring support in the SDK scaffolder.
