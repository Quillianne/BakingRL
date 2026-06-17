# BakingRL Plugin Platform Refactor Plan

## Goal

Refactor the plugin platform around the actual product goal: make Rocket League telemetry easy to consume, render and redistribute without making the base app responsible for every output surface.

This refactor intentionally does not preserve backward compatibility with the current plugin manifest or runtime APIs.

## Non-Negotiable Rules

1. The base app is for Rocket League telemetry and in-game overlays.
2. OBS, remote dashboards, exports and other output channels are plugins, not core features.
3. Plugins are trusted. Remove the permission/capability security model from manifests, SDK and UI.
4. Sidecars are first-class runtimes, not helper processes hidden behind Node.
5. Visual plugins must support high-performance overlay rendering.
6. Settings are owned and persisted by the host, but plugins may provide custom visual settings UIs.
7. Runtime and API design must keep future remote surfaces possible, without implementing the remote server now.
8. Performance regressions visible to users are unacceptable.

## Target Architecture

The host owns:

- Rocket League telemetry ingestion.
- Telemetry snapshot and stream distribution.
- Shared state hub.
- Plugin lifecycle.
- Runtime supervision.
- Plugin settings and secrets persistence.
- In-game overlay surface.
- Diagnostics and safe startup controls.

Node extension hosts own:

- JavaScript plugin orchestration.
- Command and service registration.
- Lightweight integration logic.
- Communication with visuals and sidecars.

Sidecars own:

- Heavy compute.
- Native integrations.
- Local HTTP or WebSocket services.
- Cloud or remote uplinks.
- OBS gateway behavior when provided by the first-party OBS plugin.

Visuals own:

- Overlay rendering.
- Configuration rendering.
- Future web-compatible rendering surfaces.

## Public API Direction

Introduce a breaking manifest version:

- `schemaVersion: "bakingrl.plugin/4"`
- SDK major version: `2.0.0`
- Runtime API major version: `2.0.0`

The manifest should expose these concepts:

- `runtime.node`
- `runtime.sidecars`
- `contributes.visuals`
- `contributes.settings`
- `contributes.services`
- `externalSurfaces`

Remove these concepts:

- `capabilities.permissions`
- permission-gated frontend APIs
- core OBS routes and settings
- compatibility code for old plugin manifests

## Settings Direction

Use a two-layer settings model inspired by trusted plugin ecosystems:

- Declarative settings schema for defaults, validation and automatic host UI.
- Optional custom configuration visual for advanced plugin-owned setup.

Settings scopes:

- `plugin`: global plugin settings.
- `instance`: per visual instance settings.
- `secret`: sensitive values stored separately by the host.

Settings changes should hot-apply by default. A manifest key may explicitly mark a setting as restart-required.

## Ticket Map

1. `01-core-principles.md`
2. `02-manifest-v4.md`
3. `03-telemetry-state-hub.md`
4. `04-runtime-node-and-sidecars.md`
5. `05-plugin-settings.md`
6. `06-visuals-and-config-surfaces.md`
7. `07-obs-gateway-plugin.md`
8. `08-sdk-and-templates.md`
9. `09-testing-and-migration.md`

## External References

- VS Code contribution points and configuration model: https://code.visualstudio.com/api/references/contribution-points#contributes.configuration
- VS Code webviews: https://code.visualstudio.com/api/extension-guides/webview
- Obsidian plugin data APIs: https://raw.githubusercontent.com/obsidianmd/obsidian-api/master/obsidian.d.ts
- JetBrains plugin settings guide: https://plugins.jetbrains.com/docs/intellij/settings-guide.html
- Figma plugin manifest and UI model: https://developers.figma.com/docs/plugins/manifest/
