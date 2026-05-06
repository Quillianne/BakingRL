# Plugin Bundle Format

BakingRL installs v2 plugin packages from `.brlp` bundles.

Authoring documentation and CLI usage live in `../BakingRLSDK/docs`.

## Host Requirements

Required file:

```txt
bakingrl.plugin.json
```

Supported files:

```txt
dist/
assets/
schemas/
manifest.hashes.json
signature.ed25519
```

## Validation

The host rejects bundles with:

- missing `bakingrl.plugin.json`;
- unsupported manifest schema;
- absolute paths;
- `../` path traversal;
- symlinks;
- oversized files;
- oversized total uncompressed size;
- hash mismatches;
- invalid Ed25519 signatures.

Installation is staged first, then moved into the live package directory only
after user confirmation.
