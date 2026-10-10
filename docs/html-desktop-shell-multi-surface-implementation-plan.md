# HTML Desktop Shell Typed Multi-Surface Implementation Plan

## Status

Completed on 2026-10-06, following initial implementation delivery on 2026-08-02.

Quick Panel, compositor-confirmed Workspace HUD, and read-only Diagnostics
Dialog are implemented with the typed multi-surface foundation. Final automated
release checks and the post-fix DP-2/DP-3 manual matrix passed on 2026-10-06, as
recorded below. This document remains the delivered design and verification
record, not an outstanding implementation queue.

This document expands the P3 contract in
[`html-desktop-shell-feature-roadmap.md`](html-desktop-shell-feature-roadmap.md).
The roadmap remains the priority source of truth.

The P3 sections record the completed Bridge v4 delivery; the desktop extension
records Bridge v5. The tray-menu extension below defines current Bridge v6:
it adds a narrowly scoped fifth content surface and close operation without
changing the original four surfaces' geometry/input policies or local trust
boundary. Earlier version descriptions are historical, not current contracts.

## Goal

Evolve the current one-Bar-per-monitor shell into a typed collection of
independent layer-shell surfaces without weakening the local-assets-only WebView
boundary or introducing generic native authority.

The current 32px Bar is already the repository's native panel. A drawer, dialog,
or HUD cannot render outside that window's Wayland surface, so every new product
surface must be a separate layer-shell `ApplicationWindow`. Normal GTK windows,
`GtkDialog`, and in-Bar DOM overlays are not substitutes.

The first accepted delivery is a usable Quick Panel vertical slice. Do not ship
an empty surface framework, placeholder views, or no-op bridge methods.

## Non-goals

This plan does not add:

- StatusNotifierItem or system tray support;
- notification daemon behavior;
- audio controls or PipeWire/WirePlumber integration;
- launcher, search, or process-launch authority;
- power, logout, reboot, suspend, or other session control;
- generic DBus, filesystem, HTTP, process, compositor-action, or evaluation APIs;
- a generic window, dialog, HUD, or plugin API;
- normal GTK window fallbacks;
- runtime theming or arbitrary surface configuration.

The Diagnostics Dialog is read-only. It is not a power dialog or a generic
permission prompt.

## Delivered implementation baseline

The current implementation preserves these boundaries:

- `src/shell_host.rs` owns the monitor observation boundary and a GIO application
  hold across runtime window gaps; it delegates presentation, complete topology
  rebuild, and shutdown to `SurfaceManager`.
- `src/surface.rs` defines the closed surface kinds, keys, immutable contexts,
  native policies, and typed errors.
- `src/surface_manager.rs` owns persistent Bars, typed transients, HUD
  generations/timers, monitor mappings, and the niri activation subscription.
- `src/surface_window.rs` is the single typed layer-shell/WebView factory.
- `src/web_security.rs` serves one canonical local asset root through
  `html-desktop-shell://assets/` and gives every WebView an independent trust
  latch.
- `src/bridge.rs` attaches the trusted `shell` handler with the exact immutable
  caller context and weak typed surface operations.
- `src/messages.rs` implements Bridge v4's six-method, deny-by-default,
  surface-scoped capability matrix.
- `src/providers/niri.rs` publishes coherent cached state and output-scoped
  active-window snapshots, validates exact workspace indices, and emits
  compositor-confirmed typed activation events.
- `src/providers/niri_collector.rs` owns the asynchronous niri event-stream
  lifecycle.
- `web/src/SurfaceRoot.vue` dispatches directly to the four typed views without
  a router; `web/src/stores/shell.ts` and each stateful view own initialization
  and polling lifecycle.
- `scripts/niri-layer-smoke.ts` validates persistent and transient compositor
  tuples plus disappearance.

## Fixed design decisions

### Surface kinds

Use a closed native and TypeScript surface kind set:

```text
Bar
QuickPanel
WorkspaceHud
DiagnosticsDialog
```

Do not represent kinds as arbitrary strings below the serialization boundary.
Every exhaustive Rust and TypeScript match must fail compilation when a new kind
is added without handling.

### Surface policy matrix

| Kind | Lifetime | Layer | Placement | Exclusive zone | Keyboard | Pointer |
| --- | --- | --- | --- | ---: | --- | --- |
| Bar | Persistent per active monitor | Top | Left + right + top | Configured Bar height | Existing configured mode | Interactive |
| Quick Panel | On demand, one per monitor | Overlay | Top + right | 0 | OnDemand | Interactive |
| Workspace HUD | Timed, one per monitor | Overlay | Centered | 0 | None | Empty native input region |
| Diagnostics Dialog | On demand, one per monitor | Overlay | Centered | 0 | OnDemand | Interactive |

Transient surface geometry is a typed native policy, not arbitrary bridge input.
Frontend CSS owns internal layout and presentation within the native allocation.

### Namespace contract

Keep the existing Bar namespace and add exact transient prefixes:

```text
html-desktop-shell-panel-<monitor-index>
html-desktop-shell-quick-panel-<monitor-index>
html-desktop-shell-workspace-hud-<monitor-index>
html-desktop-shell-diagnostics-<monitor-index>
```

A surface registry must never contain duplicate `(kind, monitor-index)` keys.

### Monitor topology behavior

Retain the current complete-rebuild strategy for monitor topology changes:

1. Construct the new persistent Bar set against the new GDK monitor model.
2. If construction fails, keep the old set and report the error.
3. If construction succeeds, close every old persistent and transient surface.
4. Replace the monitor mapping and present the new Bars.
5. Do not carry an open Quick Panel, HUD timer, or Diagnostics Dialog across a
   topology rebuild.

This avoids inventing stable monitor identity beyond the current connector and
runtime monitor index contract.

### WebView isolation

Every surface:

- uses the one shared `TrustedWebContext` and canonical asset root;
- creates a separate WebView and `WebViewTrust` latch;
- loads only `html-desktop-shell://assets/index.html`;
- receives its immutable native `SurfaceContext` when the bridge is attached;
- cannot select its own kind, monitor, output, or workspace HUD payload;
- loses bridge access permanently after the existing trust policy revokes it.

Destroy the first-version Quick Panel, HUD, and Diagnostics WebView when the
surface closes. Do not introduce native JavaScript injection or a generic
native-to-web event channel to reuse hidden WebViews. If measured HUD startup is
unacceptable, a separate reviewed design may add a narrow typed notification;
it is not part of this plan.

## Bridge v4 wire contract

### Clean version cutover

Set the native and shell API bridge version to `4`. Migrate every caller and
example in one cutover. Do not retain `PanelContext`, bridge v3 aliases, version
fallbacks, or deprecated re-exports.

### Surface context

Replace `HostInfo.panel` with the following discriminated context:

```ts
export type SurfaceContext =
  | {
      kind: "bar"
      monitorIndex: number
      output: string | null
    }
  | {
      kind: "quickPanel"
      monitorIndex: number
      output: string | null
    }
  | {
      kind: "workspaceHud"
      monitorIndex: number
      output: string | null
      workspaceIndex: number
    }
  | {
      kind: "diagnosticsDialog"
      monitorIndex: number
      output: string | null
    }

export interface HostInfo {
  shell: "html-desktop-shell"
  backend: "wayland-layer-shell"
  bridgeVersion: 4
  surface: SurfaceContext
}
```

