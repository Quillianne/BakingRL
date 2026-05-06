# Pages Roadmap

## Goal

Add a `My Pages` mode where users can create custom in-app pages from the same
visual plugin exports used by overlays. A page should let the player assemble
performance dashboards, live match panels, graphs, scoreboards, text, images,
and backgrounds directly inside BakingRL.

## Core Concept

A page is effectively a BakingRL layout that is not an overlay.

It should reuse the same foundation as overlay layouts:

- layers;
- items;
- visual plugin exports;
- per-instance settings;
- ordering, sizing, and positioning;
- shared renderer logic.

But pages differ from overlays:

- no click-through behavior;
- plugins can receive clicks, inputs, hover, scroll, and keyboard focus;
- pages render inside BakingRL instead of OBS or the in-game overlay window;
- users can add native content blocks such as text, images, and backgrounds;
- pages can have a read mode and an edit mode.

## Proposed Model

Create a persistent `pages.json` model close to `OverlayLayout`.

Suggested fields:

- `id`;
- `name`;
- `width`;
- `height`;
- `background`;
- `layers`;
- optional page metadata such as `createdAt`, `updatedAt`, or `templateSource`.

Native page blocks can be represented as built-in item types or internal
package-like exports:

- text;
- image;
- shape/container;
- background.

The important design rule is that plugin visuals and native blocks should live
in the same canvas/layer system.

## UI Scope

Add a main app tab named `My Pages`.

Initial workflows:

- list pages;
- create page;
- rename page;
- duplicate page;
- delete page;
- open page;
- edit page.

The page editor can reuse most of the overlay editor, but it needs explicit
modes:

- edit mode: select, move, resize, configure items;
- preview/read mode: pointer and keyboard events go to the plugins.

## Renderer Changes

Factor the current overlay renderer into a more general layout renderer.

Suggested renderer modes:

- `runtime-overlay`: current OBS/in-game behavior;
- `editor`: current layout editing behavior;
- `page`: in-app interactive page behavior.

In `page` mode:

- visual roots should keep `pointer-events: auto`;
- focusable controls inside plugin visuals must work;
- the renderer should still receive telemetry and registry/service access;
- layout chrome should not block plugin interactions outside edit mode.

## Plugin Integration

The same `visuals` exports should work in overlays and pages.

Later, plugins could also provide page templates through the manifest, for
example:

```json
{
  "exports": {
    "pages": {
      "performance-dashboard": {
        "path": "templates/performance-dashboard.page.json"
      }
    }
  }
}
```

This should be treated as an importable template, not a locked page. Users
should be able to modify imported pages after creation.

## Data Requirements

Live panels can use telemetry directly. Historical widgets, such as win/loss
graphs or long-term performance tracking, need a service plugin that aggregates
and persists data.

Examples:

- a stats service consumes `MatchEnded` and stores win/loss history;
- a graph visual reads that service or registry state;
- a scoreboard visual reads live `UpdateState` and optionally BO Tracker state.

## Implementation Steps

Initial implementation status:

- backend persistence and Tauri commands for `pages.json` are in place;
- `My Pages` can create, list, rename, duplicate, delete, open, and edit pages;
- pages can open in the main app window or in a dedicated app window through
  per-page settings;
- the renderer supports a page mode and native `text`, `image`, and `shape`
  blocks alongside plugin visual exports;
- packages can expose importable page templates through `exports.pages`.

Remaining/future work:

1. Extract shared layout/page TypeScript and Rust types further where practical.
2. Add richer native block editors beyond JSON settings.
3. Add page template authoring helpers to the plugin scaffolder.
4. Decide whether pages need page-only layer properties beyond the current
   shared layer model.
5. Expand responsive sizing options beyond the current fixed-canvas scaling.

## Open Questions

- Should pages share the exact same layer model as overlays, or should they
  have extra page-only properties?
- Should native text/image blocks be built into the renderer or exposed as
  first-party plugins?
- Should page templates be a new manifest export type or regular assets plus a
  convention?
- How should responsive sizing work for pages: fixed canvas, fluid canvas, or
  selectable presets?
