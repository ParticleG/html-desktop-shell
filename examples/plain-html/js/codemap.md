# `examples/plain-html/js/`

## Responsibility

This directory implements a framework-free Bridge v5 client and one-shot surface-aware DOM renderer.

- `bridge.js` converts named JavaScript functions into request/reply messages for WebKit's `shell` handler.
- `app.js` discovers the receiving surface and its scoped capabilities, then renders status and only the controls that surface may invoke.

## Request/Reply Boundary (`bridge.js`)

`request(method, params = {})`:

1. allocates a process-local string ID from `nextRequestId`;
2. posts `{ id, method, params }` to `window.webkit.messageHandlers.shell.postMessage(...)`;
3. parses the returned JSON string;
4. returns `response.result` when `response.ok` is true;
5. otherwise throws `NativeBridgeError`, retaining `response.error.code` and using the native message or `"native bridge request failed"`.

The module exports only exact Bridge v5 adapters:

| Function | Native method | Parameters |
| --- | --- | --- |
| `getHostInfo()` | `getHostInfo` | `{}` |
| `getCapabilities()` | `getCapabilities` | `{}` |
| `getState()` | `getState` | `{}` |
| `focusWorkspace(workspaceIndex)` | `niriFocusWorkspace` | `{ workspaceIndex }` |
| `setQuickPanelOpen(open)` | `setQuickPanelOpen` | `{ open }` |
| `setDiagnosticsDialogOpen(open)` | `setDiagnosticsDialogOpen` | `{ open }` |

The additional audio, tray/menu, notification and installed-application adapters
use the same exact v5 params/results documented in the
[native contract](../../../docs/html-desktop-shell-multi-surface-implementation-plan.md#desktop-capability-extension--bridge-v5).
`getTrayMenu(itemId,menuItemId=0)` supports native-prepared lazy submenus.
`app.js` stays a minimal surface/state demo rather than duplicating the official
desktop UI; every named capability remains usable by another framework-free UI.

The wrapper neither validates nor reshapes successful results. It has no generic invoke, command, file, D-Bus, network, eval, arbitrary Niri action, or window-creation adapter.

## Capability and Surface Semantics (`app.js`)

`render()` loads host/capabilities concurrently, requires Bridge v5, and requests state only when authorized:

- `bar` and `quickPanel`: all 20 named methods;
- `workspaceHud`: `getHostInfo` and `getCapabilities` only;
- `diagnosticsDialog`: `getHostInfo`, `getCapabilities`, `getState`, and `setDiagnosticsDialogOpen`.

The native dispatcher enforces this matrix; hiding a button is not the security boundary. A known method outside the receiving surface's scope returns a structured `surface_forbidden` error.

The renderer reports `hostInfo.surface.kind`, monitor index, output, and the Workspace HUD's workspace index. When state is authorized, it also reports clock, monitor count, and Niri availability.

Actions are derived from both context and capability:

- `setQuickPanelOpen` renders an Open button everywhere except on `quickPanel`, where it renders Close.
- `setDiagnosticsDialogOpen` renders an Open button everywhere except on `diagnosticsDialog`, where it renders Close.
- Workspace buttons require both state and `niriFocusWorkspace`; entries must have a positive integer index and, when the receiving surface has an output, must match that output.

Each button disables itself during its request, clears the prior action status, writes errors through `textContent`, and restores itself in `finally`. A successful action does not trigger another state read or render.

## Data & Control Flow

1. `index.html` imports `app.js`, which captures `#app` and invokes `render()` once.
2. Host information identifies the typed receiving surface; capabilities provide its exact allowlist.
3. An authorized state read supplies provider data and candidate Niri workspaces.
4. Helper functions construct DOM nodes and assign dynamic values with `textContent`.
5. Capability-gated buttons call the exact wrapper with `{ open }` or `{ workspaceIndex }`.
6. Native success resolves silently; action failure updates the live status; startup failure replaces the mount with an error.

There is no interval, timeout, polling loop, event subscription, store, remote asset load, or direct WebKit call outside `bridge.js`.

## Integration Points

- **HTML consumer:** `examples/plain-html/index.html` provides the `#app` mount and imports `app.js`.
- **Trusted transport:** `window.webkit.messageHandlers.shell.postMessage` is the only browser-to-native dependency and exists only in the trusted hosted document.
- **Native dispatcher:** Bridge v5 enforces exact surface-scoped methods/parameters and native cache authority; asynchronous operations preserve the same structured reply envelope.
- **Typed surface controls:** Quick Panel and Diagnostics wrappers delegate visibility to the native surface manager rather than creating or addressing generic windows.
- **Workspace activation:** the native operation accepts only a positive published workspace index and executes the exact focus-workspace action.
- **Local-only browser behavior:** without the injected handler, the initial render reaches the text-only `"native bridge unavailable"`/native JavaScript error path; no fallback transport or remote endpoint exists.
