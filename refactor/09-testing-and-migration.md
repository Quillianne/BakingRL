# Ticket 09: Testing, Removal And Migration Checklist

## Objective

Make the breaking refactor safe by defining the required tests, removal checklist and manual validation scenarios.

## Removal Checklist

- Remove legacy manifest parsing.
- Remove permission declarations from manifests.
- Remove permission checks from frontend visual APIs.
- Remove permission checks from runtime APIs.
- Remove OBS startup from core.
- Remove OBS routes from core.
- Remove OBS settings from core UI.
- Remove storage assumptions that place plugin writable data under the package directory.
- Remove docs that describe permissions as a security feature.

## Required Automated Tests

Rust:

- manifest v4 parsing;
- legacy manifest rejection;
- platform filtering;
- telemetry snapshot and stream;
- sidecar lifecycle;
- settings validation;
- secrets separation;
- restart-required settings.

TypeScript:

- SDK type coverage;
- manifest generation;
- template builds;
- visual context types;
- configuration context types.

Frontend:

- overlay visual rendering;
- config visual rendering;
- automatic settings UI;
- instance settings editing;
- no OBS UI without OBS plugin.

## Manual Scenarios

- Run the app with no plugins.
- Install a visual-only plugin.
- Install a sidecar-only plugin.
- Install a Node plus sidecar plugin.
- Install a plugin with automatic settings.
- Install a plugin with custom config visual.
- Update settings while plugin is running.
- Trigger a restart-required setting.
- Disable a crashing plugin.
- Install the OBS gateway plugin and load an OBS browser source.

## Acceptance Criteria

- The app starts cleanly with no plugins.
- A plugin can consume telemetry from Node, sidecar and visual contexts.
- A plugin can render an in-game overlay.
- A plugin can expose custom settings UI.
- OBS functionality exists only through the OBS plugin.
- No legacy permissions remain in user-facing docs, SDK types or manifest schema.

## Dependencies

- All implementation tickets.

## Out Of Scope

- Preserving existing plugin compatibility.
- Remote server implementation.
