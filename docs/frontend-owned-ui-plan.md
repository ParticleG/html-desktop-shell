# Frontend-owned UI migration execution plan

## Context

The user requires moving widget display, position, hiding, and styling control from Rust config to the frontend. Rust only provides backend interfaces, provider state, and precise action bridge methods. Current code defines `WidgetLayout`/`WidgetName` in `src/config.rs`, builds `widgetsLeft`/`widgetsCenter`/`widgetsRight` into the panel URI in `src/shell_host.rs`, and moves DOM nodes from Rust query params in `web/js/app.js::applyWidgetLayout()`. That makes Rust participate in component layout that should belong to the HTML frontend.

Target state: `html-desktop-shell` core only requires a framework-agnostic native bridge API. The official project provides a Vue 3 + Vite + TypeScript sample frontend and local bundled UI/theme/icon/plugin dependencies. Developers can use any framework/build tool to generate static assets and connect to the shell.

This plan intentionally changes the prior repository invariant that prohibited framework/build tooling. New invariant: runtime still only loads local static web assets; the official sample has a reproducible Bun lockfile; no CDN or remote scripts are loaded; no Electron, X11 fallback, generic native bridge, arbitrary filesystem/process bridge, or generic native escape hatch is added.

## Approach

### 1. Define the new frontend architecture boundary: core is not bound to Vue, official sample uses Vue

- `@html-desktop-shell/shell-api` is the only stable frontend API. This package must be framework-agnostic TypeScript and must not import Vue, PrimeVue, Pinia, Router, i18n, VueUse, or Lucide.
- The official default UI uses Vue 3 + Vite + TypeScript as official sample frontend source under `web/`. It may be installed by the package as the default UI, but developers are not required to use Vue.
- Custom frontend integration is fixed: developers generate a static directory containing `index.html` and connect it through one existing asset lookup path:
  - `$HTML_DESKTOP_SHELL_WEB_DIR/index.html`
  - `$XDG_DATA_HOME/html-desktop-shell/web/index.html`
  - `~/.local/share/html-desktop-shell/web/index.html`
  - `/usr/share/html-desktop-shell/web/index.html`
- README must add a “Custom frontend contract”: any framework only needs to implement `window.webkit.messageHandlers.shell.postMessage({ id, method, params })` calls and follow the `@html-desktop-shell/shell-api` request/response schema. Rust does not care whether the frontend is Vue, React, Svelte, Solid, plain HTML, or any other build output.

### 2. Choose the official sample frontend local dependency stack

Official sample dependencies must all come from npm packages and be bundled by Vite into local `web-dist/`; no CDN, remote icon API, remote fonts, or remote theme assets.

Fixed dependency choices:

- Framework/build: Vue 3 + Vite + TypeScript + Bun.
  - Use Bun because this workstation has `bun 1.3.14` and no `node`/`npm`.
  - Use Vite because Vue official SFC workflow is Vite-based, and Vite supports `base: "./"`, producing relative asset paths suitable for WebKit local file/install directory loading.
- UI component library: PrimeVue.
  - Reason: PrimeVue is a Vue UI library with styled mode, built-in themes, dark mode, typed components, and many components; it does not require Nuxt; it can be locally bundled.
  - Usage: import only used components, not global registration of the whole library. The official sample must use at least `primevue/button` and `primevue/tag`.
  - Do not use Nuxt UI as the default sample because it couples the official sample to Nuxt/Tailwind/Reka; that violates the boundary that shell core is not bound to a frontend stack.
- Theme: PrimeVue styled mode + Aura preset.
  - Use `@primeuix/themes/aura`.
  - Configure PrimeVue with `darkModeSelector: "system"` to follow system dark mode; do not add app-persisted theme switching.
  - App-specific panel variables remain in `web/src/styles.css` for the 32px panel, translucent background, workspace pill, and overflow rules.
- Icon library: `@lucide/vue`.
  - Reason: Lucide Vue exports independent SVG Vue components, is tree-shakable, TypeScript friendly, and fully local bundled.
  - Do not use Iconify as the default sample because Iconify Vue defaults to loading icon data from the Iconify API by icon name; that violates the runtime no-remote-resources requirement. If Iconify is used later, it must use only local icon data collections with a separate plan.
