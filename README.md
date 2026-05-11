# BakingRL

BakingRL is the desktop host application for Rocket League telemetry overlays,
pages, and plugin runtimes. It owns telemetry ingestion, package discovery,
permissions, runtime lifecycle, overlay windows, OBS browser output, and the
dashboard UI.

Plugin authoring tools and public plugin contracts live in the sibling
`BakingRLSDK` repository. First-party plugin source lives in `BakingRLPlugins`.

## Repository Role

Use this repository when working on:

- the Tauri host and Rust backend;
- the Svelte dashboard, page editor, overlay editor, and package manager;
- package installation, runtime compatibility, permissions, settings, and
  diagnostics;
- OBS gateway routes and in-game overlay windows;
- host-facing documentation.

Do not copy plugin source into this repository. Install plugins through local
app data or `.brlp` bundles instead.

## Local Development

```sh
npm install
npm run check
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
npm run tauri dev
```

Recommended sibling layout:

```txt
bakingproject/
  BakingRL/
  BakingRLSDK/
  BakingRLPlugins/
```

## Documentation

Host documentation source lives in `docs-src/`.

```sh
npm run docs:build
npm run docs:dev
```

The host docs explain application behavior. Plugin package formats, SDK APIs,
generator usage, and authoring workflows are documented in `BakingRLSDK`.

## Current Runtime Contract

The current host runtime API is `0.3.0`. The host supports packages declaring:

```txt
>=0.3.0 <0.4.0
```

Packages without a compatible `compatibility.runtimeApi` field can be installed
for inspection, but BakingRL disables them and refuses activation until they
are rebuilt with a compatible SDK.
