# `web/packages/shell-api/`

## Responsibility

This package publishes the framework-independent Bridge v6 client as `@html-desktop-shell/shell-api`. It owns typed snapshots, exact bootstrap validation, 21 operation wrappers, and WebKit replies; it has no UI, Pinia, polling, or native authority.

## Package contract

- `package.json` declares an ESM package, version `0.1.0`, licensed `MIT OR Apache-2.0`, with the sole root export mapped directly to `./src/index.ts`.
- There are no package-local runtime dependencies or subpath exports; the Bun/Vite workspace consumes the TypeScript source directly.
- `src/index.ts` exports v6 contracts and five surface variants, including native-bound Tray Menu read/activation/self-close.

## Design and flow

1. A consumer imports from the package root without depending on Vue or Pinia.
2. Bootstrap wrappers validate exact v6 host/capability shapes before consumers use native identity.
3. Named wrappers send only their exact operation/parameters, including host tray-menu open and close-only `closeTrayMenu`.
4. `request()` posts an ID-bearing envelope to `window.webkit.messageHandlers.shell`, parses the reply, and throws `NativeBridgeError` with the native code/message on an error response.
5. The Vue store retains validated host information and capabilities; `SurfaceRoot` dispatches the correct view and views expose only controls authorized by their scoped capability set.

## Boundaries

- Importing types is environment-neutral; invoking operations requires the host-injected WebKit handler. There is no HTTP, Node, Bun, or ordinary-browser fallback.
- Native Bridge v6 allowlists, strict parameters, and cache-owned identities—not generic transport—remain the authority boundary.
- See [`src/codemap.md`](src/codemap.md) for exact types, validators, wrappers, error behavior, and contract tests.

## Integration points

- `web/package.json` registers `packages/*` and consumes this package via `workspace:*`.
- `web/src/stores/shell.ts` calls its wrappers and owns reactive state/errors.
- `web/src/view-model.ts`, `SurfaceRoot.vue`, views, widgets, and tests import protocol types.
- Native Bridge v6 supplies scoped trusted transport and structured asynchronous replies.
