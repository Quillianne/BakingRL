# Host Documentation Source

`docs-src/` contains the English source for the BakingRL host documentation.
It documents the application, not the plugin SDK. Public plugin authoring
contracts live in the sibling `BakingRLSDK` documentation.

The future French documentation should be a separate source tree or build
target, not mixed into these English pages. Keep page slugs stable so links can
be shared across languages later.

## Structure

```txt
main.qd
_nav.qd
_setup.qd
pages/
  overview.qd
  setup.qd
  repository-layout.qd
  architecture.qd
  telemetry.qd
  developer-workflow.qd
  plugin-system.qd
  plugin-lifecycle.qd
  plugin-package-format.qd
  plugin-security.qd
  troubleshooting.qd
  status.qd
```

## Build

```sh
npm run docs:build
npm run docs:dev
```

The build uses `--subdoc-naming file-name`, so generated URLs are based on file
names. Avoid renaming pages unless the redirect story is explicit.

## Writing Conventions

- Each page starts with `.docname`, `.docauthor`, `.include`, and `.doclang`.
- Do not repeat `.docname` as the first visible Markdown heading.
- Prefer short guide sections followed by precise reference sections.
- Keep host behavior here and SDK/package authoring details in `BakingRLSDK`.
- Document current stable behavior only. Loose planning belongs in Obsidian.
