# Quarkdown Documentation Source

`main.qd` is the landing page for BakingRL host documentation. Each page under
`pages/` is linked as a Quarkdown subdocument and becomes a separate HTML page
in the generated site.

```txt
main.qd
_nav.qd
_setup.qd
pages/
  overview.qd
  setup.qd
  repository-layout.qd
  architecture.qd
  plugin-system.qd
  plugin-lifecycle.qd
  plugin-package-format.qd
  plugin-security.qd
  telemetry.qd
  developer-workflow.qd
  troubleshooting.qd
  status.qd
```

Build:

```sh
npm run docs:build
```

Live preview:

```sh
npm run docs:dev
```

The build script uses `--subdoc-naming file-name` so generated URLs are stable.
Each page defines `.docname`; do not repeat that title as the first Markdown
heading, otherwise Quarkdown renders a duplicate visible title.
