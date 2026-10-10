# `web/packages/`

## Responsibility

This directory is the Bun workspace's reusable package layer. It keeps the browser-to-host protocol independent of the Vue application so bridge consumers do not depend on Vue, Pinia, or component models.

| Package | Responsibility | Detailed map |
| --- | --- | --- |
| `shell-api/` | Framework-independent Bridge v6 types, validators, transport, and 21 exact wrappers. | [`shell-api/codemap.md`](shell-api/codemap.md) |

## Design and flow

- `web/package.json` declares `workspaces: ["packages/*"]` and consumes `@html-desktop-shell/shell-api` through `workspace:*`.
- `shell-api/package.json` exports ESM TypeScript source directly from `src/index.ts` and has no framework or runtime-library dependency.
- Bootstrap validation requires exact v6 host/capability shapes and five discriminated content contexts.
- Tray Menu reads/activation are bound to its immutable item ID; only that surface exposes close-only `closeTrayMenu`. No generic native authority is granted.
- Application imports resolve to the source through the workspace. Calls post `{ id, method, params }` to the host's WebKit handler and receive parsed typed results or a `NativeBridgeError` retaining native code/message.

## Integration points

- `web/src/stores/shell.ts` is the principal runtime consumer and caches host info, capabilities, state, and errors.
- `SurfaceRoot` and five views consume validated identity/capabilities; Tray Menu never requests general state.
- Desktop widgets import exact SDK operations and provider/result types; their capability hints never replace native authorization.
- The native Bridge v6 dispatcher remains authoritative for caller/item scope and parameter validation.