- Vue plugins:
  - `pinia`: official sample state store for `hostInfo`, provider state, bridge/action errors, refresh state.
  - `vue-router`: official sample uses `createMemoryHistory()` and only memory routing, without browser/file URL reads/writes. First version only configures `/` -> `PanelView` to demonstrate mounting multiple views inside shell without Rust or file path route participation.
  - `vue-i18n`: official sample uses Composition API mode (`legacy: false`), default locale `en`. First version only commits English messages to avoid non-English UI copy in code; locale files may be added later.
  - `@vueuse/core`: official sample uses `useIntervalFn` for the 1s polling interval and avoids hand-written `setInterval` lifecycle.

`web/package.json` exact structure:

```json
{
  "private": true,
  "type": "module",
  "workspaces": ["packages/*"],
  "scripts": {
    "build": "vite build",
    "typecheck": "vue-tsc --noEmit",
    "test": "bun test"
  },
  "dependencies": {
    "@html-desktop-shell/shell-api": "workspace:*",
    "@lucide/vue": "^1.21.0",
    "@primeuix/themes": "^2.0.3",
    "@vueuse/core": "^14.3.0",
    "pinia": "^3.0.4",
    "primevue": "^4.5.5",
    "vue": "^3.5.38",
    "vue-i18n": "^11.4.6",
    "vue-router": "^4.5.0"
  },
  "devDependencies": {
    "@types/bun": "^1.3.14",
    "@vitejs/plugin-vue": "^6.0.7",
    "typescript": "^5.9.3",
    "vite": "^7.2.4",
    "vue-tsc": "^3.3.5"
  }
}
```

Implementation runs `cd web && bun install`, generating and committing `web/bun.lock`. Do not commit `web/node_modules`.

Update `.gitignore` with exactly:

```gitignore
/web/node_modules/
/web-dist/
```

### 3. Build the framework-agnostic shell API package

Add `web/packages/shell-api/package.json`:

```json
{
  "name": "@html-desktop-shell/shell-api",
  "version": "0.1.0",
  "type": "module",
  "license": "MIT OR Apache-2.0",
  "private": false,
  "exports": {
    ".": "./src/index.ts"
  }
}
```

Add `web/packages/shell-api/src/index.ts`. It replaces `web/js/bridge.js`; do not keep compatibility aliases. It must not import Vue or any official sample UI dependency.

Exact exported functions:

```ts
export async function request<T = unknown>(method: string, params?: unknown): Promise<T>;
export function getHostInfo(): Promise<HostInfo>;
export function getCapabilities(): Promise<Capabilities>;
export function getState(): Promise<ShellState>;
export function focusWorkspace(workspaceId: number): Promise<{ workspaceId: number }>;
```

Exact exported core types:

```ts
export interface HostInfo {
  shell: "html-desktop-shell";
  backend: "wayland-layer-shell";
  bridgeVersion: 2;
  panel: PanelContext;
}

export interface PanelContext {
  index: number;
  output: string | null;
}

export interface Capabilities {
  methods: string[];
}

export interface ShellState {
  clock?: { time?: string };
  host?: { backend?: string; monitorCount?: number; bridgeVersion?: number };
  niri?: NiriState;
  battery?: BatteryState;
  network?: NetworkState;
}
```

Also export provider/result shapes: `NiriState`, `NiriWorkspace`, `NiriFocusedOutputState`, `NiriFocusedWindowState`, `FocusedWindow`, `BatteryState`, `BatteryItem`, `NetworkState`, `NetworkInterfaceState`, `NativeResponse<T>`, `NativeError`. Rust may omit missing JSON fields; TypeScript fields must be optional where appropriate.

`request()` behavior must be exactly:

- If `window.webkit?.messageHandlers?.shell` is missing, throw `new Error("native bridge unavailable")`.
- Use an incrementing string id for every request and send `{ id, method, params: params ?? {} }`.
- WebKit `postMessage` returns a JSON string; parse it. If `ok === false`, throw `new Error(response.error?.message || "native bridge request failed")`.
- On success, return `response.result as T`.

### 4. Move per-panel context into native bridge and remove URL layout query

In `src/messages.rs`:

