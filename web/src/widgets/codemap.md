# `web/src/widgets/`

## Responsibility

This directory contains reusable Bar and Quick Panel presentation and desktop controls. Views own bootstrap and polling. Existing status widgets receive immutable `WidgetProps`; desktop controls read the surface-local store and call exact shell-api operations. Rust remains authoritative for provider state, IDs, permissions, and actions.

## Shared contract

`types.ts` defines:

- `WidgetProps`: latest `ShellState`, validated `HostInfo`, bridge/action status strings, and `focusWorkspaceFromButton(workspaceIndex)`.
- `workspaceIndexForFocus(workspace)`: returns only a positive integer `workspace.index`, never the compositor workspace ID, or `null` for an invalid action target.

`hostInfo.surface.output` scopes visible workspaces, active-window selection, and the fixed Niri output label for both Bar and Quick Panel surfaces. Global keyboard focus never chooses a widget's output.

## Widget inventory

| Component | Role |
| --- | --- |
| `AppNameWidget.vue` | Localized application name. |
| `WorkspacesWidget.vue` | Output-filtered workspace list, unavailable/empty states, and primary-pointer/Enter/Space focus actions using `workspace.index`. |
| `FocusedWindowWidget.vue` | Selects the matching `niri.focusedWindows.items` output, with defensive unavailable/no-window states and a tooltip. |
| `ClockWidget.vue` | Clock snapshot with `--:--:--` fallback. |
| `BatteryWidget.vue` | PrimeVue tag hidden when unavailable; charging state and per-battery tooltip when present. |
| `NetworkWidget.vue` | PrimeVue status tag, up/off icon, summary, and interface tooltip. |
| `ActionStatusWidget.vue` | Live action-error summary with native detail in its title. |
| `BridgeStatusWidget.vue` | Bridge error with precedence over the backend/monitor summary; its Niri label uses the fixed surface output, not global focused output. |
| `AudioBarWidget.vue` | Default-output volume/mute summary and Quick Panel entry. |
| `AudioControls.vue` | Integer volume writes, mute control, fresh native results, and amplified/unavailable/error presentation. |
| `TrayBarWidget.vue` | Current non-passive tray items, attention/overlay icons, primary/secondary activation, scroll, and Quick Panel menu entry. |
| `TrayControls.vue` | Separate primary/secondary/native-menu operations and revision-bound DBusMenu preparation/activation inside Quick Panel. |
| `TrayMenuTree.vue` | Recursive visible menus, disabled/separator/toggle/submenu rendering, and keyboard navigation. |
| `NotificationBarWidget.vue` | Session unread count and Quick Panel entry. |
| `NotificationCenter.vue` | Literal session history, read/dismiss/sender-action controls, and serialized history refresh from parent snapshots. |
| `ApplicationLauncher.vue` | Debounced installed-application search, selected-ID preservation, keyboard launch, and catalog-refresh/error handling. |
| `ShellIcon.vue` | Bounded RGBA pixels rendered on canvas with a local fallback; no image URLs or SVG markup. |

## Design and flow

- Components use `<script setup lang="ts">`, computed projections, Lucide icons, PrimeVue controls/tags, and local vue-i18n messages.
- Shared domain formatting stays in `../view-model.ts`: workspace filtering/labels/titles and focused-window, battery, network, and bridge summaries.
- `BarView.vue` renders the eight configured status widgets plus fixed audio/tray/notification controls. `QuickPanelView.vue` lazily mounts Audio, System tray, Notifications, or Applications through PrimeVue tabs.
- `WorkspacesWidget.vue` requires a known immutable surface output before exposing workspace buttons, filters only that output, validates the index, prevents default for accepted inputs, and launches the injected async action. Unknown output never falls back to every output's repeated workspace indices.
- Desktop controls use `useNativeAction` for local busy/error state, preserve fresh native results, and discard retired async responses after unmount or authorization/catalog changes. They never choose native commands, paths, bus identities, or global coordinates.
- The launcher uses a text input with `role="searchbox"`; WebKit's native search decoration requests a `data:` SVG that would revoke the local-only trust latch. Native resource validation stays unchanged.

## Mounted coverage

`WorkspacesWidget.mounted.test.ts` mounts the real PrimeVue-backed component with i18n and verifies primary pointer, Enter, and Space use `workspace.index`, other inputs are ignored, and unavailable/empty states render without buttons. View-mounted suites exercise the same widgets through real Pinia/store/shell-api wiring.

`FocusedWindowWidget.mounted.test.ts` covers distinct per-output titles and missing/null/unavailable state. The mounted Bar suite also changes global focused output while keeping each Bar's Niri label tied to its immutable host context.

The audio, tray, notification, and launcher mounted suites exercise real Vue/PrimeVue/store/SDK wiring with controlled native replies. Coverage includes visible unavailable/errors, exact action parameters, stale menu/catalog/async results, literal notification text, keyboard actions, parent-snapshot refresh, and teardown.
