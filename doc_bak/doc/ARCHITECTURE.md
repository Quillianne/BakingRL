# BakingRL Architecture

BakingRL is a Tauri desktop application with a Rust host and a Svelte frontend.
Rust is the policy authority. The frontend is the control surface and renderer.

## Core Responsibilities

The host application owns:

- Rocket League telemetry ingestion.
- Native event bus fan-out.
- Package installation and validation.
- Effective permission calculation.
- Service and connector runtimes.
- Registry and package settings.
- Overlay layouts and custom pages.
- OBS browser-source HTTP/WebSocket gateway.

The SDK and first-party plugin source are intentionally outside this repository.

## Runtime Flow

1. Rocket League emits telemetry on its local socket.
2. `ingestor` parses telemetry into internal bus events.
3. `plugin_host` forwards events to enabled package runtimes and visual hosts.
4. Visual exports render in Tauri windows or OBS browser-source routes.
5. Services and connectors run behind host-mediated RPC.
6. Registry and settings writes go through Rust permission checks.

## Important Rust Modules

- `bus`: in-memory telemetry/event broadcast.
- `ingestor`: Rocket League local telemetry connection.
- `plugin_v2::manifest`: package manifest data model.
- `plugin_v2::permissions`: effective permission calculation.
- `plugin_v2::bundle`: `.brlp` inspection and safe extraction.
- `plugin_v2::install`: package install pipeline.
- `plugin_host`: installed package scanning, settings, runtime orchestration,
  overlays, pages, and Tauri commands.
- `obs_gateway`: HTTP/WebSocket gateway for OBS and browser rendering.
- `window_watcher`: platform overlay visibility integration.

## Frontend Structure

- `src/routes/+page.svelte`: dashboard.
- `src/lib/OverlayRenderer.svelte`: shared runtime/editor renderer.
- `src/routes/overlay/*`: overlay and browser-source routes.
- `src/routes/editor/*`: fullscreen layout and page editors.
- `src/lib/adapter`: Tauri and OBS/browser transport abstraction.

## Package Runtime Model

Installed packages are read from the app-data package directory. A package can
export visuals, components, services, connectors, and page templates. Cross
package access is never direct: the host mediates component loads, service
calls, asset URLs, registry reads, and network operations.

## Security Position

BakingRL assumes package code is untrusted. The host rejects unsafe bundles,
calculates effective permissions, and checks privileged operations at runtime.
SDK helpers are only developer ergonomics.