- Change `pub const BRIDGE_VERSION: u32 = 1;` to `2`.
- Add:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelContext {
    pub index: u32,
    pub output: Option<String>,
}
```

- Change `handle_native_request` signature to receive `panel_context: &PanelContext` after the two closure parameters:

```rust
pub fn handle_native_request<S, A>(
    raw: &str,
    state_snapshot: S,
    focus_workspace: A,
    panel_context: &PanelContext,
) -> String
where
    S: FnOnce() -> serde_json::Value,
    A: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>;
```

- Pass `panel_context` to `handle_request`.
- Change `getHostInfo` result exactly to:

```json
{
  "shell": "html-desktop-shell",
  "backend": "wayland-layer-shell",
  "bridgeVersion": 2,
  "panel": { "index": <u32>, "output": <string-or-null> }
}
```

- Supported methods remain exactly `getHostInfo`, `getCapabilities`, `getState`, `niriFocusWorkspace`. Do not add generic config, file, process, network, DBus, eval, or generic niri action methods.
- Update messages tests: capabilities still asserts the four methods in order; `getHostInfo` asserts `bridgeVersion == 2`, `panel.index`, and `panel.output`.

In `src/bridge.rs`:

- Change `attach_bridge` signature to:

```rust
pub fn attach_bridge(
    web_view: &webkit6::WebView,
    providers: ProviderRegistry,
    panel_context: messages::PanelContext,
) -> Result<(), &'static str>
```

- Capture `panel_context` in the callback and pass `&panel_context` to `messages::handle_native_request`.

In `src/shell_window.rs`:

- Add `panel_context: messages::PanelContext` to `shell_window_for_monitor` and pass it to `bridge::attach_bridge`.

In `src/shell_host.rs`:

- Stop importing `WidgetLayout` and `WidgetName`.
- In `panels_for_monitors`, build for each monitor:

```rust
messages::PanelContext {
    index,
    output: monitor.connector().map(|connector| connector.to_string()),
}
```

- Pass the base asset URI unchanged to `shell_window_for_monitor`; do not append `panelIndex`, `panelOutput`, `widgetsLeft`, `widgetsCenter`, or `widgetsRight`.
- Delete `panel_uri`, `push_widget_param`, and `push_url_component` if unused.
- Delete `panel_uri_includes_panel_index_and_output` and `panel_uri_escapes_output_component` tests unless helpers still need tests. Do not keep dead helpers for tests.

This lets frontend filter workspaces through `getHostInfo().panel.output` instead of URL query.

### 5. Cleanly remove Rust-owned widget layout config

In `src/config.rs`:

- Remove `widgets: WidgetLayout` from `ShellConfig`.
- Delete `WidgetLayout`, `WidgetName`, `WidgetName::as_str`, and `RawWidgetLayout`.
- Delete `widgets: Option<RawWidgetLayout>` from `RawShellConfig`.
- Delete the `raw.widgets` merge block in `parse_config`.
- Keep `#[serde(deny_unknown_fields)]` on `RawShellConfig`. After migration, `[widgets]` in Rust TOML must be a config error because Rust no longer owns UI layout.
- Delete widget layout tests:
  - `default_widget_layout_preserves_current_ui`
  - `valid_widget_layout_config_loads`
  - `unknown_widget_name_is_rejected`
  - `empty_widgets_section_preserves_default_layout`
- Keep panel shape tests (`panel_height`, `layer`, `keyboard_mode`, diagnostics, duplicate `--config`) and update `ShellConfig::default()` equality expectations after field removal.

Update config/example files:

- `packaging/html-desktop-shell.default.toml`: delete the whole `[widgets]` section, leaving only:

```toml
panel_height = 32
layer = "top"
keyboard_mode = "on-demand"
```

- `test/panel-test.toml`: delete the whole `[widgets]` section, keeping existing user values for `panel_height`, `layer`, and `keyboard_mode` if present.
- `test/panel-default.toml` already omits `[widgets]`; do not modify it unless current status shows user changes requiring a re-read.

### 6. Implement the official Vue sample frontend

`web/` becomes official sample source; `web-dist/` becomes generated runtime output.

Entry and config:

- Replace `web/index.html` with Vite entry:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>HTML Shell</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

- Delete `web/js/app.js`, `web/js/bridge.js`, `web/shell.css`. Do not keep compatibility imports.
- Add `web/vite.config.ts`:

```ts
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  base: "./",
  plugins: [vue()],
  build: {
    outDir: "../web-dist",
    emptyOutDir: true,
  },
});
```

