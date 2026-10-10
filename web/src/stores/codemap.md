# `web/src/stores/`

## Responsibility

`shell.ts` is the Pinia boundary to the framework-independent Bridge v6 SDK. It owns validated bootstrap, refreshed state, action errors, and capability hints. Tray Menu uses only bootstrap/capabilities and its separate bound menu session, never `refreshState`.

## State and initialization

- `hostInfo: HostInfo | null` and `capabilities: Capabilities | null` are populated together by `loadHostInfo()`.
- A module-store-local `initialization: Promise<void> | null` deduplicates concurrent and later successful bootstrap calls. `getHostInfo()` and `getCapabilities()` run concurrently with `Promise.all`; only a fully successful pair is committed.
- On bootstrap failure both values are cleared, the initialization promise is reset so a later call can retry, and `bridgeError` receives the thrown message or `bridge: unavailable`.
- `state: ShellState | null` stores the latest successful `getState()` result. `isRefreshing` prevents overlapping refresh requests; failed refreshes preserve the prior state and update `bridgeError`.
- `actionError` and `actionErrorDetail` hold a stable summary and the native error message for user-triggered operations.

## Public operations

- `loadHostInfo()` performs the one-time HostInfo/capability initialization described above.
- `refreshState()` serializes `getState()` calls and clears `bridgeError` after success.
- `focusWorkspaceFromButton(index)` clears action status, calls `focusWorkspace(index)`, and refreshes state on success; failures become `workspace switch failed` plus detail.
- `setQuickPanelOpenFromButton(open)` calls the exact Quick Panel wrapper and records `quick panel open failed` or `quick panel close failed` on failure.
- `setDiagnosticsDialogOpenFromButton(open)` calls the exact Diagnostics wrapper and records the corresponding open/close failure.
- `hasCapability(method: ShellMethod)` checks only the validated surface-scoped capability list. Views use it to decide whether a control exists.
- `clearActionError()` clears both action strings; every action invokes it before sending a request.

## Ownership and flow

1. `SurfaceRoot.vue` calls `loadHostInfo()` once on mount and dispatches from the validated `hostInfo.surface`.
2. Bar, Quick Panel, and Diagnostics views call `useShellPolling()` and read store refs. The HUD reads only its immutable surface prop and does not poll.
3. Polling views invoke `refreshState()` on their own lifecycle cadence; the store prevents overlap.
4. Bar and Quick Panel bind workspace and transient controls to the exact store actions, gated by `hasCapability()`.
5. Desktop controls read this surface-local state and capabilities. Successful audio/notification writes publish their fresh native snapshots; tray/menu and application operations retain native-owned IDs and local result generations.
6. Diagnostics renders host/capability data and provider availability/reasons from these refs, including recovery after a later successful refresh.

## Boundaries

The store is a presentation boundary, not authority. Native Bridge v6 enforces caller/item method scope and parameters; SDK errors preserve native codes/messages.
