# BakingRL

BakingRL is the desktop host application for Rocket League telemetry and
plugin runtimes. It owns telemetry ingestion, package discovery, runtime
lifecycle, host-mediated plugin APIs, package settings/secrets, diagnostics,
the verified Marketplace client, transactional package installation, and the
dashboard UI.

Plugin authoring tools and public plugin contracts live in the sibling
`BakingRLSDK` repository. First-party plugin source lives in `BakingRLPlugins`.
The signed public catalogue lives in `BakingRLMarketplace`.

## Repository Role

Use this repository when working on:

- the Tauri host and Rust backend;
- the Svelte dashboard, standalone package webviews, and package manager;
- package installation, runtime compatibility, settings, secrets, and diagnostics;
- Marketplace catalogue verification, publisher trust, dependency planning,
  platform artifact selection, and transactional installation;
- plugin runtime APIs, command/service routing, resources, and sidecar status;
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

On macOS, the first Tauri dev or build command downloads the pinned official
Node.js runtime into `node_modules/.cache`, verifies its SHA-256 digest, and
stages it as the plugin runtime sidecar. Set `BAKINGRL_NODE_BINARY` only when an
explicit self-contained Node binary must be used instead.

Local macOS bundles use a valid ad-hoc signature so the embedded runtime receives
its JIT entitlement. A production build must set `APPLE_SIGNING_IDENTITY` to a
Developer ID identity; Tauri gives that environment variable precedence over the
local default.

Recommended sibling layout:

```txt
bakingproject/
  BakingRL/
  BakingRLSDK/
  BakingRLPlugins/
  BakingRLMarketplace/
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

The current host runtime API is `2.3.0`. The host supports packages declaring:

```txt
2.3.x
```

Packages without a compatible top-level `bakingrlApi` field can be installed
for inspection, but BakingRL disables them and refuses activation until they are
rebuilt with a compatible SDK.

The host downloads the Marketplace catalogue from
`https://quillianne.github.io/BakingRLMarketplace`, verifies its detached
signature against a pinned root key, and installs only exact reviewed artifacts.
Signatures establish provenance, not safety: plugins execute trusted code with
the user's rights, so users must trust the publisher and receive no warranty.
