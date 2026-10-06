# `web/packages/shell-api/src/`

## Responsibility

`index.ts` is the framework-independent TypeScript boundary for Bridge v4. It defines the discriminated surface and snapshot types, validates the two bootstrap results that establish a surface's identity and authority, adapts WebKit's reply handler, and exposes operation-specific wrappers. It has no Vue, Pinia, or component dependency.

## Bridge v4 model

- `SurfaceContext` is the union of `BarSurfaceContext`, `QuickPanelSurfaceContext`, `WorkspaceHudSurfaceContext`, and `DiagnosticsDialogSurfaceContext`.
- Every surface carries `kind`, a nonnegative 32-bit `monitorIndex`, and `output: string | null`. `WorkspaceHudSurfaceContext` additionally requires a positive integer `workspaceIndex`.
- `HostInfo` has the exact literals `shell: "html-desktop-shell"`, `backend: "wayland-layer-shell"`, and `bridgeVersion: 4`, plus one `surface` variant. There is no panel field or generic panel context.
- `ShellMethod` is the closed set `getHostInfo`, `getCapabilities`, `getState`, `niriFocusWorkspace`, `setQuickPanelOpen`, and `setDiagnosticsDialogOpen`. `Capabilities.methods` is a surface-scoped subset of that set.
- `ShellState` aggregates optional clock, host, Niri, battery, and network snapshots. Most provider data fields remain optional because providers can be absent or partially populated; the output-scoped window envelope's required fields are described below.
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

## Safety boundary and tests

The named wrappers are the intended API, but exported generic `request()` is not authorization. The native dispatcher remains authoritative for the per-surface allowlist, parameter validation, and operation execution; the client neither invents capabilities nor provides a fallback transport. `index.test.ts` covers exact action envelopes, all four surface shapes, rejection of legacy/unknown/malformed host data, rejection of unsupported capability names, and native error code/message preservation.

## Integration points

- `web/packages/shell-api/package.json` exports this file as `@html-desktop-shell/shell-api`.
- `web/src/stores/shell.ts` owns application state and calls the operation wrappers.
- `web/src/SurfaceRoot.vue` dispatches on validated `HostInfo.surface`; views gate controls with validated capabilities.
- The Rust WebKit handler and Bridge v4 dispatcher provide the transport, scoped authority, state snapshots, and exact reply envelopes consumed here.
