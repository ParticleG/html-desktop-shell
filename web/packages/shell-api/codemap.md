# `web/packages/shell-api/`

## Responsibility

This package publishes the framework-independent Bridge v5 client as `@html-desktop-shell/shell-api`. It owns typed snapshots, bootstrap validation, exact operation wrappers and WebKit request/reply handling, but no UI, Pinia state, polling, native authority or transport installation.

## Package contract

- `package.json` declares an ESM package, version `0.1.0`, licensed `MIT OR Apache-2.0`, with the sole root export mapped directly to `./src/index.ts`.
- There are no package-local runtime dependencies or subpath exports; the Bun/Vite workspace consumes the TypeScript source directly.
- `src/index.ts` exports v5 host/surface/capability/provider/result/error types, bootstrap validators and 20 exact named operations, including native audio/tray/notifications/applications.

## Design and flow

1. A consumer imports from the package root without depending on Vue or Pinia.
2. Bootstrap wrappers request `getHostInfo` and `getCapabilities`, then validate exact v4 shapes before application code uses surface identity or capabilities.
3. Other wrappers send only their named operation and exact parameter object: `getState`, `niriFocusWorkspace`, `setQuickPanelOpen`, or `setDiagnosticsDialogOpen`.
4. `request()` posts an ID-bearing envelope to `window.webkit.messageHandlers.shell`, parses the reply, and throws `NativeBridgeError` with the native code/message on an error response.
5. The Vue store retains validated host information and capabilities; `SurfaceRoot` dispatches the correct view and views expose only controls authorized by their scoped capability set.

## Boundaries

- Importing types is environment-neutral; invoking operations requires the host-injected WebKit handler. There is no HTTP, Node, Bun, or ordinary-browser fallback.
- Generic `request()` is transport only. Native Bridge v5 allowlists, strict params and cache-owned identities remain the security boundary.
- See [`src/codemap.md`](src/codemap.md) for exact types, validators, wrappers, error behavior, and contract tests.

## Integration points

- `web/package.json` registers `packages/*` and consumes this package via `workspace:*`.
- `web/src/stores/shell.ts` calls its wrappers and owns reactive state/errors.
- `web/src/view-model.ts`, `SurfaceRoot.vue`, views, widgets, and tests import protocol types.
- Native Bridge v5 supplies trusted transport, scoped capabilities, exact operations and structured asynchronous replies.
