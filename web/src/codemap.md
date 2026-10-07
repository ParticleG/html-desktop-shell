# `web/src/`

## Responsibility

This directory is the Vue 3 reference application for four independent Bridge v5 layer-shell surfaces: Bar, Quick Panel, Workspace HUD, and Diagnostics Dialog. It bootstraps shared services, performs typed direct surface dispatch, owns surface-local presentation state and per-view polling, and supplies native desktop controls, layout, styles, localization, and mounted integration coverage.

## Bootstrap and surface dispatch

- `main.ts` creates `App`, installs Pinia and i18n, applies `installPrimeVue`, imports `styles.css`, and mounts on `#app`.
- `App.vue` renders `SurfaceRoot` directly.
- `SurfaceRoot.vue` calls `shell.loadHostInfo()` on mount. The store deduplicates this one-time concurrent `getHostInfo()`/`getCapabilities()` initialization.
- A computed switch over validated `hostInfo.surface.kind` maps `bar`, `quickPanel`, `workspaceHud`, and `diagnosticsDialog` directly to their four view components. The HUD alone receives its typed surface context as a prop.
- Until a valid surface exists, the root renders localized loading text or the bridge/validation error. Unknown or malformed bootstrap results never select a view.

## State, capabilities, and polling

- `stores/shell.ts` caches validated HostInfo/capabilities, latest `ShellState`, bridge/action errors, and refresh status. It exposes exact workspace, Quick Panel, and Diagnostics operations plus `hasCapability(ShellMethod)`.
- Capabilities are surface-scoped UI authority hints: views omit unavailable controls, while native Bridge v5 remains the security and parameter-validation boundary.
- `composables/useShellPolling.ts` is owned by Bar, Quick Panel, and Diagnostics. It performs an immediate refresh, awaits completion, then schedules the next one-second timeout; disposal clears the timer and prevents an in-flight refresh from rescheduling.
- `composables/useNativeAction.ts` owns local desktop-control busy/error state and rejects late post-unmount results. Desktop providers own authoritative state and action validation.
- Workspace HUD never polls or reads the store after dispatch. It renders the compositor-confirmed immutable `workspaceIndex` and optional output from `WorkspaceHudSurfaceContext`.

## Layout and presentation

- `layout.ts` defines `WidgetKey`, `BarLayout`, and `barLayout`: app/workspaces/focused window left, clock center, and battery/network/action/bridge status right.
- `views/BarView.vue` pairs that layout with an exhaustive widget registry, fixed audio/tray/notification controls, and capability-gated Quick Panel/Diagnostics entry points.
- `views/QuickPanelView.vue` composes workspace/provider widgets with lazy PrimeVue Audio, System tray, Notifications, and Applications tabs plus close/Diagnostics controls.
- `views/DiagnosticsDialogView.vue` lists host/capability data and provider availability/count summaries. New providers never expose full audio/tray/notification/application state here.
- `view-model.ts` remains a pure presentation layer for output-scoped workspaces, active windows, and fixed Niri output labels, plus defensive battery/network/bridge text. Window selection reads `niri.focusedWindows.items`; output identity comes from `HostInfo.surface.output`.
- `i18n.ts` supplies English labels for surfaces, Bar regions, diagnostics, desktop controls, native unavailable/error states, and transient actions.
- `styles.css` owns full-viewport surfaces, widgets, desktop controls, canvas icons, overflow, focus treatment, and PrimeVue overrides. Desktop Tabs use scoped dark-surface tokens independently of the system theme.

## Data and control flow

1. The native window loads the bundle; `main.ts` mounts `App` and `SurfaceRoot`.
2. Root initialization requests and validates exact Bridge v5 HostInfo and capabilities once.
3. `SurfaceRoot` selects a view directly from the discriminated surface context.
4. Mutable views own polling and render store snapshots. HUD renders only its surface prop.
5. Bar and Quick Panel pass shared `WidgetProps` to leaf widgets. Workspace input uses the positive compositor `workspace.index`, not workspace ID.
6. Capability-gated controls call operation-specific store actions, which use shell-api's exact wrappers and preserve visible native failure details.
7. Desktop widgets use the exact SDK operations with native-owned IDs and revisions. Only presentation reads are debounced/coalesced; native providers own business state, authorization, and launch authority.
8. Diagnostics polling updates provider states so exact failure reasons and later recovery are reflected without remounting.

## Tests

- `SurfaceRoot.mounted.test.ts` mounts all four variants, verifies one bootstrap pair per mount and direct dispatch, and verifies invalid HostInfo/capabilities remain in failure UI.
- `views/BarView.mounted.test.ts`, `QuickPanelView.mounted.test.ts`, and `DiagnosticsDialogView.mounted.test.ts` cover exact action envelopes, scoped controls, polling lifecycle, provider reasons, and recovery.
- `widgets/WorkspacesWidget.mounted.test.ts` covers pointer/keyboard workspace-index actions and unavailable/empty rendering.
- `widgets/FocusedWindowWidget.mounted.test.ts` covers distinct output titles and missing/null/unavailable windows. The mounted Bar suite verifies output labels remain fixed across global keyboard-focus changes.
- Mounted audio, tray, notification, and launcher suites cover exact payloads, native failures/unavailability, literal text, keyboard interactions, retired reads, catalog/menu invalidation, and teardown.
- `view-model.test.ts` covers pure formatting/filtering and workspace-index selection. `test-setup.ts` registers Happy DOM for mounted suites.

## Direct child maps

| Directory | Responsibility | Detailed map |
| --- | --- | --- |
| `composables/` | Mutable-view polling and local desktop-action lifecycle. | [Composables](composables/codemap.md) |
| `plugins/` | PrimeVue installation and theme policy. | [Plugins](plugins/codemap.md) |
| `stores/` | Bridge v5 bootstrap, state, capabilities, errors, and exact surface/workspace actions. | [Stores](stores/codemap.md) |
| `views/` | Four typed surface views. | [Views](views/codemap.md) |
| `widgets/` | Bar/status renderers, desktop controls, canvas icons, and shared status props. | [Widgets](widgets/codemap.md) |
