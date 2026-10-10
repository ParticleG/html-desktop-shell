# `examples/plain-html/`

## Responsibility

This framework-free Bridge v6 frontend demonstrates local assets, scoped surface controls, and a functional native-bound Tray Menu without duplicating the official desktop UI.

- `index.html` is the document entry point and contains only the `#app` loading mount.
- `shell.css` supplies the self-contained presentation for status details and action buttons.
- `js/app.js` renders the receiving surface, host state when authorized, and capability-gated controls.
- [`js/codemap.md`](js/codemap.md) maps exact Bridge v6 adapters and the renderer.

The example is source material, not an automatically selected runtime frontend.

## Startup and Local-Only Assets

1. Point the host at this directory, for example with `HTML_DESKTOP_SHELL_WEB_DIR=examples/plain-html`.
2. `src/assets.rs::web_assets` resolves the override to a canonical contained root and `index.html`.
3. `TrustedWebContext` serves that root read-only at `html-desktop-shell://assets/` and commits the trusted entry URI before the native bridge is attached.
4. `index.html` synchronously links local `shell.css` and loads local `js/app.js` as an ES module; the initial `<main id="app">` remains visible until rendering replaces it.

The document contains no remote URLs, fetches, package runtime, framework runtime, or external network assets. Opening it outside the trusted host does not manufacture native access: the missing WebKit handler is reported through the renderer's error path.

## Design / Patterns

- **Typed native context:** v6 identifies monitor/output, confirmed HUD workspace, or native-bound Tray Menu item.
- **Capability discovery:** only allowed exact operations are rendered/invoked.
- **Scope:** Bar/Quick retain 20 methods, HUD two, Diagnostics four, Tray Menu five. Native code independently enforces the matrix.
- **Exact controls:** `setQuickPanelOpen({ open })` and `setDiagnosticsDialogOpen({ open })` control only their typed surfaces. `niriFocusWorkspace({ workspaceIndex })` carries only a validated positive workspace index; there is no generic command, file, DBus, HTTP, eval, compositor-action, or window-creation method.
- **Output-relative actions:** unknown output exposes no workspace-index buttons; known output filters its own cached rows.
- **Menu renderer:** Tray Menu replaces the generic metadata view with `#tray-menu-content`, literal enabled/toggle/separator nodes, lazy submenus, current-revision activation, close/refresh/errors, and keyboard navigation. It never reads general state.
- **DOM-safe dynamic values:** details and errors use `textContent`; the app builds elements rather than interpolating native values into HTML.

## UI Structure

On success, `app.js::render()` replaces the loading state with:

- a heading;
- a definition list for backend, Bridge version, surface kind, monitor index, output, optional Workspace HUD workspace index, and exact capability names;
- clock, monitor count, and Niri availability only when `getState` is authorized;
- Quick Panel and Diagnostics open/close controls only when their methods are authorized;
- positive-index workspace focus buttons only when state and `niriFocusWorkspace` are both available;
- a live action-status paragraph when at least one action is rendered.

Button handlers disable their button for the in-flight call, clear the old status, report native errors as text, and always restore the button. Successful actions are not followed by a local rerender; the native surface manager owns resulting visibility.

## Data & Control Flow

1. `index.html` loads `js/app.js`, which captures `#app` and invokes `render()` once.
2. `render()` requests host information and scoped capabilities concurrently.
3. It requests provider state only if `getState` appears in the capability set.
4. `bridge.js::request()` allocates a string ID, posts `{ id, method, params }` to `window.webkit.messageHandlers.shell`, parses the JSON reply, returns `result`, or throws `NativeBridgeError` with native `message` and `code`.
5. The renderer builds status details and only the controls authorized for this surface.
6. A control calls one exact wrapper. Native visibility changes or Niri activation occur outside the page; wrapper failures appear in the action status.
7. Startup failures replace the entire mount with a text-only error.

## Integration Points

- **Native asset loader:** `HTML_DESKTOP_SHELL_WEB_DIR` can select this contained directory instead of generated or installed assets.
- **Trusted WebKit boundary:** only the host's reply-capable `shell` handler provides native access; the example itself remains static local content.
- **Bridge v6 methods:** `bridge.js` wraps 21 exact methods; native authorization remains authoritative. Tray Menu uses its five-method scope and never requests general state.
- **Native surface manager:** Quick Panel and Diagnostics visibility requests are routed to typed manager operations; the example has no direct window or layer-shell authority.
- **Niri provider:** workspace controls use only published workspace indices and the exact native focus-workspace operation.
- **Sub-map:** [`js/codemap.md`](js/codemap.md) documents request framing, wrappers, capability behavior, and DOM control flow.
