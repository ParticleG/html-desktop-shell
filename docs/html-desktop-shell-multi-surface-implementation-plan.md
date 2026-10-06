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
