# Ticket 05: Plugin Settings System

## Objective

Replace the current settings behavior with a host-owned settings system that supports trusted plugins, custom configuration visuals and per-instance overlay configuration.

## Design

Use two layers:

1. Declarative schema for defaults, validation and automatic host UI.
2. Optional custom configuration visual for advanced plugin-owned setup.

This follows the broad pattern used by trusted plugin ecosystems:

- VS Code exposes declarative configuration and supports custom webviews.
- Obsidian lets plugins persist their own data and build settings UI.
- JetBrains plugins can provide custom settings pages.
- Figma plugins can declare and open custom plugin UI.

## Settings Scopes

- `plugin`: global plugin settings.
- `instance`: settings for a specific visual instance in a layout.
- `secret`: sensitive values stored separately from normal JSON settings.

## Implementation Work

- Define settings schema support in manifest v4:
  - `contributes.settings.schema`;
  - `contributes.settings.ui`;
  - optional restart-required metadata.
- Persist settings outside the plugin package directory.
- Keep secrets outside settings JSON and expose them through host APIs.
- Validate settings on save.
- Merge defaults with persisted values on read.
- Emit settings change events to:
  - Node runtime;
  - sidecars;
  - visuals;
  - configuration visuals.
- Hot-apply changes by default.
- Restart runtimes only when a setting explicitly requires restart.
- Add host-rendered automatic settings UI when no custom settings visual exists.

## Configuration Visual Context

Custom settings visuals receive a `ConfigurationContext` instead of the normal overlay context.

Expected APIs:

```ts
const current = await bakingrl.settings.get();
await bakingrl.settings.update({ theme: "compact" });

const apiKey = await bakingrl.secrets.get("apiKey");
await bakingrl.secrets.set("apiKey", nextValue);

const unsubscribe = bakingrl.settings.subscribe(next => {
  // update UI
});
```

Configuration visuals may call plugin services for actions such as:

- test connection;
- scan local resource;
- preview generated output;
- validate credentials.

They should not write directly to the filesystem.

## Acceptance Criteria

- A plugin with only a schema gets a usable automatic settings UI.
- A plugin with a custom settings visual can render its own configuration UI.
- Global plugin settings persist across restarts.
- Visual instance settings are stored per layout instance.
- Secrets are not stored in normal settings JSON.
- Hot updates reach Node, sidecars and visuals.
- Restart-required settings trigger controlled runtime restart.

## Test Plan

- Unit tests for defaults merge.
- Unit tests for validation failures.
- Unit tests for secrets separation.
- Integration test for hot settings update.
- Integration test for restart-required settings.
- Frontend test for automatic settings UI.
- Frontend test for custom configuration visual.

## Dependencies

- Ticket 02.
- Ticket 04.

## Out Of Scope

- Remote settings sync.
- Multi-user settings profiles.
- Plugin marketplace review policy.
