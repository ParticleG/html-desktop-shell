# `web/packages/`

## Responsibility

This directory is the Bun workspace's reusable package layer. It keeps the browser-to-host protocol independent of the Vue application so bridge consumers do not depend on Vue, Pinia, or component models.

| Package | Responsibility | Detailed map |
| --- | --- | --- |
| `shell-api/` | Framework-independent Bridge v4 contracts, validators, transport, typed snapshots, and exact operation wrappers. | [`shell-api/codemap.md`](shell-api/codemap.md) |

## Design and flow

- `web/package.json` declares `workspaces: ["packages/*"]` and consumes `@html-desktop-shell/shell-api` through `workspace:*`.
- `shell-api/package.json` exports ESM TypeScript source directly from `src/index.ts` and has no framework or runtime-library dependency.
- The package validates exact Bridge v4 `HostInfo` and scoped `Capabilities` bootstrap results, including the four discriminated surface contexts.
- Typed wrappers expose `getHostInfo`, `getCapabilities`, `getState`, workspace focus, Quick Panel visibility, and Diagnostics visibility. The generic transport does not grant generic native authority; the native per-surface allowlist remains the security boundary.
- Application imports resolve to the source through the workspace. Calls post `{ id, method, params }` to the host's WebKit handler and receive parsed typed results or a `NativeBridgeError` retaining native code/message.

## Integration points

- `web/src/stores/shell.ts` is the principal runtime consumer and caches host info, capabilities, state, and errors.
- `web/src/SurfaceRoot.vue` and the four views consume validated surface identity and scoped capabilities through the store.
- View-model and widget modules import provider types for presentation without acquiring bridge authority.
- The native Bridge v4 dispatcher implements the same exact operations and remains authoritative for method scope and parameter validation.
