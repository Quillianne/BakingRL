# Ticket 04: Node Runtime And First-Class Sidecars

## Objective

Make Node and sidecars peers in the plugin runtime model. Sidecars should be powerful enough to own heavy work and integrations without being hidden behind Node.

## Implementation Work

- Define a shared host RPC contract for Node and sidecar runtimes.
- Expose these host APIs to both runtime types:
  - telemetry snapshot and stream;
  - state hub read/write where appropriate;
  - settings read and subscribe;
  - service calls;
  - diagnostics logging;
  - runtime health reporting.
- Add sidecar lifecycle supervision:
  - startup;
  - stop;
  - restart;
  - crash detection;
  - bounded restart policy;
  - status reporting.
- Keep Node useful for JavaScript orchestration, commands and package glue.
- Allow sidecars to register services directly with the host.
- Apply platform filtering before spawning sidecars.

## Runtime Responsibilities

Node should handle:

- plugin activation logic;
- command registration;
- lightweight service glue;
- orchestration between visuals and sidecars.

Sidecars should handle:

- native dependencies;
- heavy compute;
- local servers;
- OBS gateway;
- cloud or remote sync;
- protocol integrations.

## Acceptance Criteria

- A plugin can run only a sidecar without requiring Node.
- A plugin can run Node and one or more sidecars.
- Sidecars can receive telemetry directly from the host.
- Sidecars can register callable services.
- Sidecar crashes are visible in diagnostics.
- Sidecar restart behavior is deterministic and tested.

## Test Plan

- Unit tests for sidecar spec creation.
- Integration test with sidecar-only plugin.
- Integration test with Node plus sidecar plugin.
- Crash/restart test.
- Platform-filter test.

## Dependencies

- Ticket 02.
- Ticket 03.

## Out Of Scope

- Implementing the OBS plugin.
- Adding remote server behavior.
- Custom settings UI.