- Add `web/tsconfig.json`, enabling strict TypeScript, DOM libs, `moduleResolution: "Bundler"`, `jsx: "preserve"`, `types: ["vite/client", "bun"]`, and include `src/**/*.ts`, `src/**/*.vue`, `packages/**/*.ts`.
- Add `web/env.d.ts` with `/// <reference types="vite/client" />` and `declare module "*.vue"`.

Vue app wiring:

- Add `web/src/main.ts`: create Vue app, install Pinia, Router, i18n, PrimeVue in that order, then mount `#app`.
- Add `web/src/plugins/primevue.ts`:

```ts
import type { App } from "vue";
import PrimeVue from "primevue/config";
import Aura from "@primeuix/themes/aura";

export function installPrimeVue(app: App): void {
  app.use(PrimeVue, {
    theme: {
      preset: Aura,
      options: {
        darkModeSelector: "system",
        cssLayer: false,
      },
    },
  });
}
```

- Add `web/src/i18n.ts`: use `createI18n({ legacy: false, locale: "en", fallbackLocale: "en", messages })`. First version only commits English messages. Message keys must include at least: `app.name`, `widgets.workspaces`, `states.workspacesUnavailable`, `states.workspacesNone`, `states.windowUnavailable`, `states.noFocusedWindow`, `actions.workspaceSwitchFailed`, `bridge.unavailable`.
- Add `web/src/router.ts`:

```ts
import { createMemoryHistory, createRouter } from "vue-router";

export const router = createRouter({
  history: createMemoryHistory(),
  routes: [
    { path: "/", name: "panel", component: () => import("./views/PanelView.vue") },
  ],
});
```

- Add `web/src/App.vue`: only render `<RouterView />`. Do not put panel layout in `App.vue`.

State/store/composables:

- Add `web/src/stores/shell.ts` using Pinia setup store `defineStore("shell", () => { ... })`.
- Store state refs:
  - `hostInfo: Ref<HostInfo | null>`
  - `state: Ref<ShellState | null>`
  - `bridgeError: Ref<string>`
  - `actionError: Ref<string>`
  - `actionErrorDetail: Ref<string>`
  - `isRefreshing: Ref<boolean>`
- Store actions:
  - `async loadHostInfo(): Promise<void>`: call `getHostInfo()`; on failure set `bridgeError.value = "bridge: unavailable"`.
  - `async refreshState(): Promise<void>`: call `getState()`; on success update `state` and clear `bridgeError`; on failure set `bridgeError.value = "bridge: unavailable"`; use `isRefreshing` to prevent concurrent refresh.
  - `async focusWorkspaceFromButton(workspaceId: number): Promise<void>`: clear action error, call `focusWorkspace(workspaceId)`; on failure set `actionError.value = "workspace switch failed"`, and `actionErrorDetail.value` to the thrown error message.
  - `clearActionError(): void`: clear `actionError` and `actionErrorDetail`.
- Add `web/src/composables/useShellPolling.ts`: use VueUse `useIntervalFn` for 1000ms polling. On mounted, sequentially call `shell.loadHostInfo()`, `shell.refreshState()`, then `resume()`; on unmounted call `pause()`. Do not write a long-lived `setInterval` by hand.

Layout/view-model:

- Add `web/src/layout.ts` as official sample only default layout owner:

```ts
export type WidgetKey =
  | "appName"
  | "workspaces"
  | "focusedWindow"
  | "clock"
  | "battery"
  | "network"
  | "actionStatus"
  | "bridgeStatus";

export interface PanelLayout {
  left: WidgetKey[];
  center: WidgetKey[];
  right: WidgetKey[];
}

export const panelLayout: PanelLayout = {
  left: ["appName", "workspaces", "focusedWindow"],
  center: ["clock"],
  right: ["battery", "network", "actionStatus", "bridgeStatus"],
};
```

Developers change widget display/position by modifying this file or the component tree, not Rust config.

