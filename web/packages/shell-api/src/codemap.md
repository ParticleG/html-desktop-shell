# `web/packages/shell-api/src/`

## Responsibility

`index.ts` is the framework-independent TypeScript boundary for Bridge v5. It defines discriminated surface/provider types, validates bootstrap identity/capabilities, adapts WebKit replies and exposes exact named wrappers. It has no Vue, Pinia or component dependency.

## Bridge v5 model

- `SurfaceContext` is the union of `BarSurfaceContext`, `QuickPanelSurfaceContext`, `WorkspaceHudSurfaceContext`, and `DiagnosticsDialogSurfaceContext`.
- Every surface carries `kind`, a nonnegative 32-bit `monitorIndex`, and `output: string | null`. `WorkspaceHudSurfaceContext` additionally requires a positive integer `workspaceIndex`.
- `HostInfo` requires exact shell/backend literals and `bridgeVersion: 5`, with one immutable `SurfaceContext`.
- `ShellMethod` is the closed 20-method set: six retained surface/state/workspace operations plus 14 audio/tray/notification/application operations.
- `ShellState` includes existing system/Niri state and cached audio/tray/notification/application snapshots. New providers have explicit available/unavailable unions; Diagnostics audio/tray are summary-only.
- `NiriState.focusedWindows` uses `NiriFocusedWindowsState` with required `available`, optional `items`/`reason`, and `NiriFocusedWindowItem { output: string, window: FocusedWindow | null }`. `FocusedWindow` has optional `title`/`appId`; output selection belongs to the consuming surface, while `focusedOutput` is global compositor focus.
- `NativeResponse<T>`, `NativeError`, and `NativeBridgeError` model replies. `NativeBridgeError` preserves both the native message and optional error code.

## Validation and request flow

1. `request<T>()` locates only `window.webkit.messageHandlers.shell`, allocates a monotonically increasing decimal-string ID, substitutes `{}` for omitted params, posts `{ id, method, params }`, and parses the JSON reply.
2. A reply with `ok: false` throws `NativeBridgeError`; successful replies return `result`. Missing transport, invalid JSON, and rejected handler promises propagate as errors.
3. `getHostInfo()` requests unknown data and passes it through `validateHostInfo()`. The validator requires the exact top-level keys and literals, then delegates to `validateSurfaceContext()`.
4. `validateSurfaceContext()` rejects unknown kinds, extra or missing keys, invalid monitor indexes, non-string/non-null outputs, and missing or nonpositive HUD workspace indexes.
5. `getCapabilities()` requests unknown data and passes it through `validateCapabilities()`, which requires the exact `{ methods }` shape and rejects any method outside the closed `SHELL_METHODS` record.
6. `requireObject`, `requireExactObject`, `requireExactKeys`, `validateSurfaceFields`, and `invalidResult` implement these runtime shape checks. Validation failures are `TypeError`s prefixed with `invalid native bridge result:`.

## Operation wrappers

| Export | Native method | Exact params | Result |
| --- | --- | --- | --- |
| `getHostInfo()` | `getHostInfo` | `{}` | validated `HostInfo` |
| `getCapabilities()` | `getCapabilities` | `{}` | validated `Capabilities` |
| `getState()` | `getState` | `{}` | `ShellState` |
| `focusWorkspace(workspaceIndex)` | `niriFocusWorkspace` | `{ workspaceIndex }` | `{ workspaceIndex }` |
| `setQuickPanelOpen(open)` | `setQuickPanelOpen` | `{ open }` | `{ open }` |
| `setDiagnosticsDialogOpen(open)` | `setDiagnosticsDialogOpen` | `{ open }` | `{ open }` |
| `setAudioVolume(volume)` / `setAudioMuted(muted)` | exact audio write | `{ volume }` / `{ muted }` | fresh `AudioState` |
| `getTrayMenu(itemId,menuItemId=0)` | `getTrayMenu` | `{ itemId,menuItemId }` | native `TrayMenu` revision/tree |
| `activateTrayItem` / `secondaryActivateTrayItem` / `showTrayItemContextMenu` | corresponding exact item action | `{ itemId }` | `{ itemId }` |
| `scrollTrayItem(itemId,delta,orientation)` | `scrollTrayItem` | named fields | `{ itemId }` |
| `activateTrayMenuItem(itemId,menuItemId,revision)` | `activateTrayMenuItem` | named fields | selected IDs/revision |
| `getNotifications()` | `getNotifications` | `{}` | session history items |
| `dismissNotification` / `invokeNotificationAction` / `markAllNotificationsRead` | corresponding exact notification action | native ID/key or `{}` | authoritative count state |
| `searchApplications(query,limit=25)` | `searchApplications` | `{ query,limit }` | installed `DesktopApplication` items |
| `launchApplication(applicationId)` | `launchApplication` | `{ applicationId }` | `{ applicationId }` |

`ShellIcon` carries bounded native RGBA8 pixels, not a URL. Tray items include
`hasMenu`; menu nodes include `hasSubmenu` for lazy expansion. Notification text
is literal; application results contain installed IDs, not filenames/commands.
Full native bounds, ownership, lifecycle and result details are in the
[implementation plan](../../../../docs/html-desktop-shell-multi-surface-implementation-plan.md#desktop-capability-extension--bridge-v5).

## Safety boundary and tests

The named wrappers are the intended API, but exported generic `request()` is not authorization. The native dispatcher remains authoritative for the per-surface allowlist, parameter validation, and operation execution; the client neither invents capabilities nor provides a fallback transport. `index.test.ts` covers exact action envelopes, all four surface shapes, rejection of legacy/unknown/malformed host data, rejection of unsupported capability names, and native error code/message preservation.

## Integration points

- `web/packages/shell-api/package.json` exports this file as `@html-desktop-shell/shell-api`.
- `web/src/stores/shell.ts` owns application state and calls the operation wrappers.
- `web/src/SurfaceRoot.vue` dispatches on validated `HostInfo.surface`; views gate controls with validated capabilities.
- The Rust WebKit handler and Bridge v5 dispatcher provide transport, immutable surface authority, strict typed operations and trusted asynchronous replies.