The Rust representation should use a serde-tagged enum with exact camel-case
wire names. `workspaceIndex` is present only for `workspaceHud`.

### New exact methods

Add exactly these methods:

```text
setQuickPanelOpen({ open: boolean }) -> { open: boolean }
setDiagnosticsDialogOpen({ open: boolean }) -> { open: boolean }
```

Request envelopes and action params must be JSON objects. Reject array/scalar
params before typed deserialization; serde's `deny_unknown_fields` alone does
not reject the sequence representation of a struct. Parameter objects reject
missing, non-boolean, and unknown fields. Native code infers the monitor/output
from the trusted caller `SurfaceContext`; the request cannot contain either value.

Stable surface errors:

| Code | Meaning |
| --- | --- |
| `surface_forbidden` | The caller surface kind is not authorized for the method. |
| `surface_unavailable` | The caller's monitor/output is no longer current. |
| `surface_failed` | Native construction or presentation failed. |

Idempotent open/close requests return the resulting state rather than an error.

### Surface-scoped capabilities

| Method | Bar | Quick Panel | Workspace HUD | Diagnostics Dialog |
| --- | :---: | :---: | :---: | :---: |
| `getHostInfo` | Yes | Yes | Yes | Yes |
| `getCapabilities` | Yes | Yes | Yes | Yes |
| `getState` | Yes | Yes | No | Yes |
| `niriFocusWorkspace` | Yes | Yes | No | No |
| `setQuickPanelOpen` | Yes | Yes | No | No |
| `setDiagnosticsDialogOpen` | Yes | Yes | No | Yes |

`setQuickPanelOpen(false)` is the Quick Panel's explicit close method.
`setDiagnosticsDialogOpen(false)` is the Diagnostics Dialog's explicit close
method. Opening Diagnostics from Quick Panel closes Quick Panel after the dialog
has been constructed successfully, preventing overlapping interactive transient
surfaces on the same monitor.

`getCapabilities` must be computed from this matrix and the caller context, not
from one application-global method list.

### Native dispatch integration

Do not add a generic request handler map. Extend the existing dispatcher with
explicit method branches and explicit callbacks for the two surface operations.
The bridge callback holds a weak reference to `SurfaceManager` so the
`UserContentManager` callback cannot keep the host or windows alive.

Do not hold a `RefCell` borrow across:

- WebKit reply completion;
- window creation or presentation;
- provider action execution;
- focus/close signal emission;
- nested calls into `SurfaceManager`.

Extract the exact request and caller context, release borrows, perform the
operation, then build the response.

## Native architecture

### Native modules

The delivered module boundaries are:

- `src/surface.rs` — closed kinds, contexts, keys, policies, and typed errors.
- `src/surface_manager.rs` — persistent/transient registry and lifecycle.
- `src/surface_window.rs` — the sole typed layer-shell/WebView window factory.
- `src/shell_host.rs` — application/display owner delegating all windows to
  `SurfaceManager`.
- `src/bridge.rs` — per-view trust gate plus weak typed surface operations.
- `src/messages.rs` — Bridge v4 wire types, capability matrix, validation, and
  exact dispatch.

### Core native types

The implementation may refine field visibility, but it must preserve this
closed structure:

```rust
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SurfaceKind {
    Bar,
    QuickPanel,
    WorkspaceHud,
    DiagnosticsDialog,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SurfaceKey {
    kind: SurfaceKind,
    monitor_index: u32,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum SurfaceContext {
    Bar { monitor_index: u32, output: Option<String> },
    QuickPanel { monitor_index: u32, output: Option<String> },
    WorkspaceHud {
        monitor_index: u32,
        output: Option<String>,
        workspace_index: u8,
    },
    DiagnosticsDialog { monitor_index: u32, output: Option<String> },
}
```

`SurfaceContext` attached to a WebView is immutable for that WebView's lifetime.

### Surface registry

`SurfaceManager` owns:

- the GTK application;
- the current GDK monitor list/mapping;
- current `ShellConfig` for Bar geometry only;
- shared `ProviderRegistry`;
- shared `TrustedWebContext`;
- persistent Bar instances;
- a map of transient `SurfaceKey -> SurfaceInstance`;
- HUD timer generation/source state;
- exact niri workspace activation subscription.

Required operations:

```text
build_initial_bars
rebuild_for_monitors
set_quick_panel_open
show_workspace_hud
set_diagnostics_dialog_open
close_surface
shutdown
```

These are internal typed methods, not generic bridge methods. `close_surface` is
used only by native lifecycle code after it has already selected a typed key.

### Window construction

`surface_window.rs` should accept a closed typed surface specification and match
all geometry/input behavior exhaustively. It must not accept arbitrary layer,
anchor, namespace, keyboard, or exclusive-zone values from browser requests.

Construction order remains security-sensitive:

1. Build and configure the layer-shell window.
2. Create the secured WebView and its trust latch.
3. Install native surface-specific realize/focus/key/close hooks.
4. Attach the bridge callback before registering its message handler.
5. Load the exact trusted entry URI.
6. Set the WebView as child.
7. Present only after the complete `SurfaceInstance` is registered.

If any step fails, close the partial window and leave the registry unchanged.

### Quick Panel lifecycle

Open:

1. Authorize the caller kind and resolve its current monitor index.
2. If the exact Quick Panel key exists, return `{ open: true }`.
3. Construct a top/right overlay surface with zero exclusive zone.
4. Register the instance, then present it.
5. Return `{ open: true }`.

Close:

1. If absent, return `{ open: false }`.
2. Remove the instance from the registry before calling `close()`.
3. Disconnect native handlers owned outside normal GTK object lifetime if any.
4. Close the window and return `{ open: false }`.

Close on Escape and focus loss through the same idempotent native path. Do not
create a fullscreen transparent click-catcher for outside-click behavior because
it would intercept input from other Wayland clients.

### Workspace activation notification

Add one narrow observer contract to the niri provider; do not add a generic
provider event bus.

After a generation has completed the initial workspace/window barrier:

1. Derive the focused workspace from the newly applied reducer state.
2. Compare `(output, workspace_index)` with the last published focused workspace.
3. Suppress the initial coherent publication and every reconnect's initial
   publication.
4. Suppress duplicate values.
5. On a real subsequent change, publish one typed
   `WorkspaceActivation { output, workspace_index }` after releasing reducer and
   publication borrows.

The SurfaceManager owns the single activation handler and disconnects it during
shutdown. This notification covers Bar actions, niri key bindings, and other
niri clients. `niriFocusWorkspace` only requests a change; subprocess success
must not directly display a HUD.

### Workspace HUD lifecycle

On a typed workspace activation:

1. Resolve the output against the current monitor mapping; ignore a stale or
   absent output.
2. Increment the HUD generation for that monitor.
3. Remove and close any existing HUD for the exact key.
4. Construct a new immutable HUD `SurfaceContext` with the confirmed index.
5. Present the centered overlay surface.
6. Register a one-shot GLib timeout carrying the monitor key and generation.
7. On timeout, close only when the current generation still matches.