- Add `web/src/view-model.ts` with pure helpers for components/tests:
  - `visibleWorkspaces(state: ShellState | null | undefined, panelOutput: string | null | undefined): NiriWorkspace[]`
    - Return `[]` when `niri/workspaces/items` is unavailable or not an array.
    - If `panelOutput` is a non-empty string, return only items with `workspace.output === panelOutput`; otherwise return all items.
  - `workspaceLabel(workspace: NiriWorkspace): string`
    - Prefer non-empty string `name`; then integer `index`; then integer `id`; otherwise `"?"`.
  - `workspaceTitle(workspace: NiriWorkspace): string`
    - Return exactly `workspace ${label} on ${output || "unknown output"}`.
  - `focusedWindowText(windowState: NiriFocusedWindowState | undefined): string`
    - Unavailable: `"window: unavailable"`; `window` null/undefined: `"no focused window"`; appId/title present and different: `${appId} — ${title}`; otherwise title or appId or `"focused window"`.
  - `batteryText(battery: BatteryState | undefined): string | null`
    - Unavailable returns `null`; otherwise `bat: ${percentage ?? "?"}%${status ? " " + status : ""}`.
  - `networkText(network: NetworkState | undefined): string`
    - Unavailable: `"net: unavailable"`; otherwise keep current `up > 0 ? "up" : "down"` behavior and produce `net: wired up/down · wifi up/down`; no wired/wireless counts returns `"net: unknown"`.
  - `bridgeStatusText(state: ShellState | null | undefined): string`
    - Keep current text shape: `bridge: <backend>`, optional `monitors: <n>`, optional `niri: <focusedOutput.name>` or `niri: unavailable`.

Panel/components:

- Add `web/src/views/PanelView.vue`:
  - Call `useShellPolling()`.
  - Render `main#shell-bar` with `aria-label="Desktop shell panel"`.
  - Render three `section`s: `#left-widgets`, `#center-widgets`, `#right-widgets`.
  - For each region, iterate over `panelLayout[region]` and select a component from `widgetRegistry`.
  - Pass widget props: `state`, `hostInfo`, `bridgeError`, `actionError`, `actionErrorDetail`, `focusWorkspaceFromButton`.
- Add `web/src/widgets/AppNameWidget.vue`: use `useI18n()`, render `t("app.name")`.
- Add `web/src/widgets/ClockWidget.vue`: render `state?.clock?.time || "--:--:--"`, may use Lucide `Clock` icon.
- Add `web/src/widgets/WorkspacesWidget.vue`:
  - Use `visibleWorkspaces(state, hostInfo?.panel.output)`.
  - Unavailable displays `workspaces: unavailable`; empty list displays `workspaces: none`.
  - Workspace button uses PrimeVue `Button`, class includes `workspace-item`, and adds `is-active`, `is-focused` by state.
  - Use Lucide `Monitor` icon as prefix or button icon.
  - Preserve current input behavior: left `pointerdown` + `preventDefault` triggers; `Enter`/space `keydown` + `preventDefault` triggers.
  - Set `aria-current="true"` when focused.
- Add `web/src/widgets/FocusedWindowWidget.vue`: render `focusedWindowText(state?.niri?.focusedWindow)`, title equals visible text, may use Lucide `Terminal` icon.
- Add `web/src/widgets/BatteryWidget.vue`: render nothing when `batteryText(state?.battery)` is null; otherwise render the text with PrimeVue `Tag`; title joins each battery detail `${name}: ${percentage}% ${status}` with ` · `; may use Lucide `Battery`/`BatteryCharging` icon.
- Add `web/src/widgets/NetworkWidget.vue`: render `networkText(state?.network)` with PrimeVue `Tag`; title joins interfaces as `${name}: ${kind} ${state}`; may use Lucide `Wifi`/`WifiOff` icon.
- Add `web/src/widgets/ActionStatusWidget.vue`: render nothing when `actionError` is empty; otherwise render text `workspace switch failed`, `role="status"`, `aria-live="polite"`, title from `actionErrorDetail`; may use Lucide `AlertTriangle` icon.
- Add `web/src/widgets/BridgeStatusWidget.vue`: render `bridgeError || bridgeStatusText(state)`.

Styles:

- Add `web/src/styles.css`, porting `web/shell.css` and adapting to Vue/PrimeVue classes. Preserve observable behavior:
  - `body` transparent, no margin, overflow hidden;
  - `#shell-bar` full width/height, adapted to the 32px top panel, translucent background;
  - `#center-widgets` absolute centered: `left: 50%; top: 50%; transform: translate(-50%, -50%)`;
  - `#right-widgets` flex, right aligned, overflow clipped;
  - `.workspace-item:focus-visible` visible outline;
  - `.workspace-item.is-focused` uses light background and dark text;
  - focused-window text ellipsis;
  - PrimeVue `Button`/`Tag` padding/height must not stretch panel height.

