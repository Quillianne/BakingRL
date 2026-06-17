# Ticket 02: Plugin Manifest V4

## Objective

Replace the current plugin manifest with a breaking `bakingrl.plugin/4` schema that matches the new trusted runtime model.

## Implementation Work

- Introduce manifest schema version `bakingrl.plugin/4`.
- Remove `capabilities.permissions` and all permission declarations.
- Define top-level runtime declarations:
  - `runtime.node`;
  - `runtime.sidecars`.
- Define contribution declarations:
  - `contributes.visuals`;
  - `contributes.settings`;
  - `contributes.services`;
  - `contributes.commands` if still needed.
- Define optional external surface declarations:
  - `externalSurfaces.obs`;
  - `externalSurfaces.web`;
  - `externalSurfaces.remote`.
- Add explicit platform filtering for runtimes and binaries.
- Keep package identity strict:
  - stable package id;
  - display name;
  - version;
  - supported BakingRL API version.

## Suggested Manifest Shape

```json
{
  "schemaVersion": "bakingrl.plugin/4",
  "id": "example.plugin",
  "name": "Example Plugin",
  "version": "1.0.0",
  "bakingrlApi": "2.0.0",
  "runtime": {
    "node": {
      "entry": "dist/extension.js"
    },
    "sidecars": [
      {
        "id": "worker",
        "bin": "bin/worker",
        "platforms": ["linux-x64", "windows-x64"]
      }
    ]
  },
  "contributes": {
    "visuals": [
      {
        "id": "main-overlay",
        "entry": "dist/overlay/index.js",
        "instanceSettings": "schemas/main-overlay.schema.json"
      }
    ],
    "settings": {
      "schema": "schemas/settings.schema.json",
      "ui": {
        "kind": "visual",
        "entry": "dist/config/index.js"
      }
    }
  }
}
```

## Acceptance Criteria

- The host rejects non-v4 manifests after the breaking refactor.
- The host no longer parses plugin permissions.
- Runtime declarations are enough to start Node and sidecar runtimes.
- Visuals and settings can be discovered from the manifest without runtime startup.
- Platform filtering is applied before trying to spawn a runtime.

## Test Plan

- Unit tests for valid v4 manifests.
- Unit tests for rejected legacy manifests.
- Unit tests for missing runtime entries.
- Unit tests for platform-specific sidecar filtering.
- Snapshot tests for generated manifest examples if the SDK has generator coverage.

## Dependencies

- Ticket 01.

## Out Of Scope

- SDK template migration.
- Runtime supervision changes.
- OBS extraction.
