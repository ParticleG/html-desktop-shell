# `examples/plain-html/js/`

## Responsibility

This directory implements a framework-free Bridge v6 client, scoped status controls, and a functional native-bound tray-menu renderer.

- `bridge.js` converts named JavaScript functions into request/reply messages for WebKit's `shell` handler.
- `app.js` discovers the receiving surface and its scoped capabilities, then renders status and only the controls that surface may invoke.

## Request/Reply Boundary (`bridge.js`)

`request(method, params = {})`:

1. allocates a process-local string ID from `nextRequestId`;
2. posts `{ id, method, params }` to `window.webkit.messageHandlers.shell.postMessage(...)`;
3. parses the returned JSON string;
4. returns `response.result` when `response.ok` is true;
5. otherwise throws `NativeBridgeError`, retaining `response.error.code` and using the native message or `"native bridge request failed"`.

The module exports exact Bridge v6 adapters:

| Function | Native method | Parameters |
| --- | --- | --- |
| `getHostInfo()` | `getHostInfo` | `{}` |
| `getCapabilities()` | `getCapabilities` | `{}` |
| `getState()` | `getState` | `{}` |
| `focusWorkspace(workspaceIndex)` | `niriFocusWorkspace` | `{ workspaceIndex }` |
| `setQuickPanelOpen(open)` | `setQuickPanelOpen` | `{ open }` |
| `setDiagnosticsDialogOpen(open)` | `setDiagnosticsDialogOpen` | `{ open }` |

The desktop adapters preserve exact native params/results. Tray Menu uses only
`getTrayMenu(itemId,menuItemId)`, current-revision `activateTrayMenuItem`, and
`closeTrayMenu()`; host menu opening keeps `{itemId}` and now returns `null`.
The native contract is documented in the implementation plan's v6 extension.

The wrapper neither validates nor reshapes successful results. It has no generic invoke, command, file, D-Bus, network, eval, arbitrary Niri action, or window-creation adapter.

## Capability and Surface Semantics (`app.js`)

`render()` requires Bridge v6 and requests state only when authorized:

- Bar/Quick: 20 methods; HUD: two; Diagnostics: four.
- Tray Menu: host/capabilities, bound root/submenu read, leaf activation, self-close.

The native dispatcher enforces this matrix; hiding a button is not the security boundary. A known method outside the receiving surface's scope returns a structured `surface_forbidden` error.

The renderer reports `hostInfo.surface.kind`, monitor index, output, and the Workspace HUD's workspace index. When state is authorized, it also reports clock, monitor count, and Niri availability.

Actions are derived from both context and capability:

- `setQuickPanelOpen` renders an Open button everywhere except on `quickPanel`, where it renders Close.
- `setDiagnosticsDialogOpen` renders an Open button everywhere except on `diagnosticsDialog`, where it renders Close.
- Workspace buttons require state, the focus capability, a known output, and a positive index from that output's cached publication.

Each button disables itself during its request, clears the prior action status, writes errors through `textContent`, and restores itself in `finally`. A successful action does not trigger another state read or render.

## Data & Control Flow

1. `index.html` imports `app.js`, which captures `#app` and invokes `render()` once.
2. Host information identifies the typed receiving surface; capabilities provide its exact allowlist.
3. An authorized state read supplies provider data and candidate Niri workspaces.
4. Helper functions construct DOM nodes and assign dynamic values with `textContent`.
5. Capability-gated buttons call the exact wrapper with `{ open }` or `{ workspaceIndex }`.
6. Native success resolves silently; action failure updates the live status; startup failure replaces the mount with an error.

There is no general-state polling or remote asset load. Tray Menu keeps a local read/expanded/revision session, refreshes explicitly, renders literal enabled/toggle/separator nodes, supports keyboard navigation, and closes only after successful leaf activation or explicit dismissal. `#tray-menu-content` is the native fitting target.

## Integration Points

- **HTML consumer:** `examples/plain-html/index.html` provides the `#app` mount and imports `app.js`.
- **Trusted transport:** `window.webkit.messageHandlers.shell.postMessage` is the only browser-to-native dependency and exists only in the trusted hosted document.
- **Native dispatcher:** Bridge v6 enforces caller/item scope, exact parameters, and native cache authority.
- **Typed surface controls:** Quick Panel and Diagnostics wrappers delegate visibility to the native surface manager rather than creating or addressing generic windows.
- **Workspace activation:** the native operation accepts only a positive published workspace index and executes the exact focus-workspace action.
- **Local-only browser behavior:** without the injected handler, the initial render reaches the text-only `"native bridge unavailable"`/native JavaScript error path; no fallback transport or remote endpoint exists.
