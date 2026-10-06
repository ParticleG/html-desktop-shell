# `web/`

## Responsibility

This directory is the Bun workspace and Vite application root for the official Vue 3 frontend. It builds one local asset bundle used by four independent native layer-shell windows and connects those windows to Bridge v4 through the framework-independent workspace package `@html-desktop-shell/shell-api`.

## Entry points and commands

- `index.html` supplies `<div id="app">` and loads `/src/main.ts`.
- `src/main.ts` creates `App.vue`, installs Pinia, Vue I18n, and PrimeVue, imports global styles, and mounts on `#app`; `SurfaceRoot` performs direct typed dispatch.
- `package.json` declares the private Bun workspace over `packages/*` and consumes shell-api via `workspace:*`.
- `bun run build` runs Vite, `bun run typecheck` runs `vue-tsc --noEmit`, and `bun test` runs Bun tests.
- `bunfig.toml` preloads Vue SFC transformation and `src/test-setup.ts`; mounted tests run with Happy DOM and Vue Test Utils.

## Directory relationships

- [`src/`](src/codemap.md) contains root dispatch, four views, Pinia state, polling, layout, localization, styles, view-model helpers, widgets, plugins, and mounted tests.
- [`packages/`](packages/codemap.md) contains reusable workspace packages. `shell-api/` is the framework-independent Bridge v4 boundary.
- `tsconfig.json` applies strict bundler-style TypeScript checks to application and package sources; `env.d.ts` provides Vite and Vue SFC declarations.
- `vite.config.ts` uses the Vue plugin, a relative `./` asset base, and emits to repository-level `web-dist/` after clearing the prior output.

## Runtime flow

1. Each native surface window loads the same built frontend and provides the WebKit `shell` message handler.
2. `SurfaceRoot` asks the store to initialize validated HostInfo and scoped capabilities once, then directly dispatches Bar, Quick Panel, Workspace HUD, or Diagnostics from `HostInfo.surface.kind`.
3. Bar, Quick Panel, and Diagnostics own serialized one-second state polling. Workspace HUD renders only compositor-confirmed immutable surface context and performs no state polling.
4. Bar/Quick controls call exact workspace and transient-surface wrappers only when the corresponding validated capability is present. Diagnostics exposes exact unavailable reasons and reflects later provider recovery.
5. `packages/shell-api/src/index.ts` posts `{ id, method, params }`, validates v4 bootstrap results, parses replies, and preserves native error code/message. Generic transport is not generic authority; native per-surface allowlists remain the security boundary.
6. Vite writes the self-contained relative-URL bundle to `web-dist/` for native loading.

## Verification structure

- Shell-api tests cover exact Bridge v4 types, validators, wrapper payloads, and native errors.
- Root mounted tests cover all four dispatch variants and bootstrap validation failures.
- View mounted tests cover Bar/transient controls, scoped capabilities, polling lifecycle, Diagnostics reasons/recovery, and workspace actions through the real component → Pinia → shell-api path.
- Widget mounted tests cover pointer/keyboard workspace-index behavior and output-scoped active-window state; the Bar suite verifies its output label does not follow global keyboard focus. Pure view-model tests cover formatting and filtering.

## Boundaries

- **Native host:** shell-api is the typed browser adapter; native Bridge v4 owns transport installation, scoped authorization, parameter validation, surface lifetime, and state production.
- **Framework:** shell-api has no Vue/Pinia dependency. Application state and presentation live under `src/`.
- **Assets:** runtime UI, theme, icons, and styles are bundled locally; the host uses relative static asset URLs.
