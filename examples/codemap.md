# `examples/`

## Responsibility

This directory contains optional, developer-facing integrations for the desktop shell. The examples are separate from the packaged frontend and show how independently authored, local web assets can use the typed Bridge v4 contract without acquiring generic native authority.

## Design / Patterns

- Each direct child is a self-contained example rather than a shared runtime module.
- [`plain-html/`](plain-html/codemap.md) is the dependency-free HTML/CSS/JavaScript client.
- Host transport stays behind `plain-html/js/bridge.js`; `app.js` consumes named wrappers and the capability list instead of calling arbitrary native functionality.
- Every stylesheet and module is a relative local asset. The example has no remote scripts, network fetches, package runtime, or external content dependency.

## Data & Control Flow

1. When `plain-html/` is selected as the asset root, the native host loads `html-desktop-shell://assets/index.html` and serves its contained CSS and JavaScript through the trusted custom scheme.
2. `app.js::render()` requests `getHostInfo` and `getCapabilities` concurrently, uses the returned surface-scoped method names to decide which reads and buttons exist, and requests `getState` only when that capability is present.
3. `bridge.js::request()` sends `{ id, method, params }` to the reply-capable `window.webkit.messageHandlers.shell` handler, parses the Bridge v4 response, returns `result`, or throws a `NativeBridgeError` retaining the native error code.
4. The page renders the receiving surface identity and capability list. Depending on that list, it can expose exact controls for Quick Panel visibility, Diagnostics visibility, and Niri workspace focus.

## Integration Points

- **Native boundary:** Bridge v4 exposes only `getHostInfo`, `getCapabilities`, `getState`, `niriFocusWorkspace`, `setQuickPanelOpen`, and `setDiagnosticsDialogOpen`, further restricted by the receiving surface kind.
- **Authority boundary:** the wrapper cannot execute generic commands, access files, issue arbitrary compositor actions, make native HTTP requests, evaluate code, or create arbitrary windows.
- **Trusted-local boundary:** the host serves the selected contained root read-only at `html-desktop-shell://assets/` and attaches the native handler only to the committed trusted entry document. Opening the example in an ordinary browser reaches its visible bridge-unavailable path.
- **Runtime boundary:** this is standalone sample content, not the packaged Bar, Quick Panel, Workspace HUD, or Diagnostics frontend.

## Child Maps

| Directory | Responsibility | Detailed map |
| --- | --- | --- |
| `plain-html/` | Local static Bridge v4 integration with surface-aware status and capability-gated controls. | [View map](plain-html/codemap.md) |
