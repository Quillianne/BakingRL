# Ticket 03: TelemetryHub And StateHub

## Objective

Create the central data model used by plugins, visuals and sidecars to consume Rocket League telemetry and shared app state.

## Implementation Work

- Introduce `TelemetryHub` as the host-owned source for game data.
- Support two access patterns:
  - current snapshot;
  - real-time stream.
- Introduce `StateHub` for derived or plugin-shared state that is not raw telemetry.
- Define typed events for:
  - game connection status;
  - match lifecycle;
  - player updates;
  - ball updates;
  - score updates;
  - clock updates;
  - boost updates;
  - plugin-published state.
- Expose equivalent APIs to:
  - Node extension host;
  - sidecars through host RPC;
  - visuals through visual context.
- Avoid routing high-frequency telemetry through a slow general command bus.

## API Expectations

Node and visual API:

```ts
const snapshot = await bakingrl.telemetry.getSnapshot();
const unsubscribe = bakingrl.telemetry.subscribe("match.updated", event => {
  // update local state
});
```

Sidecar RPC API:

```json
{
  "method": "telemetry.subscribe",
  "params": { "events": ["match.updated"] }
}
```

## Acceptance Criteria

- Plugins can read the latest telemetry snapshot without waiting for a new event.
- Plugins can subscribe to live telemetry streams.
- Visuals can receive updates without permission checks.
- Sidecars can consume telemetry without being mediated by Node.
- The telemetry path is documented as performance-sensitive.

## Test Plan

- Unit tests for snapshot updates.
- Unit tests for subscription fan-out.
- Integration test with one Node runtime, one sidecar and one visual subscriber.
- Load test or benchmark for high-frequency telemetry events.

## Dependencies

- Ticket 01.
- Ticket 02.

## Out Of Scope

- Remote server bridge.
- OBS gateway behavior.
- Plugin settings.
