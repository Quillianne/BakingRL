# BakingRL Developer Guide

This guide is for working on the BakingRL host application. Plugin authoring
now lives in the `BakingRLSDK` repository.

## Local Repository Layout

Use sibling repositories:

```txt
perso/
  BakingRL/         Rust/Tauri host application
  BakingRLSDK/      SDK, scaffolder, helper CLI, plugin docs
  BakingRLPlugins/  First-party plugin packages
```

## Host Prerequisites

- Node.js 20 or newer.
- npm 10 or newer.
- Rust stable.
- Tauri native dependencies.

Ubuntu:

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

## Install And Verify

```sh
npm install
npm run check
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
```

## Run Development Mode

```sh
npm run tauri dev
```

The Tauri process starts Vite on port `1420`, compiles the Rust host, then
launches the desktop app.

## Install Local Plugins

Develop plugins from `../BakingRLPlugins`:

```sh
cd ../BakingRLPlugins
npm install
npm run build
npm run validate
npm run install:local
```

Then return to BakingRL and reload packages from the dashboard.

## App-Data Package Directory

BakingRL loads installed packages from app-data, not from the repository.

- Linux: `$XDG_DATA_HOME/com.quillianne.bakingrl/packages` or
  `~/.local/share/com.quillianne.bakingrl/packages`
- macOS: `~/Library/Application Support/com.quillianne.bakingrl/packages`
- Windows: `%LOCALAPPDATA%/com.quillianne.bakingrl/packages`

Set `BAKINGRL_PACKAGES_DIR` while testing to point the app at a custom package
directory.

## Useful Commands

```sh
npm run check       # Svelte and TypeScript diagnostics
npm run build       # production frontend build
npm run tauri dev   # full desktop dev session
cargo test --manifest-path src-tauri/Cargo.toml
```

## Scope Boundary

Do not add SDK packages or first-party plugin source back into this repository.
Host code should expose stable runtime APIs; SDK docs and plugin examples belong
in `BakingRLSDK` and `BakingRLPlugins`.
