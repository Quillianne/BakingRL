# Ticket 07: OBS Gateway As First-Party Plugin

## Objective

Remove OBS gateway behavior from the base app and reintroduce it as a first-party plugin implemented with the new sidecar model.

## Implementation Work

- Remove OBS-specific startup from the core app.
- Remove OBS-specific core routes and settings.
- Create a first-party plugin package, for example `bakingrl.obs-gateway`.
- Implement the gateway as a sidecar-owned local HTTP/WebSocket service.
- Let the plugin discover compatible visuals through the host plugin registry.
- Expose OBS-facing routes from the sidecar, not the core app.
- Store OBS token, port and gateway settings as plugin settings.
- Show OBS UI only when the plugin is installed and enabled.

## Expected Plugin Responsibilities

The OBS plugin should handle:

- overlay page serving;
- WebSocket event streaming;
- token/auth behavior;
- OBS-specific layout discovery;
- external browser source compatibility.

The core should only provide:

- telemetry/state APIs;
- visual package metadata;
- service routing;
- plugin lifecycle.

## Acceptance Criteria

- The base app can run with no OBS routes.
- The base UI contains no OBS settings when the OBS plugin is not installed.
- Installing the OBS plugin restores OBS gateway functionality.
- OBS gateway crashes do not crash the base app.
- OBS plugin logs and status appear in diagnostics.

## Test Plan

- Core app smoke test without OBS plugin.
- Plugin integration test with OBS plugin installed.
- Manual test loading an OBS browser source URL.
- Sidecar crash/restart test.

## Dependencies

- Ticket 03.
- Ticket 04.
- Ticket 05.
- Ticket 06.

## Out Of Scope

- Remote cloud dashboards.
- OBS marketplace packaging.
