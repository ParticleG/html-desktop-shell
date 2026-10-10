# `examples/`

## Responsibility

These optional local frontend examples use Bridge v6 without generic native authority. They remain separate from the packaged official UI.

## Design / Patterns

- Each direct child is a self-contained example rather than a shared runtime module.
- [`plain-html/`](plain-html/codemap.md) is the dependency-free HTML/CSS/JavaScript client.
- Host transport stays behind `plain-html/js/bridge.js`; `app.js` consumes named wrappers and the capability list instead of calling arbitrary native functionality.
- Every stylesheet and module is a relative local asset. The example has no remote scripts, network fetches, package runtime, or external content dependency.

## Data & Control Flow

1. When `plain-html/` is selected as the asset root, the native host loads `html-desktop-shell://assets/index.html` and serves its contained CSS and JavaScript through the trusted custom scheme.
2. `app.js::render()` requests `getHostInfo` and `getCapabilities` concurrently, uses the returned surface-scoped method names to decide which reads and buttons exist, and requests `getState` only when that capability is present.
3. `bridge.js::request()` posts the exact envelope, parses native JSON, and preserves structured error codes. A Tray Menu branch reads/activates only its native-bound item and closes itself.
4. The page renders the receiving surface identity and capability list. Depending on that list, it can expose exact controls for Quick Panel visibility, Diagnostics visibility, and Niri workspace focus.

## Integration Points

- **Native boundary:** Bridge v6 has 21 exact operations, restricted by content kind; Tray Menu has five methods and no general state.
- **Authority boundary:** the wrapper cannot execute generic commands, access files, issue arbitrary compositor actions, make native HTTP requests, evaluate code, or create arbitrary windows.
- **Trusted-local boundary:** the host serves the selected contained root read-only at `html-desktop-shell://assets/` and attaches the native handler only to the committed trusted entry document. Opening the example in an ordinary browser reaches its visible bridge-unavailable path.
- **Runtime boundary:** this is standalone sample content, not the packaged Bar, Quick Panel, Workspace HUD, or Diagnostics frontend.

## Child Maps

| Directory | Responsibility | Detailed map |
| --- | --- | --- |
| `plain-html/` | Dependency-free Bridge v6 frontend, including bound tray-menu rendering and capability-gated surface controls. | [View map](plain-html/codemap.md) |