### 7. Provide only a plain HTML bridge demo as an example

Add `examples/plain-html/` as non-runtime sample. It is not used by Rust asset lookup, not packaged as default UI, and not referenced by `src/assets.rs`.

Files:

- `examples/plain-html/index.html`: minimal static page loading `shell.css` and `js/app.js`, with `#app` root and a comment saying it is only a bridge demo.
- `examples/plain-html/js/bridge.js`: before deleting `web/js/bridge.js`, copy the current bridge wrapper behavior, including `request`, `getHostInfo`, `getCapabilities`, `getState`, and `focusWorkspace`.
- `examples/plain-html/js/app.js`: minimal demo calling `getHostInfo()` and `getState()`, then rendering backend, bridge version, clock, monitor count, and niri availability text. Do not reimplement full panel or widget layout.
- `examples/plain-html/shell.css`: minimal readable demo styles, not full panel CSS.

### 8. Update runtime asset lookup and packaging for built frontend assets

In `src/assets.rs`:

- Change development/current checkout candidates from `web/index.html` to `web-dist/index.html`.
- Keep `$HTML_DESKTOP_SHELL_WEB_DIR/index.html` as the first-priority override path.
- Keep XDG/local/installed runtime directories as `.../html-desktop-shell/web/index.html`, because installed assets still live in `web`.
- Update missing-asset error tests: current directory and manifest candidates expect `web-dist/index.html`.
- Keep or add a test proving `$HTML_DESKTOP_SHELL_WEB_DIR` still has priority over generated assets.

In `packaging/PKGBUILD`:

- Add `bun` to `makedepends`.
- In `build()`, build frontend before Cargo release build:

```bash
cd "$srcdir/$pkgname/web"
bun install --frozen-lockfile
bun run typecheck
bun test
bun run build
cd "$srcdir/$pkgname"
cargo build --release --locked
```

- In `package()`, replace copying `web` with copying `web-dist` to the installed web asset directory:

```bash
install -dm755 "$pkgdir/usr/share/html-desktop-shell/web"
cp -a web-dist/. "$pkgdir/usr/share/html-desktop-shell/web/"
```

- Continue installing README, service, niri snippet, default config, and licenses.

In README local install instructions, replace Cargo-only build with:

```bash
cd ~/coding/RustroverProjects/html-desktop-shell
cd web
bun install
bun run typecheck
bun test
bun run build
cd ..
cargo build --release --locked
install -Dm755 target/release/html-desktop-shell "$HOME/.local/bin/html-desktop-shell"
mkdir -p "$HOME/.local/share/html-desktop-shell/web"
cp -a web-dist/. "$HOME/.local/share/html-desktop-shell/web/"
install -Dm644 packaging/html-desktop-shell.default.toml "$HOME/.config/html-desktop-shell/config.toml"
```

### 9. Update docs and repository conventions to match the new architecture

Update `AGENTS.md`:

- Replace “Plain HTML/CSS/JS only. No bundler, framework, package manager...” with the new rule: core runtime loads local static assets; official sample uses Vue 3 + TypeScript + Vite, source is in `web/`, Bun lockfile is committed, production assets are generated to `web-dist/`, and CDN/remote scripts are not allowed.
- Replace “DOM IDs are stable integration points” with: stable frontend API is framework-agnostic `@html-desktop-shell/shell-api`; official sample component names/layout are in `web/src/layout.ts`; Rust does not define widget placement.
- Keep the security boundary: no generic bridge methods and no filesystem/process/network/DBus/clipboard/screenshot/session/eval access.

Update `docs/html-desktop-shell-feature-roadmap.md` baseline/invariants:

- Change baseline from “local ES modules under `web/js/` with no frontend build step” to “framework-agnostic native bridge API plus official Vue 3 sample frontend under `web/`; local production assets built to `web-dist/` and installed as web assets”.
- Delete the invariant prohibiting framework/Node/Bun/npm/generated assets; replace it with “frontend build artifacts are local, reproducible through Bun lockfile, and no remote runtime resources are loaded.”
- Keep layer-shell and deny-by-default bridge invariants unchanged.

Update `README.md`:

