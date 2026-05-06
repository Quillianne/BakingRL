# BakingRL

BakingRL is a desktop overlay host for Rocket League telemetry. It ingests the
official local telemetry stream, runs trusted native services in Rust, and
renders plugin-powered overlays in Tauri windows and OBS browser sources.

This repository now contains only the host application.

## Repository Split

The project is organized as three Git repositories:

- `BakingRL`: the Rust/Tauri host application.
- `BakingRLSDK`: TypeScript SDK, plugin package scaffolder, helper CLI, and
  plugin-author documentation.
- `BakingRLPlugins`: first-party and experimental plugin packages developed by
  the project.

Keep the repositories side by side for local development:

```txt
perso/
  BakingRL/
  BakingRLSDK/
  BakingRLPlugins/
```

## What BakingRL Does

- Connects to Rocket League's local telemetry socket.
- Normalizes telemetry into a native event bus.
- Loads installed plugin packages from app-data.
- Enforces package permissions and bundle validation.
- Hosts visuals in in-game overlays, OBS browser sources, and custom pages.
- Runs service and connector exports through host-mediated APIs.
- Provides dashboard, package management, overlay editing, and page editing.

## Requirements

- Node.js 20 or newer.
- npm 10 or newer.
- Rust stable.
- Linux/Tauri native dependencies on Ubuntu:

```sh
sudo apt install -y \
  build-essential \
  curl \
  git \
  pkg-config \
  libdbus-1-dev \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

## Run The App

```sh
npm install
npm run check
npm run build
npm run tauri dev
```

Runtime notes:

- Rocket League telemetry warnings are expected until the game telemetry socket
  is enabled and reachable.
- Deep-link registration warnings can appear in development builds.
- GPU/EGL warnings are usually host-driver related and not a BakingRL compile
  failure when the Tauri window still opens.

## Plugin Development

Plugin source code does not live in this repository anymore.

- Use `../BakingRLSDK` to create packages and read SDK documentation.
- Use `../BakingRLPlugins` for maintained project plugins.
- BakingRL installs packages from `.brlp` files, URLs, Git bundle sources, or
  the local app-data package directory.

## Documentation

- [Developer Guide](doc/DEVELOPER_GUIDE.md)
- [Architecture](doc/ARCHITECTURE.md)
- [API Reference](doc/API_REFERENCE.md)
- [Plugin Bundle Format](doc/PLUGIN_BUNDLE_FORMAT.md)
- [Plugin Security](doc/PLUGIN_SECURITY.md)
- [Current State](doc/status/current_state.md)
- [Roadmap](doc/status/roadmap.md)
- [v2 Refactor Status](doc/status/roadmap_refactor.md)
- [Pages Roadmap](doc/status/roadmap_pages.md)
- [Rocket League Local API](doc/RL_API.md)
