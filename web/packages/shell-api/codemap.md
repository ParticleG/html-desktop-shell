# `web/packages/shell-api/`

## Responsibility

This workspace package publishes the framework-independent Bridge v4 browser client as `@html-desktop-shell/shell-api`. It isolates WebKit request/reply details, runtime bootstrap validation, typed native snapshots, and exact operation wrappers from the Vue application. It owns no UI, Pinia state, polling, native authorization, or transport installation.

## Package contract

- `package.json` declares an ESM package, version `0.1.0`, licensed `MIT OR Apache-2.0`, with the sole root export mapped directly to `./src/index.ts`.
- There are no package-local runtime dependencies or subpath exports; the Bun/Vite workspace consumes the TypeScript source directly.
- `src/index.ts` exports Bridge v4 `HostInfo`, the four discriminated `SurfaceContext` variants, the closed `ShellMethod` and `Capabilities` contracts, provider/snapshot types, reply/error types, validators, generic transport, and operation-specific wrappers.

## Design and flow

1. A consumer imports from the package root without depending on Vue or Pinia.
2. Bootstrap wrappers request `getHostInfo` and `getCapabilities`, then validate exact v4 shapes before application code uses surface identity or capabilities.
3. Other wrappers send only their named operation and exact parameter object: `getState`, `niriFocusWorkspace`, `setQuickPanelOpen`, or `setDiagnosticsDialogOpen`.
4. `request()` posts an ID-bearing envelope to `window.webkit.messageHandlers.shell`, parses the reply, and throws `NativeBridgeError` with the native code/message on an error response.
5. The Vue store retains validated host information and capabilities; `SurfaceRoot` dispatches the correct view and views expose only controls authorized by their scoped capability set.

## Boundaries

- Importing types is environment-neutral; invoking operations requires the host-injected WebKit handler. There is no HTTP, Node, Bun, or ordinary-browser fallback.
- Generic `request()` remains a transport utility, not generic native authority. Native Bridge v4 method allowlists and parameter checks remain the security boundary.
- See [`src/codemap.md`](src/codemap.md) for exact types, validators, wrappers, error behavior, and contract tests.

## Integration points

- `web/package.json` registers `packages/*` and consumes this package via `workspace:*`.
- `web/src/stores/shell.ts` calls its wrappers and owns reactive state/errors.
- `web/src/view-model.ts`, `SurfaceRoot.vue`, views, widgets, and tests import protocol types.
- Native Bridge v4 supplies the matching handler, scoped capabilities, exact operations, and response envelopes.