- Configuration section: remove `[widgets]` from default TOML and state that Rust config only controls native panel shape (`panel_height`, `layer`, `keyboard_mode`). Widget layout is owned by the frontend; official sample layout is in `web/src/layout.ts`.
- Frontend integration section: Vue 3 + Vite is official sample, not a core requirement; custom frontend only needs to implement/use native bridge API and generate static assets.
- Official sample stack section: record PrimeVue + Aura, Lucide, Pinia, Vue Router memory history, vue-i18n, and VueUse purposes plus the “no CDN/remote icon API” constraint.
- Web assets section: explain `web/` source, `web-dist/` runtime output, and asset lookup.
- Native bridge section: record that `getHostInfo` now returns `panel.index` and `panel.output`; `bridgeVersion` is `2`.
- Provider state section: explain official sample consumes provider snapshots through `@html-desktop-shell/shell-api`; provider JSON shape remains backend-owned.
- Build/package verification section: include frontend, Rust, packaging, and current-niri smoke commands.

### 10. Add frontend tests and update Rust tests

Add `web/src/view-model.test.ts` using Bun test runner, not Vitest.

Test cases:

- `visibleWorkspaces` filters by panel output: input `{index:1, output:"eDP-1"}`, `{index:2, output:"DP-2"}`, `panelOutput = "eDP-1"` returns only index `1`.
- When `panelOutput` is `null` or `""`, `visibleWorkspaces` returns all workspaces.
- `workspaceLabel` returns `name`, then `index`, then `id`, then `?`.
- `focusedWindowText` returns `Terminal — cargo test` for `{appId:"Terminal", title:"cargo test"}`, returns `no focused window` for `window:null`, and returns `window: unavailable` for unavailable state.
- `batteryText` returns `bat: 87% discharging` for available battery and `null` for `{available:false, reason:"no battery"}`.
- `networkText` returns `net: wired up · wifi down` for wired up and wireless down counts, and `net: unavailable` for unavailable network.
- `bridgeStatusText` keeps current status text shape: backend, monitor count, focused output.

Update Rust tests:

- `src/config.rs` tests no longer mention widgets, and add or keep a test proving `[widgets]` is rejected as an unknown field.
- `src/messages.rs` tests assert `bridgeVersion == 2` and `getHostInfo` panel context.
- `src/assets.rs` tests assert `web-dist/index.html` lookup.

## Critical files and anchors

- `src/config.rs` — `ShellConfig`, `RawShellConfig`, `parse_config`, widget-layout tests; remove Rust widget ownership here.
- `src/shell_host.rs` / `src/shell_window.rs` / `src/bridge.rs` — panel URI and bridge attachment path; remove widget query params and move per-panel context into `getHostInfo`.
- `src/messages.rs` — native wire protocol; bridge version becomes `2`, `getHostInfo` adds `panel`, method allowlist remains precise.
- `web/js/app.js`, `web/js/bridge.js`, `web/shell.css`, `web/index.html` — current imperative UI; replace with official Vue sample source and framework-agnostic shell API package.
- `src/assets.rs` and `packaging/PKGBUILD` — runtime/install asset path; switch from source `web/` to generated `web-dist/`.

## Verification

Unless otherwise noted, run commands from repository root.

1. Frontend dependency, typecheck, test, build:

```bash
cd web
bun install
bun run typecheck
bun test
bun run build
cd ..
```

Expected: `web/bun.lock` exists; tests pass; `web-dist/index.html` exists; `web-dist/assets/` contains generated JS/CSS; `web-dist` has no remote CDN references. Do not commit `web-dist` or `web/node_modules`.

2. Rust checks:

```bash
cargo fmt
cargo test
cargo build --release --locked
```

Expected: all Rust tests pass; `cargo test` includes updated config/messages/assets tests.

3. Packaging metadata checks:

```bash
(cd packaging && makepkg --printsrcinfo)
(cd packaging && makepkg --verifysource)
```

Expected: `.SRCINFO` output lists `bun` and `cargo` as `makedepends`; license/url/package metadata remains valid; source verification still skips local source checksum as before.

4. Browser/component behavior smoke with mocked bridge:

- Serve built assets:

```bash
python -m http.server 8765 --directory web-dist
```

- Before page load in Chromium/browser test, inject:

