# Plugin Development

Plugin development has moved out of this repository.

Use:

- `../BakingRLSDK` for SDK types, scaffolding, package helper CLI, and
  plugin-author documentation.
- `../BakingRLPlugins` for first-party plugin source packages.

Quick local workflow:

```sh
cd ../BakingRLPlugins
npm install
npm run build
npm run validate
npm run install:local
```

Then start BakingRL and reload packages from the dashboard:

```sh
cd ../BakingRL
npm run tauri dev
```

The BakingRL host repository should not contain plugin source packages.
