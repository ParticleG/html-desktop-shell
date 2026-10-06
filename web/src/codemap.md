# `web/src/`

## Responsibility

This directory is the Vue 3 application for four independent Bridge v4 layer-shell surfaces: Bar, Quick Panel, Workspace HUD, and Diagnostics Dialog. It bootstraps shared services, performs typed direct surface dispatch, owns application state and per-view polling, and supplies layout, widgets, styles, localization, and mounted integration coverage.

## Bootstrap and surface dispatch

- `main.ts` creates `App`, installs Pinia and i18n, applies `installPrimeVue`, imports `styles.css`, and mounts on `#app`.
- `App.vue` renders `SurfaceRoot` directly.
- `SurfaceRoot.vue` calls `shell.loadHostInfo()` on mount. The store deduplicates this one-time concurrent `getHostInfo()`/`getCapabilities()` initialization.
- A computed switch over validated `hostInfo.surface.kind` maps `bar`, `quickPanel`, `workspaceHud`, and `diagnosticsDialog` directly to their four view components. The HUD alone receives its typed surface context as a prop.
- Until a valid surface exists, the root renders localized loading text or the bridge/validation error. Unknown or malformed bootstrap results never select a view.

## State, capabilities, and polling

- `stores/shell.ts` caches validated HostInfo/capabilities, latest `ShellState`, bridge/action errors, and refresh status. It exposes exact workspace, Quick Panel, and Diagnostics operations plus `hasCapability(ShellMethod)`.
- Capabilities are surface-scoped UI authority hints: views omit unavailable controls, while native Bridge v4 remains the security and parameter-validation boundary.
- `composables/useShellPolling.ts` is owned by Bar, Quick Panel, and Diagnostics. It performs an immediate refresh, awaits completion, then schedules the next one-second timeout; disposal clears the timer and prevents an in-flight refresh from rescheduling.
- Workspace HUD never polls or reads the store after dispatch. It renders the compositor-confirmed immutable `workspaceIndex` and optional output from `WorkspaceHudSurfaceContext`.

## Layout and presentation

- `layout.ts` defines `WidgetKey`, `BarLayout`, and `barLayout`: app/workspaces/focused window left, clock center, and battery/network/action/bridge status right.
- `views/BarView.vue` pairs that layout with an exhaustive widget registry and capability-gated Quick Panel/Diagnostics controls.
- `views/QuickPanelView.vue` composes workspace and provider widgets with capability-gated close and Diagnostics controls.
- `views/DiagnosticsDialogView.vue` lists host/capability data and provider availability. Unavailable providers show exact reasons; successful later state refreshes replace reasons with `available`.
- `view-model.ts` remains a pure presentation layer for output-scoped workspaces, active windows, and fixed Niri output labels, plus defensive battery/network/bridge text. Window selection reads `niri.focusedWindows.items`; output identity comes from `HostInfo.surface.output`.
- `i18n.ts` supplies English labels for all four surfaces, Bar regions, diagnostics fields, loading/unavailable states, and transient actions.
- `styles.css` defines transparent full-viewport roots and distinct `.surface-bar`, `.surface-quick-panel`, `.surface-workspace-hud`, `.surface-diagnostics`, and loading layouts. It also owns widget spacing, focus treatment, status colors, responsive overflow, HUD typography, diagnostics grids, and PrimeVue overrides.

## Data and control flow

1. The native window loads the bundle; `main.ts` mounts `App` and `SurfaceRoot`.
2. Root initialization requests and validates exact Bridge v4 HostInfo and capabilities once.
3. `SurfaceRoot` selects a view directly from the discriminated surface context.
4. Mutable views own polling and render store snapshots. HUD renders only its surface prop.
5. Bar and Quick Panel pass shared `WidgetProps` to leaf widgets. Workspace input uses the positive compositor `workspace.index`, not workspace ID.
6. Capability-gated controls call operation-specific store actions, which use shell-api's exact wrappers and preserve visible native failure details.
7. Diagnostics polling updates provider states so exact failure reasons and later recovery are reflected without remounting.

## Tests

- `SurfaceRoot.mounted.test.ts` mounts all four variants, verifies one bootstrap pair per mount and direct dispatch, and verifies invalid HostInfo/capabilities remain in failure UI.
- `views/BarView.mounted.test.ts`, `QuickPanelView.mounted.test.ts`, and `DiagnosticsDialogView.mounted.test.ts` cover exact action envelopes, scoped controls, polling lifecycle, provider reasons, and recovery.
- `widgets/WorkspacesWidget.mounted.test.ts` covers pointer/keyboard workspace-index actions and unavailable/empty rendering.
- `widgets/FocusedWindowWidget.mounted.test.ts` covers distinct output titles and missing/null/unavailable windows. The mounted Bar suite verifies output labels remain fixed across global keyboard-focus changes.
- `view-model.test.ts` covers pure formatting/filtering and workspace-index selection. `test-setup.ts` registers Happy DOM for mounted suites.

## Direct child maps

| Directory | Responsibility | Detailed map |
| --- | --- | --- |
| `composables/` | Mutable-view polling lifecycle and serialization. | [Composables](composables/codemap.md) |
| `plugins/` | PrimeVue installation and theme policy. | [Plugins](plugins/codemap.md) |
| `stores/` | Bridge v4 bootstrap, state, capabilities, errors, and exact actions. | [Stores](stores/codemap.md) |
| `views/` | Four typed surface views. | [Views](views/codemap.md) |
| `widgets/` | Bar/Quick leaf renderers and shared widget contract. | [Widgets](widgets/codemap.md) |