The first implementation destroys and recreates the HUD WebView for each
activation. Measure visible latency during manual QA; do not add JavaScript
injection or a generic native event channel as an optimization.

After GTK realizes the HUD, obtain its underlying `GdkSurface` and apply an empty
Cairo input region through the GDK surface API. Also set WebView/widget targeting
and keyboard mode defensively, but neither is a replacement for the native empty
input region. Failure to establish native pointer transparency is a HUD
construction failure.

### Diagnostics Dialog lifecycle

- Bar and Quick Panel may request open.
- Diagnostics Dialog may request only its own close.
- It is centered, overlay-layer, zero-exclusive-zone, and on-demand keyboard.
- Escape, focus loss, monitor removal, explicit close, and shutdown use one
  idempotent native close path.
- It consumes only existing `getHostInfo`, `getCapabilities`, and `getState`
  responses.
- It exposes no asset root, environment dump, logs, process information beyond
  already-published state, or native troubleshooting escape hatch.

### Shutdown

`SurfaceManager::shutdown()` is idempotent:

1. Mark the manager stopped before disconnecting anything.
2. Disconnect the niri activation handler.
3. Remove every HUD timeout source.
4. Remove transient instances from the registry and close them.
5. Close persistent Bars.
6. Shut down the shared provider registry exactly once.
7. Ignore every queued callback whose stopped/generation guard is stale.

SIGINT/SIGTERM callbacks request `Application::quit()` on the GLib main loop.
After it returns, `main` removes those callbacks and drops the host.
`ShellHost::drop` disconnects monitor observation and delegates shutdown without
performing separate window/provider cleanup paths; its application hold is
released after cleanup. Startup still requires a usable monitor, while runtime
window/monitor gaps do not automatically terminate the host.

## Frontend architecture

### Surface root

`SurfaceRoot.vue` obtains `HostInfo.surface`, then renders exactly one view:

```text
SurfaceRoot
├── BarView
├── QuickPanelView
├── WorkspaceHudView
└── DiagnosticsDialogView
```

Do not encode the surface kind in URL paths, query parameters, fragments, global
variables, or browser storage. The immutable native bridge context is the only
authority.

There is no frontend router. Native surface context, not browser navigation, is
the sole view-selection authority.

### Store lifecycle

`SurfaceRoot` loads HostInfo once. Stateful views (Bar, Quick Panel, and
Diagnostics) then start `getState` polling without a second HostInfo request.
Workspace HUD does not poll because its immutable surface context contains all
render data. The disposed latch and refresh serialization remain in place, and
every stateful view owns and stops only its own polling lifecycle.

### Views and components

Delivered frontend views and components:

- `BarView.vue` retains the declarative Bar widget layout and exposes exact Quick
  Panel and Diagnostics controls.
- `QuickPanelView.vue` renders its separate provider/workspace layout and exact
  workspace and close controls.
- `WorkspaceHudView.vue` uses only the immutable `workspaceHud` context.
- `DiagnosticsDialogView.vue` renders read-only host, capability, provider, and
  unavailable state with an exact close control.
- CSS is scoped under distinct surface roots so Bar overflow and 32px
  assumptions do not constrain transient views.
- Surface labels, close controls, diagnostics state, and unavailable messages
  use the delivered i18n keys.

### Framework-independent shell API

`@html-desktop-shell/shell-api` exports:

- Bridge version literal `4`;
- the discriminated `SurfaceContext` union and `HostInfo.surface`;
- `setQuickPanelOpen(open: boolean)`;
- `setDiagnosticsDialogOpen(open: boolean)`;
- exact typed results and native error handling consistent with the other named
  wrappers.

`examples/plain-html` displays surface kind and consults capabilities before
showing optional surface controls. It remains framework independent and
local-only.

## Delivery checklist and dated evidence

### Typed foundation and Quick Panel

- [x] Closed kinds, keys, immutable contexts, native policies, Bridge v4 wire
  types, strict errors, and the complete surface capability matrix.
- [x] One typed window factory and `SurfaceManager`; no parallel window-factory,
  direct Bar-vector, router, or compatibility path remains.
- [x] Exact idempotent Quick Panel open/close operation, per-output uniqueness,
  Escape/focus-loss cleanup, monitor rebuild, and shutdown lifecycle.
- [x] Shell API, official frontend, plain HTML example, and tests migrated in one
  Bridge v4 cutover.
- [x] Usable Quick Panel content and exact workspace action reuse delivered.

### Compositor-confirmed Workspace HUD

- [x] Narrow post-initial-sync niri activation observer with initial/recovery and
  duplicate suppression.
- [x] Typed HUD context, output lookup, replacement generations, stale-timer
  protection, and deterministic timeout.
- [x] Empty native GDK input region after realization, defensive no-target/no-
  keyboard policy, and failure cleanup.
- [x] HUD view renders immutable output/index context without polling or write
  capabilities.

### Read-only Diagnostics Dialog

- [x] Exact caller-scoped open and self-close handling with Quick Panel handoff.
- [x] Escape, focus-loss, explicit-close, monitor-removal, and shutdown paths
  converge on idempotent native cleanup.
- [x] Existing host, scoped capability, provider, and unavailable state only; no
  new troubleshooting or privileged authority.

### Validation and release state

- [x] On 2026-10-06, `cargo fmt --check`, clippy with `-D warnings`, all 80
  Rust tests, and the locked Rust build passed.
- [x] On 2026-10-06, array-envelope and array-action-param regression tests
  rejected malformed shapes without calling state/action callbacks. A compiled
  smoke using the production dispatcher confirmed six array requests returned
  `bad_request`/`invalid_params` with zero action calls; a valid object request
  with leading JSON whitespace executed once. Native release gates were rerun
  after the object-shape fix.
- [x] On 2026-10-06, the frozen Bun install, web type-check, all 30 web tests
  (119 assertions), and the Vite build passed.
- [x] On 2026-10-06, all 31 smoke-validator tests (94 assertions) and the
  persistent Bar smoke with simultaneously active DP-2 and DP-3 passed.
- [x] On 2026-10-06, observed Quick Panel and Diagnostics tuples were unique on
  each initiating output, with Overlay/OnDemand policy and disappearance after
  close. User checks confirmed explicit close, Escape, focus loss, and
  Quick Panel-to-Diagnostics handoff on both outputs.
- [x] On 2026-10-06, HUD tuples used Overlay/None on the activated output. User
  checks confirmed exact output/index content, Bar/keyboard/external-niri
  activation, replacement, the 1.5-second timeout, center/edge pointer
  click-through, no keyboard-focus capture, and transparent rounded corners.
- [x] On 2026-10-06, both Bars retained 32px geometry/exclusive zones. Active
  window titles followed each output's active workspace, and Niri labels stayed
  tied to immutable surface output across global keyboard-focus changes.
  Transient surfaces did not change maximized-client geometry.
- [x] On 2026-10-06, physical DP-3 unplug with a transient open left one DP-2
  Bar and a running process; reconnect restored one Bar on each output. This
  passed after adding the application hold for runtime window gaps.
- [x] On 2026-10-06, SIGTERM and SIGINT each exited with status 0. No shell layer
  or recorded child remained; final SIGINT inspection covered the complete
  captured application/WebKit/Niri process tree. Native lifecycle tests cover
  timer removal and stale-callback suppression.
