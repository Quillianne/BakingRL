# Current State

BakingRL is on the plugin-system v2 architecture.

## Completed

- Host app, SDK, and first-party plugins are split into separate repositories.
- v2 package manifests, permission calculation, bundle inspection, and install
  staging are implemented.
- `.brlp` bundles support safe extraction, hashes, and optional Ed25519
  signatures.
- Packages can be installed from local files, HTTPS URLs, website deep links,
  and Git bundle sources.
- Dashboard package management is package-oriented.
- Overlay layouts support ordered layers and a dedicated event layer.
- OBS browser-source routes can render active or layout-specific overlays.
- Custom pages share the renderer and editor infrastructure.
- Services and connectors run through host-mediated runtimes.
- Component imports and service calls go through gateway APIs.

## Repository Responsibilities

- `BakingRL`: host application only.
- `BakingRLSDK`: plugin SDK, scaffolder, CLI, and public author docs.
- `BakingRLPlugins`: first-party plugin source packages.

## Remaining Work

- Marketplace and package-source discovery UX.
- More complete package diagnostics in the dashboard.
- Release automation for signed plugin bundles.
- Public publishing workflow for the SDK and scaffolder packages.
