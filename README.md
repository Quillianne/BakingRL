# BakingRL

BakingRL is a desktop host for Rocket League overlays and plugin-powered
visuals. The application ingests Rocket League telemetry, enforces package
permissions, and renders overlays through Tauri windows and OBS browser sources.

This repository contains only the host application. The SDK and first-party
plugins live in sibling repositories:

```txt
perso/
  BakingRL/
  BakingRLSDK/
  BakingRLPlugins/
```

## Documentation

The public documentation is authored with Quarkdown in
[docs-src](docs-src). The old Markdown documentation has been moved to
[doc_bak](doc_bak).

Build the documentation site:

```sh
npm run docs:build
```

Live preview:

```sh
npm run docs:dev
```

## Run The App

```sh
npm install
npm run check
npm run build
npm run tauri dev
```

On Ubuntu, install the Tauri native dependencies first:

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