- [x] The complete post-fix two-output manual matrix below was observed on
  2026-10-06. No P3 release gate remains open as of that date.

These are dated validation results, not standing pass claims for future changes.

## Verification matrix

### Native behavior

| Area | Required cases |
| --- | --- |
| Surface registry | Open, duplicate open, idempotent close, wrong monitor, construction failure, focus-loss close, hotplug rebuild, shutdown |
| Authorization | Every allowed matrix cell; every denied matrix cell; malformed/unknown params; stale caller monitor |
| Bridge v4 | Exact HostInfo union shapes, version, capability order/content, result payloads, unknown-method denial |
| Workspace activation | Initial suppression, reconnect suppression, external activation, duplicate suppression, output/index derivation, callback after borrow release |
| HUD timers | Replace old HUD, stale timer, monitor removal, shutdown, missing output |
| Pointer transparency | Realized GDK surface receives empty input region; failure path closes partial HUD |

### Mounted frontend behavior

| Surface | Required cases |
| --- | --- |
| SurfaceRoot | All four discriminated kinds, unknown/malformed HostInfo failure, no wrong-view flash |
| Bar | Quick Panel toggle, Diagnostics open, existing workspace pointer/keyboard action, visible errors |
| Quick Panel | Existing state, unavailable parts, exact workspace action, Escape/close action, Diagnostics handoff |
| Workspace HUD | Exact output/index render, no polling, no interactive controls |
| Diagnostics | Host/capability/provider data, unavailable reasons, exact close, no privileged controls |
| Lifecycle | Pending initialization unmount, interval cleanup, surface destruction, rejected bridge request |

All mounted bridge assertions use the real Pinia-to-shell-api path and mock only
`window.webkit.messageHandlers.shell.postMessage`.

### Compositor smoke

With transient surfaces closed, retain the current exact Bar assertion.

For a manually opened transient surface, validate:

| Surface | Namespace prefix | Layer | Keyboard | Count |
| --- | --- | --- | --- | ---: |
| Quick Panel | `html-desktop-shell-quick-panel-` | Overlay | OnDemand | 1 on caller output |
| Workspace HUD | `html-desktop-shell-workspace-hud-` | Overlay | None | 1 on activated output |
| Diagnostics | `html-desktop-shell-diagnostics-` | Overlay | OnDemand | 1 on caller output |

After close/timeout, the corresponding namespace must disappear. Diagnostics
must distinguish missing, duplicate, wrong output, wrong index, wrong layer,
wrong keyboard mode, and leaked-after-close surfaces.

### Manual two-output matrix

Verify on both outputs:

- the persistent Bar remains 32px and preserves the exclusive zone;
- each Bar's active-window title follows its own output, while the Niri output
  label remains fixed to that surface even when global keyboard focus changes;
- Quick Panel opens on the initiating output, reserves no space, receives focus,
  closes on Escape/focus loss, and does not duplicate;
- workspace activation from Bar, keyboard binding, and external niri command
  shows the HUD on the compositor-confirmed output;
- clicking through every visible HUD area reaches the underlying client;
- HUD never receives keyboard focus and closes at the expected timeout;
- Diagnostics opens on the initiating output, renders current read-only state,
  and closes deterministically;
- maximizing clients is unaffected by transient surfaces;
- unplugging an output closes its transient surfaces, keeps the process alive,
  and leaves the remaining Bar set correct; reconnecting restores one Bar per
  output;
- SIGINT/SIGTERM shutdown leaves no WebKit window, GLib timer, or niri child.

## Release commands

Run after the requested surfaces work and the frontend bundle has been rebuilt:

```bash
(cd web && bun install --frozen-lockfile && bun run typecheck && bun test && bun run build)
bun test scripts/niri-layer-smoke.test.ts
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
scripts/smoke-current-niri.sh
```

The transient-surface compositor checks and rendered/focus/input behavior remain
current-session manual release gates until a narrow automation path can open
those exact surfaces without adding a production-only generic control API.

## Risks and required decisions during implementation

### Layer-shell focus semantics

Focus loss is compositor-mediated. Verify actual niri behavior before treating a
signal as the authoritative close trigger. Escape remains the deterministic
keyboard close path. Do not implement outside-click through a fullscreen input
surface.

### HUD startup latency

Creating a WebKit view per activation is the initial safe design. Measure from
compositor-confirmed activation to rendered HUD. If unacceptable, stop and
review a typed reuse/notification design; do not add `evaluate_javascript`,
generic events, mutable caller identity, or hidden polling as an unreviewed
shortcut.

### Output identity

Connector names and runtime indices are sufficient only within the current
monitor model. The complete-rebuild policy deliberately discards transient state
on topology changes. Do not add persistence or inferred stable IDs in this
phase.

### Custom frontends

A custom frontend receives the same exact surface context and scoped
capabilities. Omitting a view for a kind may render a local unavailable message,
but it must not weaken bridge authorization or cause native fallback content.

### Deferred authority

Quick Panel and Diagnostics may make deferred features visually tempting. Do not
add placeholder audio, tray, notification, launcher, power, or session controls.
Each requires a separate user need, provider boundary, exact API, unavailable
state, authorization analysis, and mounted/native tests before implementation.

## Completion definition

P3 is complete only when:

- Quick Panel, Workspace HUD, and Diagnostics Dialog behave end to end on two
  outputs;
- all surfaces are independent layer-shell windows with typed native policies;
- bridge v4 is the only protocol and all callers are migrated;
- capabilities and writes are scoped to trusted caller surface kinds;
- HUD state comes from compositor-confirmed activation and is natively
  pointer-transparent;
- monitor rebuild and application shutdown leave no transient window, timer,
  callback, WebView trust state, or child process;
- automated release commands pass;
- current-session compositor and manual gates pass;
- README, roadmap, examples, and codemaps describe the delivered behavior without
  undated standing pass claims.

## Desktop-capability extension — Bridge v5

Approved on 2026-10-08, backend first. This extends the native shell with complete
default-output audio control, standard tray/menu integration, a standard
notification server with session history, and installed desktop application
search/launch. The native host and public protocol are independent of Vue;
`web/src/` is the replaceable reference UI, not the authority for business logic.
The shell API contains no Vue or Pinia dependency.

### Authorization and exact parameters

Bridge version is `5`. Bar and Quick Panel expose all 20 exact methods. HUD keeps
only `getHostInfo`/`getCapabilities`. Diagnostics keeps the four original methods,
with close-only self authority and new provider availability/count summaries.

All new params are JSON objects with exactly the named fields. Arrays, null,
unknown fields, wrong numeric types, unsafe identifiers and unauthorized surfaces
fail before privileged dispatch. Empty-object methods still require `{}`.

