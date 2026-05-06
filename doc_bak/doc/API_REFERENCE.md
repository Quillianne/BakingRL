# BakingRL Host API Reference

This document covers host APIs exposed by the BakingRL app. SDK authoring APIs
are documented in `../BakingRLSDK/docs`.

## Package Commands

- `list_packages`
- `packages_dir`
- `reload_packages`
- `set_package_enabled`
- `inspect_package_bundle`
- `install_package_from_file`
- `install_package_from_url`
- `prepare_package_from_url`
- `prepare_package_from_deep_link`
- `prepare_package_from_git`
- `install_prepared_package`
- `discard_prepared_package`
- `remove_package`

Install sources are staged first. The dashboard shows inspection results before
calling `install_prepared_package`.

## Package Runtime Commands

- `read_visual_export_source`
- `read_component_export_source`
- `call_service_export`
- `plugin_registry_get`
- `get_package_settings`
- `save_package_settings`

These commands are host mediated and permission checked.

## Overlay Commands

- `get_overlay_layouts`
- `save_overlay_layout`
- `create_overlay_layout`
- `duplicate_overlay_layout`
- `set_active_overlay_layout`
- `set_stream_overlay_layout`
- `delete_overlay_layout`

Overlay layouts are layer based. Normal layers hold persistent visuals. The
event layer holds temporary event visuals that can hide normal layers by calling
`setActive(true)`.

## Page Commands

- `get_pages`
- `save_page`
- `create_page`
- `duplicate_page`
- `delete_page`
- `import_package_page`
- `open_page`

Custom pages use the same canvas model as overlays and can include plugin
visuals plus native text, image, and shape items.

## Registry Commands

- `registry_get`
- `registry_entries`

Dashboard reads use these commands. Plugin reads and writes go through scoped
runtime APIs with package permissions.

## Browser Routes

- `/overlay/ingame`
- `/overlay/stream`
- `/overlay/layout/:layoutId`
- `/editor/layout/:layoutId`
- `/page/:pageId`
- `/editor/page/:pageId`

Use layout-specific overlay URLs for OBS scenes that must stay pinned to one
layout.
