# Plugin Security

BakingRL treats plugin packages as untrusted code. Rust is the policy authority.

## Enforced Boundaries

- Package IDs cannot use reserved runtime prefixes.
- Registry writes are scoped to package-owned keys.
- Storage access is limited to package-owned `plugin://self/*` paths.
- Connector network hosts must be explicitly declared.
- Bundle extraction rejects path traversal, symlinks, and oversized payloads.
- Hashes and Ed25519 signatures are verified when present.

## Runtime Mediation

Plugins do not access each other directly. The host mediates:

- component imports;
- service calls;
- registry reads and writes;
- package settings;
- asset URLs;
- HTTP and WebSocket access.

## SDK Boundary

The SDK provides types and helper functions only. It is not trusted for security
decisions. Any operation that crosses a package boundary must be checked by the
host.
