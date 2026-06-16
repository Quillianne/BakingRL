# Ticket 01: Core Principles And Product Boundaries

## Objective

Lock the architectural rules before changing runtime code. This ticket defines what must stay in the base app and what must move to plugins.

## Decisions

- The base app focuses on Rocket League telemetry and in-game overlays.
- OBS is not a core concept.
- Remote dashboards are not core concepts.
- Exporters, bridges and external gateways are plugins.
- Plugins are trusted; the platform optimizes openness and performance over permission isolation.
- No backward compatibility is required for existing plugin manifests or runtime APIs.

## Implementation Work

- Add a short architecture document in the developer docs describing the base app boundary.
- Update any existing plugin documentation that describes permissions as a security boundary.
- Identify core UI references to OBS or external output surfaces and mark them for removal in later tickets.
- Define the minimum host responsibilities:
  - telemetry ingestion;
  - telemetry/state distribution;
  - plugin lifecycle;
  - runtime supervision;
  - settings/secrets persistence;
  - in-game overlay rendering;
  - diagnostics and safe startup.

## Acceptance Criteria

- The documented product boundary is unambiguous.
- No future ticket relies on OBS being available in the core app.
- No future ticket relies on plugin permission prompts or permission-gated APIs.
- The refactor can be implemented as a breaking change.

## Test Plan

- Documentation review only.
- Confirm later tickets reference this boundary instead of redefining product scope.

## Out Of Scope

- Removing code.
- Implementing the new runtime API.
- Implementing the OBS plugin.
