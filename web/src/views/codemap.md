# `web/src/views/`

## Responsibility

This directory contains five Bridge v6 content views selected directly by `SurfaceRoot.vue`. Each trusted native content surface receives immutable context and renders exactly one view.

## View inventory

### `BarView.vue`

- Renders `#shell-bar` with the three `barLayout` regions and an exhaustive `widgetRegistry` for the eight widget keys.
- Projects Pinia refs and the workspace-focus action into shared `WidgetProps`, then starts view-owned polling.
- Shows **Open Quick Panel** and **Open Diagnostics** buttons only when the corresponding scoped capability exists. Buttons call the exact transient-open store operations.
- Fixed audio/tray/notification controls use native operations; right-click opens an independent host Tray Menu, not Quick Panel or an application's ordinary window.

### `QuickPanelView.vue`

- Renders workspace controls plus focused-window, battery, network, bridge, and action summaries.
- Owns polling and uses the same `WidgetProps` boundary as the Bar.
- Shows close and Diagnostics-open controls only for `setQuickPanelOpen` and `setDiagnosticsDialogOpen` capabilities respectively. Workspace actions use compositor workspace indexes.
- Lazily mounts Audio, System tray, Notifications, and Applications through PrimeVue tabs. Only the selected desktop control owns its local async work; the view retains one serialized parent-state poll.

### `WorkspaceHudView.vue`

- Accepts only `{ surface: WorkspaceHudSurfaceContext }` and renders its confirmed `workspaceIndex` and optional output.
- It does not read Pinia, issue bridge requests, or poll. The native surface context is immutable display input for that HUD window.

### `DiagnosticsDialogView.vue`

- Owns polling and renders validated host metadata, the exact scoped capability list, and all provider availability. Desktop providers expose native availability/count summaries, not audio controls, tray items, application paths, or notification bodies.
- Unavailable providers show their exact native `reason` when present; available providers show `available`, so stale reasons disappear after recovery on a later refresh.
- The close button exists only with `setDiagnosticsDialogOpen`; otherwise the view explicitly renders `diagnostics close unavailable`. Bridge and action errors remain visible.

### `TrayMenuView.vue`

- Accepts immutable `TrayMenuSurfaceContext` and reads only its bound item through `useTrayMenu`; it never polls `getState`.
- Reuses `TrayMenuTree` for literal labels, enabled/toggle state, lazy submenus, revision-bound activation, and keyboard navigation.
- `#tray-menu-content` is the native measurement target. Native code owns coordinates/dimensions; the view owns scrolling, wrapped errors, refresh, close, and focus.
- Successful leaf activation replies before `closeTrayMenu`; stale/error responses remain visible. Unmount retires pending menu reads.

## Data and control flow

1. `SurfaceRoot` completes one-time HostInfo/capability initialization and selects one view from `hostInfo.surface.kind`.
2. Bar, Quick Panel, and Diagnostics obtain the store and call `useShellPolling()`. Each poll awaits `refreshState()` before scheduling its next one-second timeout and stops permanently on unmount.
3. Bar and Quick Panel create `WidgetProps` from current store refs and pass the exact workspace action callback to widgets.
4. Capability flags are UI hints; native Bridge v6 remains the actual caller/item authorization boundary.
5. HUD bypasses this state path and displays only its typed surface prop.

## Mounted coverage

- `BarView.mounted.test.ts` covers exact transient-control requests, visible native failures, serialized polling, continued polling, and teardown without late scheduling.
- `QuickPanelView.mounted.test.ts` covers provider/workspace rendering, workspace focus by `workspaceIndex`, and exact close/open transient controls.
- `DiagnosticsDialogView.mounted.test.ts` covers capability-limited rendering, exact close payload, native provider reasons, and recovery to available state.
- Root-level `SurfaceRoot.mounted.test.ts` covers exhaustive dispatch of all four surface kinds, one HostInfo/capability initialization, and rejection UI for invalid HostInfo or capabilities.

## Integration points

- `../SurfaceRoot.vue` is the sole view dispatcher.
- `../stores/shell.ts` owns host information, capabilities, snapshots, errors, and exact operation actions.
- `../composables/useShellPolling.ts` owns polling for the three mutable views.
- `../layout.ts`, `../widgets/`, `../styles.css`, and `../i18n.ts` provide Bar composition and shared presentation contracts.
