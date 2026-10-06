# `web/src/widgets/`

## Responsibility

This directory contains reusable leaf presentation components for the Bar and Quick Panel. Widgets do not own Pinia state, polling, bootstrap, surface dispatch, or bridge authority. Container views pass a shared immutable `WidgetProps` projection.

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

## Design and flow

- Components use `<script setup lang="ts">`, computed projections, Lucide icons, PrimeVue controls/tags, and local vue-i18n messages.
- Shared domain formatting stays in `../view-model.ts`: workspace filtering/labels/titles and focused-window, battery, network, and bridge summaries.
- `BarView.vue` dynamically renders all eight widgets according to `barLayout`. `QuickPanelView.vue` explicitly composes the workspace and provider/status subset.
- `WorkspacesWidget.vue` filters against `props.hostInfo?.surface.output`, validates the index, prevents default for accepted inputs, and launches the injected async action. Widgets never invoke shell-api directly.

## Mounted coverage

`WorkspacesWidget.mounted.test.ts` mounts the real PrimeVue-backed component with i18n and verifies primary pointer, Enter, and Space use `workspace.index`, other inputs are ignored, and unavailable/empty states render without buttons. View-mounted suites exercise the same widgets through real Pinia/store/shell-api wiring.

`FocusedWindowWidget.mounted.test.ts` covers distinct per-output titles and missing/null/unavailable state. The mounted Bar suite also changes global focused output while keeping each Bar's Niri label tied to its immutable host context.
