# Ticket 06: Visuals And Configuration Surfaces

## Objective

Unify visual rendering around reusable plugin-provided UI surfaces while separating overlay visuals from settings/configuration visuals.

## Visual Types

- `overlay`: rendered in-game.
- `config`: rendered in the app settings flow.
- `external`: rendered by a plugin-owned external surface such as OBS or a future web dashboard.

## Implementation Work

- Update visual contribution schema for manifest v4.
- Remove permission checks from visual context APIs.
- Provide a stable `VisualContext` for overlays:
  - telemetry snapshot;
  - telemetry subscribe;
  - state subscribe;
  - service calls;
  - instance settings;
  - diagnostics.
- Provide a separate `ConfigurationContext` for config visuals:
  - plugin settings;
  - instance settings where relevant;
  - secrets;
  - validation;
  - save/reset;
  - service calls.
- Ensure visuals can be loaded without starting unrelated plugin runtimes when possible.
- Keep visual APIs compatible with future remote rendering:
  - explicit data inputs;
  - no implicit filesystem reads;
  - no dependence on core OBS routes.

## Acceptance Criteria

- Overlay visuals render with telemetry access.
- Config visuals render inside the app settings UX.
- Instance settings can be edited without affecting other visual instances.
- Existing permission-gated visual API paths are removed.
- A visual can be documented as local-only or remote-compatible.

## Test Plan

- Frontend test for overlay visual loading.
- Frontend test for config visual loading.
- Frontend test for instance settings updates.
- Manual test with one plugin exposing both overlay and config visuals.

## Dependencies

- Ticket 02.
- Ticket 03.
- Ticket 05.

## Out Of Scope

- Building a remote rendering server.
- Implementing the OBS gateway plugin.
