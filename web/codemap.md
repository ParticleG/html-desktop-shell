# `web/`

## Responsibility

This Bun/Vite workspace builds the replaceable Vue 3 frontend for five native content surfaces using framework-independent Bridge v6 `@html-desktop-shell/shell-api`.

## Entry points and commands

- `index.html` supplies `<div id="app">` and loads `/src/main.ts`.
- `src/main.ts` creates `App.vue`, installs Pinia, Vue I18n, and PrimeVue, imports global styles, and mounts on `#app`; `SurfaceRoot` performs direct typed dispatch.
- `package.json` declares the private Bun workspace over `packages/*` and consumes shell-api via `workspace:*`.
- `bun run build` runs Vite, `bun run typecheck` runs `vue-tsc --noEmit`, and `bun test` runs Bun tests.
- `bunfig.toml` preloads Vue SFC transformation and `src/test-setup.ts`; mounted tests run with Happy DOM and Vue Test Utils.

## Directory relationships

- [`src/`](src/codemap.md) contains typed dispatch, five views, local state/polling/menu sessions, layout, localization, styles, and mounted tests.
- [`packages/`](packages/codemap.md) contains the framework-independent Bridge v6 boundary.
- `tsconfig.json` applies strict bundler-style TypeScript checks to application and package sources; `env.d.ts` provides Vite and Vue SFC declarations.
- `vite.config.ts` uses the Vue plugin, a relative `./` asset base, and emits to repository-level `web-dist/` after clearing the prior output.

## Runtime flow

1. Each native surface window loads the same built frontend and provides the WebKit `shell` message handler.
2. `SurfaceRoot` validates native identity/capabilities once and selects Bar, Quick Panel, HUD, Diagnostics, or Tray Menu.
3. Bar/Quick/Diagnostics poll state; HUD renders confirmed context; Tray Menu reads only its bound DBusMenu session.
4. Bar/Quick controls call exact workspace and transient-surface wrappers only when the corresponding validated capability is present. Diagnostics exposes exact unavailable reasons and reflects later provider recovery.
5. The SDK validates exact v6 bootstrap results, preserves native errors, and exposes only named wrappers; native allowlists remain authoritative.
6. Vite writes the self-contained relative-URL bundle to `web-dist/` for native loading.

## Verification structure

- SDK tests cover exact v6 bootstrap and native errors; mounted/native tests cover caller behavior and boundaries.
- Root mounted tests cover five variants and bootstrap failure.
- View mounted tests cover Bar/transient controls, scoped capabilities, polling lifecycle, Diagnostics reasons/recovery, and workspace actions through the real component → Pinia → shell-api path.
- Widget mounted tests cover pointer/keyboard workspace-index behavior and output-scoped active-window state; the Bar suite verifies its output label does not follow global keyboard focus. Pure view-model tests cover formatting and filtering.

## Boundaries

- **Native host:** Bridge v6 owns trust, caller/item scope, exact params, IDs/revisions, operations, and surface lifecycle.
- **Framework:** shell-api has no Vue/Pinia dependency. Application state and presentation live under `src/`.
- **Assets:** runtime UI, theme, icons, and styles are bundled locally; the host uses relative static asset URLs.
