# Ticket 08: SDK And Plugin Templates

## Objective

Update the SDK and plugin scaffolding so new plugins target manifest v4 and runtime API 2.0.0 by default.

## Implementation Work

- Update TypeScript SDK types for:
  - manifest v4;
  - telemetry API;
  - state API;
  - settings API;
  - secrets API;
  - sidecar API;
  - visual context;
  - configuration context.
- Remove permission-related SDK types and generator prompts.
- Update manifest validation.
- Update plugin templates:
  - visual overlay plugin;
  - sidecar plugin;
  - Node plus sidecar plugin;
  - plugin with settings schema;
  - plugin with custom config visual;
  - OBS/external surface plugin template if useful.
- Update docs to explain the trusted plugin model.

## Acceptance Criteria

- Newly generated plugins use `bakingrl.plugin/4`.
- No generated plugin contains `capabilities.permissions`.
- Templates compile against SDK `2.0.0`.
- Templates demonstrate settings and telemetry APIs.
- Sidecar templates demonstrate direct host RPC access.

## Test Plan

- SDK typecheck.
- Template generation tests.
- Manifest validation tests.
- Build smoke test for each template.

## Dependencies

- Ticket 02.
- Ticket 03.
- Ticket 04.
- Ticket 05.
- Ticket 06.

## Out Of Scope

- Backward-compatible SDK shims.
- Marketplace publishing workflow.