| Native method | Exact params | Successful result |
| --- | --- | --- |
| `setAudioVolume` | `{volume}`: integer 0–100 | fresh `AudioState` |
| `setAudioMuted` | `{muted}`: boolean | fresh `AudioState` |
| `getTrayMenu` | `{itemId,menuItemId}`: opaque string / i32 | `{itemId,revision,root}` |
| `activateTrayItem` | `{itemId}` | `{itemId}` |
| `secondaryActivateTrayItem` | `{itemId}` | `{itemId}` |
| `showTrayItemContextMenu` | `{itemId}` | `{itemId}` |
| `scrollTrayItem` | `{itemId,delta,orientation}`: i32 / `horizontal` or `vertical` | `{itemId}` |
| `activateTrayMenuItem` | `{itemId,menuItemId,revision}`: i32 / u32 | `{itemId,menuItemId,revision}` |
| `getNotifications` | `{}` | `{items:NotificationItem[]}` |
| `dismissNotification` | `{notificationId}`: nonzero u32 | notification-count state |
| `invokeNotificationAction` | `{notificationId,actionKey}` | notification-count state |
| `markAllNotificationsRead` | `{}` | notification-count state |
| `searchApplications` | `{query,limit}`: at most 256 characters / integer 1–50 | `{items:DesktopApplication[]}` |
| `launchApplication` | `{applicationId}` | `{applicationId}` |

IDs are nonempty, NUL-free, at most 256 UTF-8 bytes, and must belong to the
current native cache. Notification action keys instead match the server's
256-character bound, including non-ASCII keys. A query may be empty but contains
no NUL. No request supplies a command, argument, environment, filename, launch
URI, bus name, object path, global coordinate, monitor or output.

`NativeDispatch` separates immediate responses from typed `DesktopRequest`
operations. `bridge.rs` resolves the caller's current native monitor, derives a
screen point and GDK launch context, and returns asynchronous results only to a
still-live, still-trusted receiving WebView. Failure never grants a fallback
transport or operation.

### Shared cached providers

`ProviderRegistry` owns one controller for each new provider. Reads are
cache-only; controllers perform slow work asynchronously on GIO workers or the
GLib main context. Diagnostics serialization is provider-scoped: no tray item
list, notification body/history or audio volume/mute data is constructed for its
response.

| Key | Available state | Unavailable state |
| --- | --- | --- |
| `audio` | `{available:true,volume:number,muted:boolean}` | `{available:false,reason}` |
| `tray` | `{available:true,items:TrayItem[]}` | `{available:false,reason}` |
| `notifications` | `{available:true,unreadCount,activeCount,historyCount}` | `{available:false,reason}` |
| `applications` | `{available:true,count}` | `{available:false,reason}` |

Diagnostics receives `{available:true}` for audio and `{available:true,count}`
for tray; notification/application snapshots are already count-only. Provider
failure reasons and action errors are sanitized, without raw native error
details, command lines, private paths or environment values.

Icons use `{width,height,rgba}`: base64 tightly packed, unpremultiplied RGBA8,
with each dimension from 1 to 512 and bounded native input. Clients render
pixels on canvas. No image URL, SVG markup, arbitrary theme path or remote
resource is introduced; the existing CSP, local scheme and trust latch remain
unchanged.

### Audio

Use fixed asynchronous `wpctl get-volume @DEFAULT_AUDIO_SINK@`,
`set-volume ... <integer>%` and `set-mute ... 0|1`. A serialized lane covers
periodic reads, writes and fresh post-write observation. An exit status alone
does not report success. Poll every second, apply a five-second command deadline,
cancel pending work on shutdown, terminate active children and reap them with an
uncancellable wait.

Readings retain amplification above 100%; only writes are capped at 100.
Unavailable/malformed responses invalidate cached control state and later valid
observations recover it. The bridge exposes no microphone or device-selection
operation and no PulseAudio fallback. Default-output identity cannot be
correlated across an external default-sink change because `wpctl get-volume`
does not include a stable device ID; an observed mismatch fails the action.

### Tray and DBusMenu

Use `org.kde.StatusNotifierWatcher`/StatusNotifierHost/StatusNotifierItem.
Register an additional host with the existing watcher; queue non-replacing
watcher ownership for the absent/released-owner case. Never stop or replace the
desktop's watcher. Bind opaque IDs to native-observed unique owners and paths,
track registration/property/owner signals, and remove stale items and work.
There is no XEmbed or X11 fallback.