```js
window.webkit = {
  messageHandlers: {
    shell: {
      postMessage: async (request) => JSON.stringify({
        id: String(request.id),
        ok: true,
        result: request.method === "getHostInfo"
          ? { shell: "html-desktop-shell", backend: "wayland-layer-shell", bridgeVersion: 2, panel: { index: 0, output: "eDP-1" } }
          : request.method === "getState"
            ? {
                clock: { time: "12:34:56" },
                host: { backend: "wayland-layer-shell", monitorCount: 2, bridgeVersion: 2 },
                niri: {
                  available: true,
                  focusedOutput: { available: true, name: "eDP-1" },
                  workspaces: { available: true, items: [
                    { id: 1, index: 1, output: "eDP-1", isActive: true, isFocused: true },
                    { id: 2, index: 2, output: "DP-2", isActive: true, isFocused: false }
                  ] },
                  focusedWindow: { available: true, window: { appId: "Terminal", title: "cargo test" } }
                },
                battery: { available: true, percentage: 87, status: "discharging", batteries: [{ name: "BAT0", percentage: 87, status: "discharging" }] },
                network: { available: true, wired: { up: 1, down: 0 }, wireless: { up: 0, down: 1 }, interfaces: [] }
              }
            : request.method === "niriFocusWorkspace"
              ? { workspaceId: request.params.workspaceId }
              : { methods: ["getHostInfo", "getCapabilities", "getState", "niriFocusWorkspace"] }
      })
    }
  }
};
```

- After page load, expected DOM: only one `eDP-1` workspace button is displayed; `DP-2` workspace button is not displayed; focused window text is `Terminal — cargo test`; battery text is `bat: 87% discharging`; network text is `net: wired up · wifi down`; bridge status contains `bridge: wayland-layer-shell`, `monitors: 2`, `niri: eDP-1`.
- Clicking workspace button sends exactly `{ method: "niriFocusWorkspace", params: { workspaceId: 1 } }`.

5. Current niri runtime smoke:

```bash
HTML_DESKTOP_SHELL_WEB_DIR="$PWD/web-dist" ./target/release/html-desktop-shell --config ./test/panel-default.toml
```

Manual expected result: each monitor shows a panel; workspaces are filtered through that monitor's `getHostInfo().panel.output`; focused window, battery/network, clock, and bridge status render normally; clicking workspace switches focus; WebKit load failures/logs do not contain `widgetsLeft`/`widgetsRight` query params.

6. Existing smoke helper using built release binary:

```bash
HTML_DESKTOP_SHELL_BIN=./target/release/html-desktop-shell HTML_DESKTOP_SHELL_WEB_DIR="$PWD/web-dist" scripts/smoke-current-niri.sh
```

Expected: `niri msg -j layers` shows one `html-desktop-shell-panel-<index>` top-layer surface for every detected monitor.

7. Raw TTY/layer-shell boundary remains unchanged: without a Wayland layer-shell compositor, running the binary must still fail instead of opening a normal window. Frontend framework code must not add Electron/browser fallback.

## Assumptions and contingencies

- Official sample uses Vue 3 + Vite, but core shell is not bound to Vue. If the official sample must become React, discard the Vue sample part of this plan and write a React-specific plan; do not translate Vue files to React during implementation because UI library, router/store/i18n/plugin choices all change.
- This migration does not add frontend preference persistence. Layout is owned by source code in `web/src/layout.ts`; if user-editable persistent UI preferences are needed later, add precise scoped bridge methods such as `getUiPreferences`/`setUiPreferences` targeting one JSON file under the app config directory with size limits and object-only validation. Do not add generic `readFile`/`writeFile`.
- Runtime assets build to `web-dist/` and are git-ignored. If the app must run before frontend assets are built, set `HTML_DESKTOP_SHELL_WEB_DIR` to an already built directory; do not make Rust fallback to Vite source `web/index.html`, because WebKit cannot directly run `.vue`/TypeScript source.
- The current repo may have user-uncommitted changes to `test/niri-kvm-guest.kdl`, `test/niri-tty2-host.kdl`, or `test/panel-test.toml`. Before editing any test fixture, inspect the worktree and preserve all changes unrelated to removing Rust-owned `[widgets]` layout.
- If `bun install` cannot access the registry or cannot generate `web/bun.lock`, stop and report package registry access as blocker. Do not manually vendor Vue/Vite/PrimeVue and do not use CDN scripts.