`TrayItem` contains `id,title,status,category,isMenu,hasMenu,tooltip,icon,
attentionIcon,overlayIcon`; text is plain and icons contain pixels, not paths.
SNI `Title` and fallback `Id`, category text, and the ToolTip title preserve
literal angle brackets, entity-like strings, and underscores. Filter disallowed
control characters and bound each displayed field to 4096 Unicode characters.
Only the ToolTip description allows markup in the
[SNI specification](https://specifications.freedesktop.org/status-notifier-item/latest/status-notifier-item.html#status-notifier-item-properties-tooltip);
convert that description to plain text without loading embedded resources.
Display-name underscores are not DBusMenu mnemonics.
`getTrayMenu` prepares root `0` or a cached enabled/visible submenu with
`AboutToShow`, then fetches the complete root with `GetLayout(0,-1,[])`. Preserve
the server's actual revision. Nodes contain `id,label,enabled,visible,type,
toggleType,toggleState,icon,children,hasSubmenu`, including lazy submenus.
Disabled, hidden, separator, submenu, unknown and stale clicks fail before
`Event`. `LayoutUpdated` and `ItemsPropertiesUpdated` invalidate the cache even
when the revision does not change.

DBusMenu `label` is literal menu text: preserve angle brackets and entity-like
strings such as `&lt;literal&gt;` and `&amp;value`, rather than parsing markup or
decoding entities. Normalize underscore mnemonics, escaped underscores, and
disallowed control characters within the text bound. ToolTip description
normalization remains separate. The reference menu renders labels through text
interpolation.

Primary, secondary, context-menu and scroll operations are separate exact APIs.
Native screen coordinates come from immutable caller monitor context, never
browser input. The reference UI opens menus in Quick Panel, not a DOM overlay
extending beyond the Bar's native surface.

### Notifications

Export `org.freedesktop.Notifications` at `/org/freedesktop/Notifications`, with
`GetCapabilities`, `Notify`, `CloseNotification` and `GetServerInformation`.
Request the name with `DO_NOT_QUEUE`, without replacement flags. An existing
daemon leaves this provider explicitly unavailable; a deliberate service
migration requires restarting the host. Do not read another daemon's private
history, stop it, or introduce a daemon-specific adapter.

Maintain at most 100 in-memory session records. Accept bounded literal text,
urgency, resident hints, supported action pairs and safe icon pixels/theme names.
No markup, URI launch, sound or persistence authority is granted. IDs are
unique, nonzero u32; same-sender active replacement keeps its ID, foreign active
replacement/close is rejected, and expired/unknown replacement requests get a
new ID. Replacement generation guards prevent stale timers.

`NotificationItem` contains `id,appName,summary,body,createdAt,active,read,urgency,
icon,actions`; timestamps are epoch milliseconds and urgency is
`low|normal|critical`. Zero expiry is sticky; `-1` uses five seconds, except
critical default notifications remain sticky. Expiry/user dismissal/client close
emit reasons 1/2/3. Action callbacks target the original unique sender;
non-resident actions close the record while resident actions remain active and
become read. Mark-read state is authoritative in the provider.

Shutdown removes timers/objects/observers and releases only this connection's
service ownership. It does not close the shared session-bus connection.

### Installed applications

Enumerate installed GIO desktop entries on a worker, transporting only owned
metadata/IDs. `AppInfo` is not Send; create/cache authoritative
`gio_unix::DesktopAppInfo` on GTK in yielded batches of eight and revalidate
visibility/launchability. Exclude Hidden/NoDisplay/unavailable TryExec and entries
with neither a nonempty native Exec nor `DBusActivatable=true`. Never parse or
expose Exec.

Search is localized, case-insensitive, all-token matching with deterministic
name/ID/description ranking and bounded results. `DesktopApplication` contains
`id,name,description,icon`; no filesystem, command or environment data is
returned. Catalog monitor invalidation clears the allowlist and cancels pending
launches until a fresh generation is ready. Removed IDs cannot be launched.

Launch the cached native object with an empty URI list and native GDK
`AppLaunchContext`. GIO owns Exec/DBus activation semantics. Errors are sanitized;
stale callbacks and shutdown cannot republish retired work.

### Backend verification — 2026-10-08

- `cargo fmt`, Clippy with `-D warnings`, 124 Rust tests and locked native build
  passed after integration.
- A throwaway harness compiled the production dispatcher/registry/controllers,
  without mock Bridge or provider implementations.
- An isolated session bus verified notification replacement and foreign-owner
  rejection, literal history, read state, Unicode/resident actions, real
  `ActionInvoked`/expiry/dismiss/client-close signals and service release.
- Real SNI/DBusMenu fixture objects verified standalone watcher registration,
  native ARGB-to-RGBA pixels, lazy submenu preparation, disabled/stale rejection,
  actual Activate/SecondaryActivate/ContextMenu/Scroll/Event calls and parameters,
  existing-watcher coexistence and non-replacing release/takeover, property
  updates, unique-owner disappearance and old-ID rejection.
- Isolated installed desktop entries verified executable/DBusActivatable
  inclusion, Hidden/NoDisplay/TryExec/missing-Exec exclusion, native launch with
  a marker-writing executable, catalog removal/addition and rejected retired IDs.
- The live desktop retained its existing watcher and notification daemon:
  11 tray items were observed, notification ownership conflict was explicit,
  and real audio was read/written at its unchanged 64%/unmuted state.
- Temporary subprocess fault injection verified malformed/unavailable audio,
  recovery through the real `wpctl`, five-second responsive deadline, killed-child
  reaping, active/queued cancellation and idempotent shutdown.

The hosted frontend checks below are separate from the backend results.
Deliberate real-session notification-daemon migration was not performed: the
existing service remains owned by the desktop.

### Reference frontend and hosted verification — 2026-10-08

- Bar renders default-output audio, tray icons/actions, and notification unread
  counts. Quick Panel lazily mounts Audio, System tray, Notifications, and
  Applications through PrimeVue tabs. Rust remains authoritative for provider
  state, IDs, revisions, caller permissions, and launch authority.
- `bun run typecheck`, the full 70-test Bun suite (13 files), and the production
  build passed. After the WebKit input fix, typecheck, all 10 launcher tests, and
  the production build passed again.
- A throwaway binary compiled the production host, manager, WebKit trust hooks,
  dispatcher, and provider modules. It loaded the actual built frontend through
  `html-desktop-shell://assets/index.html`, not a mocked browser bridge.
- On an isolated session bus, real SNI/DBusMenu objects verified canvas pixel
  content, native caller-derived context-menu coordinates, disabled nodes,
  stale-revision rejection with a visible error, refresh, and a real menu Event.
- Real WirePlumber observation and a UI volume write reused the observed volume,
  then checked fresh native state. No test intentionally changed user volume or
  mute settings.
- Standard Notify calls verified literal body rendering, native mark-read state,
  sender-targeted ActionInvoked, resident activity, dismissal, and refreshed
  history. A controlled installed entry was searched and launched through GIO,
  with a marker file proving the actual executable ran.
- Opening Diagnostics from Quick Panel closed the latter; Diagnostics exposed
  summary-only audio/tray state and rejected notification-history access.
  Host shutdown left no application windows.
- Current-session hosted checks rendered real audio, tray items, installed
  applications, notification ownership conflict, and Diagnostics reasons/counts.
  The existing watcher and notification service retained their unique owners
  before and after this read-only run; it launched no installed application.
- Native snapshots were inspected. Dark Tabs keep readable text independently
  of the system theme. The launcher's text input retains `role="searchbox"`:
  WebKit's native `type="search"` decoration had requested a `data:` SVG and
  revoked the per-view trust latch. The resource policy itself was not relaxed.
- Viewport checks accounted for fractional-scale CSS-pixel rounding rather than
  treating a one-pixel `innerWidth`/`scrollWidth` difference as layout overflow.
- Final current-state checks passed the full 70-test Bun suite, typecheck/build,
  `cargo fmt --check`, Clippy with `-D warnings`, 125 Rust tests, the locked native
  build, exact capability printing, and startup checks on two monitors.
- A DBusMenu label regression failed before the correction with deleted angle
  brackets and decoded literal entities, then passed after separating label text
  from SNI ToolTip markup handling. A further real DBus/hosted-WebKit run used
  raw `<literal>`, `&lt;literal&gt;`, and `&amp;value`, verified the exact native
  result and rendered Vue text, confirmed ToolTip markup still normalizes, and
  observed the actual menu Event while the bridge remained trusted.

These are dated checks, not standing pass claims. Accessibility-bus warnings
were observed; accessibility health and the full compositor
focus/geometry/hotplug/pointer-transparency matrix were not validated by these
desktop-capability runs.

### Release regression and acceptance — 2026-10-08 (in progress)

- An `item_view` regression reproduced SNI display-name loss: a literal
  `<Player>` was removed and entity-like strings were decoded. Display names,
  fallback `Id`, category, and ToolTip title now use bounded literal/control
  normalization, preserving underscores. Only ToolTip description is converted
  from markup. The regression and the Unicode-character bound check passed.
- `cargo fmt --check`, Clippy with `-D warnings`, all 127 Rust tests, the locked
  native build, and `--check` passed after this correction. Startup reported two
  monitors, and the existing current-niri Bar smoke passed on DP-2 and DP-3.
- A production-source GTK/WebKit host on an isolated, non-activating session bus
  observed actual SNI Title and missing-Title/Id fallback fixtures. Bridge state,
  Vue text interpolation, and painted native snapshots preserved raw angle
  brackets, entity-like strings, and single/doubled underscores on both outputs.
  ToolTip title stayed literal while its description normalized markup.
- Existing niri tuple validation passed Quick Panel open/duplicate prevention,
  caller-derived output context, Diagnostics handoff and summary privacy, and
  transient disappearance on both outputs. Host shutdown left no GTK windows.
- The user confirmed actual DP-3 unplug/reconnect: the process remained alive,
  DP-2 retained its Bar, and reconnect restored one Bar on each output. Captured
  compositor transitions corroborated the surviving process and restored set.
  Real-input focus/geometry/HUD pointer-transparency acceptance remains open.
  Accessibility warnings are not accessibility acceptance.
- Manual checks exposed missing DP-2 output metadata and duplicate workspace
  indices from other outputs. The host now observes delayed GDK connector
  metadata and coalesces transactional binding rebuilds; unchanged bindings
  preserve existing surfaces. The frontend never exposes workspace buttons
  without a known output. Formatting, Clippy, all 127 native tests, the locked
  build, frontend typecheck, all 70 web tests (426 assertions), and build passed.
- A production-source hosted check verified correct Bar, Quick Panel, and
  Diagnostics output context and rendered labels on both DP-2 and DP-3. Rendered
  workspace buttons matched only the corresponding output's cached publication.
  Existing watcher/notification owners remained unchanged.
- Fixed the Bar's 20-character tray-error clipping. Actual native rejection
  text rendered completely on both outputs at 32-pixel Bar height without
  horizontal overflow; cropped native snapshots confirmed the visible text.
- Fcitx5 exports DBusMenu but not the SNI `ContextMenu` method. Actual
  `getTrayMenu` reads succeeded through the production Bridge on both outputs.
  The selected correction is an independent layer-shell overlay and transparent
  outside-click capture on all active outputs, with the first click consumed.
  GtkPopover was an analyzed alternative at this stage. A later Bar-only native
  parent-relative popup superseded the Bar portion of that selection after real
  input exposed a coordinate-domain defect; the Quick overlay and capture layers
  are unchanged. Implementation and current-version acceptance are tracked by the
  v6 extension below.


## Native tray-menu extension — Bridge v6

### Contract and ownership

- `showTrayItemContextMenu({itemId})` opens exported DBusMenu in the host and
  returns `null`; it never calls vendor `ContextMenu` or creates an ordinary-window
  fallback. Missing menu availability remains an explicit native error.
- Five typed content surfaces share common trusted-WebView/lifecycle foundations,
  but Tray Menu owns a distinct view and, on Bar, a native parent-relative popup
  presentation instead of a layer namespace.
- `trayMenu` context binds `monitorIndex`, `output`, and `trayItemId` before content
  interaction. Browser input cannot select output, global coordinates, commands,
  menu owner, or a resize operation.
- Tray Menu allows exactly `getHostInfo`, `getCapabilities`, `getTrayMenu`,
  `activateTrayMenuItem`, and `closeTrayMenu`. The latter accepts `{}` and returns
  `null`; menu reads/actions must match the bound item, current owner, enabled
  node, and revision. General state and unrelated desktop actions are forbidden.
- The v6 method set has 21 names. Existing scopes remain Bar/Quick Panel 20,
  HUD two, Diagnostics four; no v5 compatibility shim remains.

### Native placement and input

- Capture the actual trigger before asynchronous menu work. Mouse input and
  focused/keyboard control geometry require independent observed evidence.
- Native trusted-document queries use `button[data-tray-id]` in Bar and
  `button[data-tray-menu-trigger]` in the Quick Panel row. Local bounds are
  transformed with native viewport scaling; the request still carries only item ID.
- Presentation is hybrid, sharing one Tray Menu authority and one independent
  trusted WebView per menu. A Bar menu is a native parent-relative
  `GtkPopover`/`xdg_popup` anchored to the triggered Bar control, with native
  autohide and grab instead of a full-output capture layer. It is a distinct owned
  native presentation, not an ordinary window and not a layer namespace; a stable
  native widget name identifies it for temporary inspection. A Quick Panel menu
  stays its existing Overlay/Exclusive zone-`0` top/right native workarea menu
  with all-output buffer-backed `-1` dismiss captures.
- Native configuration, margins, and captured local bounds establish placement for
  both presentations without guessing foreign exclusive-zone offsets, changing
  Quick Panel policy, or exposing a browser-selected parent, output, or coordinate.
- Native fitting measures bounded `#tray-menu-content` including `scrollHeight`,
  clamps to the current native coordinate domain, and allows tall-content
  scrolling. Neither frontend has a native-coordinate or generic resize API.
- Bar popup content reports a bounded natural size with a shrinkable native minimum.
  The compositor's configured allocation is authoritative; a tall WebView is
  allocated inside that limit and scrolls instead of imposing a larger GTK minimum.
  The native content widget forwards focus and preserves the exact owned WebView
  check through its single-child boundary.
- Native Bar capture routes Menu / Shift+F10 to the focused trusted tray button
  and stops WebKit's hidden `GtkTextView` from opening a competing editor menu.
  Bare F10 and other surface shortcuts are unchanged.
- The Quick menu namespace is `html-desktop-shell-tray-menu-{monitorIndex}`, layer
  Overlay, keyboard Exclusive. There is at most one live menu globally across both
  presentations.
- For the Quick menu, native-only transparent
  `html-desktop-shell-tray-dismiss-{monitorIndex}` layers cover every active output,
  including the caller output. They use raw zone `-1` to cover reserved panel
  regions without reserving space, keyboard None, and no WebView/Bridge. They map
  below the actual menu.
- Every close, Escape, focus loss, outside click, replacement, owner/menu loss,
  rebuild, or shutdown retires the live popup/menu, removes every Quick capture
  layer, and retires stale callbacks. The first outside press closes only; the
  underlying desktop does not receive it, and that consumed dismissal must be
  proven on both outputs for the Bar popup.
- Successful leaf activation completes before frontend self-close. Native close
  replies before identity-guarded retirement so successful replies are not revoked.
- Original Bar/Quick Panel/HUD/Diagnostics policies and service owners are unchanged.

### Presentation and acceptance

- Vue `TrayMenuView` and the plain-HTML menu branch have no general-state polling.
  They render literal labels, visible/enabled/separator/toggle state, lazy submenus,
  current revisions, wrapped native errors, refresh/close, and keyboard navigation.
- `useTrayMenu` shares read/revision/retirement handling with Quick Panel's preview;
  only standalone successful leaf activation requests popup close.
- Automated tuple validation recognizes only the Quick menu's layer namespace
  singleton and its keyboard/layer policy. A Bar menu is a native parent-relative
  popover that never enters the layer inventory, so the validator can neither
  confirm nor deny it and must not treat a missing namespace as a Bar popup
  presence pass. It deliberately does not classify auxiliary captures as content
  and cannot establish an actual popup, grab, geometry, focus, consumed clicks,
  or accessibility.
- Current checks and observed runtime evidence are recorded below. Older v4/v5
  and pre-connector-fix physical hotplug results above remain historical, not a
  pass for the changed runtime.

### Current v6 verification — 2026-10-09 (in progress)

#### Pre-popup observations

The following observations predate the Bar popup correction. They describe the
retained Quick layer path and shared menu machinery, not a Bar-popup pass. Actual
Bar-popup observations are recorded separately below.

- Pre-popup-correction automated baseline: locked Rust formatting/Clippy/build
  checks and all 139 native tests passed. Frontend typecheck, 78 tests / 499
  assertions / 14 files, production build, plain-HTML syntax checks, and 32 smoke
  tests / 99 assertions passed. Final post-popup counts are recorded below.
- A temporary production-source host exercised the actual DBus → trusted
  WebKit → Vue path. Menu scope rejected general state, another tray item,
  malformed close parameters, disabled leaves, and stale revisions. Literal
  labels remained text. Existing Quick self-close, Quick → Diagnostics handoff,
  and Diagnostics self-close still replied successfully on both outputs.
- Actual Fcitx had Id `Fcitx`, Title `输入法`, and exported `/MenuBar`.
  Its host-rendered menu opened and closed on DP-2 and DP-3 without an alert,
  vendor `ContextMenu`, or any executed Fcitx menu action. Both snapshots were
  inspected; the visible menu was 360 × 358 logical pixels.
- Real keyboard navigation expanded a fixture submenu, focused leaf 22, and
  delivered one DBusMenu `clicked` Event before popup/capture retirement. A real
  mouse click reached the actual popup above its fully rendered auxiliary layers
  and activated fixture leaf 11 before the same cleanup.
- Growing and then collapsing fixture content reproduced a nonshrinking popup.
  Removing vertical flex stretching fixed the natural-height measurement:
  the same native window measured 208 → 1692 → 208 logical pixels. Tall content
  scrolled; long unbroken labels did not overflow horizontally.
- Real pointer QA exposed an empty-snapshot defect: transparent dismiss windows
  had layer-shell roles but submitted no Wayland buffer and did not receive input.
  A transparent Cairo snapshot fixed the native surface. On both DP-2 and DP-3,
  pointer position `(40, 8)` within the reserved Bar region hit the corresponding
  dismiss layer. The first press closed the menu and all captures with unchanged
  underlay counters; the next press incremented only that output's counter.
- Earlier temporary QA underlays intercepted desktop clicks; they were stopped
  immediately and the user confirmed restored input. Subsequent fullscreen
  mouse/exclusive-keyboard segments required asktool approval, an independent
  hard limit of at most 30 seconds, and cleanup on normal completion, failure,
  or timeout. Bounded hosted runs ended with zero GTK windows and no project
  layers; watcher/notification service owners remained `:1.22`.
- The initial Quick probe reproduced GTK fixed-size clamping: Wayland configured
  4096 × 1656, but GDK reported 200 × 200 and rejected the anchor. Temporarily
  allowing resize during measurement, then restoring fixed menu size, produced
  the actual native workarea and a 360 × 208 popup. Real pointer clicks and
  keyboard Return independently opened that popup on both outputs after actual
  OnDemand focus acquisition. Observed workareas were 4096 × 1656 and
  3072 × 1656; top/right margins were 301/4 for the tested control. Escape
  removed every capture. Both rendered snapshots were inspected.
- Duplicate open retained its live native instance. Replacement bound the new
  item, rejected the old ID, and retained exactly two captures plus one menu.
  Exported-menu-path loss, unique-owner loss, and caller closure each removed
  all menu/input layers and restored original Bar keyboard policy.
- Current compositor-confirmed HUDs rendered the expected index/output on both
  outputs. Replacement changed the immutable context; the new HUD survived the
  old generation's deadline and disappeared after its own timeout. Both native
  HUDs had keyboard None and stayed inactive. Real center clicks reached only
  the underlying output's counter; both content snapshots were inspected.
- Bar keyboard invocation reproduced a WebKit default-shortcut gap. Explicit
  `ContextMenu` / Shift+F10 handling fixed it without taking over bare F10;
  actual focused-Bar Shift+F10 now opens the host menu.
- Observed Bar anchoring defect, resolved by the approved correction: with
  `dms:dankisland` reserving the top, native pointer feedback placed this Bar at
  output y=40. Its control bottom was y=68, so a below-control menu should start
  at y=72. Actual right click and Shift+F10 both opened the zone-`-1` menu at y=32
  because the local-to-output transform omitted the Bar's compositor displacement.
  [Layer-shell configure](https://github.com/swaywm/wlr-protocols/blob/master/unstable/wlr-layer-shell-unstable-v1.xml)
  supplies local width/height, not global origin, so the old all-output menu could
  not derive the Bar's real position without guessing `dms:dankisland`'s exclusive
  offsets. The user approved a Bar-only native parent-relative popup that anchors
  to the triggered Bar control through GTK/Wayland relative placement and never
  guesses a global origin; the accepted Quick overlay is unchanged.

#### Post-popup verification and deferred physical acceptance

- Final native checks passed formatting, locked all-target Clippy with
  `-D warnings`, all 141 Rust tests, the locked build, and startup `--check`
  with layer-shell support and two monitors. The integrated frontend passed
  typecheck, 78 tests / 499 assertions / 14 files, production build, plain-HTML
  syntax checks, and the root-run 32 smoke tests / 99 assertions.
- Actual Bar right click on DP-3 opened an autohide `GdkPopup` whose native parent
  matched the current Bar. For the fixture control at parent y=2, height=28, its
  parent-relative popup y=34 placed it at observed global y=74, matching control
  bottom plus the four-pixel gap under the existing top reservation. No foreign
  offset was hardcoded and no Bar dismiss layer was created.
- Real outside presses on DP-3 and DP-2 closed the Bar popup without changing
  either output's underlying click counter. The next press incremented only the
  target output's counter once. These are native Bar-grab observations, separate
  from the older layer-capture tests.
- Real Shift+F10 reopening on DP-2, End/Right submenu navigation, and Return
  delivered one `clicked` Event for leaf 22 before closing. Testing exposed a
  competing WebKit editor popover; native capture of the two Bar context-menu
  shortcuts fixed it without global context-menu suppression.
- The initial tall-popup implementation requested height 1720 as a GTK minimum.
  Wayland configured 1654 instead, after which GTK destroyed the popup. The
  shrinkable natural-size content widget fixed this reproduced failure:
  the same native popup grew 201 -> 1654 -> 201, with scrollHeight 2739 and equal
  scrollWidth/clientWidth 315, no error, and a successful `null` close reply.
- A retained old popup WebView rejected a new request after same-parent reopen
  with `Error: native surface has been retired`. A separately delayed real DBus
  layout request entered before retirement; its old view received no structured
  native payload, while the replacement still answered `getHostInfo`.
  WebKit settled the dropped reply with `undefined`, not a menu response.
- Forced compositor focus to an existing normal window retired the DP-3 popup
  through native autohide. Internal widget focus transitions and Bar parent
  deactivation are not used as substitutes for this seat-focus observation.
- Actual Fcitx menus were opened by real Bar right clicks on both DP-3 and DP-2.
  Each had the correct native parent, literal rendered labels, no error, no Bar
  capture layers, and a successful self-close reply. Both snapshots were inspected;
  no Fcitx menu action or vendor `ContextMenu` was executed.
- Hybrid checks preserved duplicate-open identity, replacement item authority,
  one global menu, and Quick's zone-0 Exclusive menu plus two zone--1/None
  captures. Quick caller closure removed the menu and captures. Popup path and
  owner loss retired the popup. Quick self-close, Diagnostics handoff, and
  Diagnostics self-close returned their successful synchronous responses.
- Every bounded input session removed its virtual mouse and native test layers
  on completion or failure. Final inventories reported zero owned windows and
  popovers and no remaining project layers. Watcher and notification service
  owners remained `:1.22`.
- The final production binary was also observed with one Top/OnDemand Bar on
  each of DP-2 and DP-3. The user deferred physical unplug/reconnect on
  2026-10-09; no output removal was observed in this run. The read-only supervisor
  reached its 150-second deadline, the host exited 0, and final content/capture
  inventories were empty.

Post-fix physical hotplug remains the release gate. Earlier physical observations
remain historical and cannot substitute. Full release acceptance is not complete.
