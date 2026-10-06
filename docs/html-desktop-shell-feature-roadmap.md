# HTML Desktop Shell Feature Roadmap

## Audited baseline

The completed foundation and typed multi-surface delivery establish the current
implementation:

- a Rust 2024, single-process GTK4/WebKitGTK Wayland layer-shell host;
- one persistent typed Bar per GDK monitor plus independently secured Quick
  Panel, Workspace HUD, and Diagnostics Dialog layer-shell surfaces;
- a shared `ProviderRegistry` serving clock, host, niri, battery, and network
  state;
- deny-by-default Bridge v4 with six exact methods and a surface-scoped
  capability matrix;
- niri snapshots and compositor-confirmed workspace activations published by one
  contained GIO event-stream collector with explicit unavailable/recovery state
  and bounded reconnects;
- native configuration limited to `panel_height`, `layer`, and
  `keyboard_mode`, with closed native policy for transient geometry and input;
- canonical six-tier lookup for local static frontend assets, served through a
  read-only custom scheme with enforced navigation, origin, CSP, and per-view
  bridge trust policy;
- an official Vue 3/Vite/Bun frontend using Pinia, `SurfaceRoot`, per-view
  polling, and eight compile-time Bar layout widgets;
- a framework-independent Bridge v4 contract, with `examples/plain-html`
  demonstrating a custom frontend.

The previously measured 500 ms polling cache was retired on 2026-07-23 after reducer/lifecycle tests, fake-child recovery and shutdown checks, and a current-session niri interaction check. Historical polling measurements are not treated as current performance results.

## Invariants

All future work must preserve these boundaries:

- The host is Wayland `zwlr_layer_shell_v1` only. There is no X11, raw-TTY, compositor-less, Electron, or normal GTK window fallback.
- The frontend policy remains local-only and reproducibly bundled. The host enforces canonical-root asset containment, rejects remote scripts, fonts, icons, and other runtime resources, and gates the bridge to the exact trusted entry document.
- The bridge is deny-by-default. Every privileged operation has one exact method, one exact parameter schema, and an explicit UI caller.
- Generic escape hatches such as command execution, filesystem access, arbitrary HTTP, DBus, evaluation, or generic niri actions are prohibited.
- Custom frontends remain static asset directories using the framework-independent bridge contract.
- Optional provider failures produce explicit unavailable state without preventing panel startup.
- Native configuration remains limited to panel/window behavior; widget layout and presentation remain frontend-owned.

## Prioritized future work

### P0 — Correct the workspace ID/index contract (completed)

Status: completed on 2026-07-23. Native and frontend contract tests cover an ID/index mismatch fixture, and a current-session niri workspace-switch check passed.

The bridge, controller, shell API, and frontend now use `WorkspaceSnapshot.index` unambiguously as `workspaceIndex`; the distinct niri workspace ID is never substituted for the CLI workspace reference.

Acceptance criteria:

- One identifier is defined unambiguously across the niri snapshot, bridge schema, controller validation, command invocation, shell API types, and frontend caller.
- The workspace button sends that identifier without conversion or substitution.
- Controller validation checks the same snapshot field represented by the bridge parameter.
- Native tests cover a snapshot where workspace ID and index differ, accepting the valid identifier and rejecting an unknown one.
- Frontend and shell API tests assert the exact `niriFocusWorkspace` request payload for an ID/index mismatch fixture.

### P1 — Restrict trusted WebView navigation and origin (completed)

Status: completed on 2026-07-23. Rust policy tests pass; current-session niri checks passed for the official frontend, `examples/plain-html`, a blocked HTTP subresource probe, and bridge revocation after attempted remote navigation.

The privileged bridge is constrained to the canonical local frontend root without breaking supported local custom frontend paths.

Acceptance criteria:

- The initial document is accepted only from the resolved local asset root selected by the existing six-tier lookup.
- Top-level navigation, redirects, popups, and subresource origins outside the trusted local policy are rejected before they can access the bridge.
- Bridge requests are processed only for a trusted document/origin, including after attempted navigation.
- Tests cover the official bundled frontend and every supported local custom frontend root, plus rejected remote, sibling-file, traversal, redirect, and popup cases.
- No network or fallback asset source is introduced.

### P1 — Move niri collection off the GTK main loop (completed)

Status: completed on 2026-07-23. Rust tests and clippy pass; a fake malformed/partial stream recovered coherently on its third generation without a leaked child, a real niri interaction check passed, and lifecycle review found no blocker, high, or medium issue.

The shared provider now reads a coherent cache published by a contained GIO event-stream collector; no niri collection subprocess is synchronously awaited on the GTK main loop.

Acceptance criteria:

- Provider reads and refresh scheduling perform no synchronous niri subprocess wait on the GTK main thread.
- A contained niri collector owns event-stream startup, snapshot updates, shutdown, and bounded reconnect behavior.
- The bridge reads a coherent cached snapshot without waiting for niri IPC.
- Compositor loss, malformed events, collector exit, restart, and application shutdown are tested without hangs, leaked children, stale writes after shutdown, or panics.
- Unavailable and recovery transitions are explicit, while the shared provider contract remains unchanged.
- The replacement is adopted only after lifecycle tests pass and an audited comparison justifies retiring the contained 500 ms polling cache.

### P2 — Strengthen interaction and smoke coverage (completed)

Status: completed on 2026-07-23. Mounted Vue tests cover the real Pinia-to-bridge interaction path and polling lifecycle; the validator and helper tests cover success, mismatch, fatal input, deadline, early-exit, and process-group cleanup paths; a current-session two-output niri smoke and manual render/exclusive-zone gate passed.

Acceptance criteria:

- Mounted frontend tests exercise workspace click and keyboard activation, exact bridge payloads, visible action failures, polling continuity, and unavailable state through rendered components rather than view-model-only assertions.
- The smoke helper exits nonzero unless expected layer-shell namespace and per-monitor surface expectations are observed.
- Smoke assertions distinguish missing, duplicate, and incorrectly named surfaces and print actionable diagnostics.
- Documentation records smoke commands as release gates, not as claims that they currently pass.

### P2 — Add product widgets or actions only through exact APIs (policy enforced)

Status: policy enforced through the Bridge v4 delivery on 2026-08-02. The only
new write methods are the exact typed surface operations
`setQuickPanelOpen({ open })` and `setDiagnosticsDialogOpen({ open })`; unknown
methods remain denied, and every future proposal remains subject to the
criteria below.

New product features are considered individually after the correctness and isolation work above.

Acceptance criteria for any proposal:

- The user-visible need, provider data, capability advertisement, exact bridge method and JSON schema, explicit caller, unavailable behavior, and authorization boundary are specified before implementation.
- Read-only data extends a narrow provider snapshot where possible.
- Each write action receives its own allowlisted method; generic command, DBus, filesystem, HTTP, or compositor-action methods are rejected.
- Native contract tests and mounted caller tests cover allowed, malformed, unavailable, and denied behavior.
- Adding or omitting the feature does not weaken local-assets-only loading or custom frontend support.

### P3 — Add typed multi-surface shell surfaces (completed)

Status: completed on 2026-10-06, following the initial implementation delivery
on 2026-08-02. Final release commands passed: frozen Bun install, web type-check,
all 30 web tests (119 assertions), and Vite build; all 31 smoke-validator tests
(94 assertions); `cargo fmt --check`, clippy with `-D warnings`, all 80 Rust
tests, and the locked Rust build. The two-output persistent Bar smoke passed on
DP-2 and DP-3.

The final post-fix two-output matrix passed on 2026-10-06. Compositor observations
validated exact Quick Panel, HUD, and Diagnostics namespace/output/layer/keyboard
tuples on both outputs. User observations confirmed rendered content, geometry,
focus, Escape/explicit/focus-loss closure, Diagnostics handoff, HUD replacement
and timeout, pointer click-through, and no keyboard-focus capture. Physical
DP-3 unplug/reconnect preserved the process and restored the correct Bar set.
SIGINT and SIGTERM each exited normally with no remaining shell layers or
recorded child processes.

The matrix also found and resolved output-scope and lifecycle defects:
`niri.focusedWindows.items` now follows each output's active workspace/window;
the Bar's Niri label uses its immutable surface output rather than global focus;
and a GIO application hold keeps the host alive through runtime window gaps.
Request-envelope and privileged-param object checks also reject serde's array
representation of structs before dispatch. Array-rejection regressions and a
direct production-dispatcher smoke passed, followed by the native release gates
on 2026-10-06.

Detailed execution steps, exact bridge schemas, native/frontend file
boundaries, lifecycle rules, verification matrices, and dated delivery evidence
are recorded in
[`html-desktop-shell-multi-surface-implementation-plan.md`](html-desktop-shell-multi-surface-implementation-plan.md).

Native surface contract:

- Introduce a closed `SurfaceKind` model for `Bar`, `QuickPanel`, `WorkspaceHud`, and `DiagnosticsDialog`; do not add generic create/show/close-window methods.
- Keep the existing Bar as one persistent top-layer surface per active output with its configured exclusive zone.
- Create every Quick Panel, HUD, and dialog as an independent layer-shell surface, never a normal GTK window or `GtkDialog`.
- Quick Panel uses the overlay layer, top/right placement, zero exclusive zone, and on-demand keyboard interaction.
- Workspace HUD uses the overlay layer, centered placement, zero exclusive zone, and no keyboard interaction.
- Diagnostics Dialog uses the overlay layer, centered placement, zero exclusive zone, and on-demand keyboard interaction.
- One typed surface registry owns per-output uniqueness, monitor hotplug removal, transient timers, focus/Escape closure, and idempotent application shutdown.
- All surfaces share one `ProviderRegistry` and `TrustedWebContext`, while every WebView keeps an independent document trust latch and exact caller context.

Bridge v4 contract:

- Replace `HostInfo.panel` with a discriminated `surface` context containing the exact surface kind, monitor index, and output; the Workspace HUD variant also carries its exact workspace index.
- Migrate the Rust protocol, framework-independent shell API, official frontend, plain HTML example, tests, and documentation as a clean cutover without a bridge v3 compatibility shim.
- Add only `setQuickPanelOpen({ open })` and `setDiagnosticsDialogOpen({ open })` as new write methods. Native code derives the target output from the trusted caller context.
- Advertise a surface-scoped capability allowlist. Bar, Quick Panel, HUD, and Diagnostics callers must not receive methods they are not authorized to use.
- Do not add generic window, HUD, command, compositor-action, filesystem, HTTP, DBus, process, or session-control access.

Quick Panel acceptance criteria:

- A Bar control toggles exactly one Quick Panel on the caller's output; repeated open/close requests are idempotent.
- The first panel reuses existing workspace, focused-window, network, battery, host, and unavailable state. It introduces no audio, tray, notification, launcher, or session-control provider.
- Existing workspace focus remains the only product action inside the first panel.
- Escape, focus loss, monitor removal, and application shutdown close the panel without leaked surfaces or stale callbacks.
- Mounted caller tests and native contract tests cover exact payloads, authorization, visible errors, duplicate prevention, and lifecycle cleanup.

Workspace HUD acceptance criteria:

- After initial synchronization, the niri reducer emits a narrow typed workspace-activation notification containing the compositor-confirmed output and workspace index whenever the focused workspace actually changes. Initial/recovery snapshots and duplicate activation events do not display a HUD.
- `SurfaceManager` consumes that typed notification and creates or replaces one non-interactive HUD on the affected output. Keyboard shortcuts and other niri clients therefore behave the same as the Bar action, while `niriFocusWorkspace` only requests the change and never treats subprocess success as confirmed compositor state.
- The HUD renders the typed workspace context, takes no pointer or keyboard focus, reserves no exclusive zone, and closes through a generation-guarded GLib timer without adding a generic `showHud` method.
- After the HUD's `GdkSurface` is realized, native code applies an empty input region so the Wayland surface is pointer-transparent. `KeyboardMode::None` or `Widget::set_can_target(false)` alone is not accepted as pointer pass-through.
- External workspace changes, failed focus actions, duplicate events, reconnects, monitor removal, timer races, and shutdown are tested so an old callback cannot close or mutate a newer HUD.

Diagnostics Dialog acceptance criteria:

- An explicit Bar or Quick Panel control opens one read-only Diagnostics Dialog on the caller's output through `setDiagnosticsDialogOpen`.
- The dialog renders existing host, capability, surface, and provider availability data without exposing asset paths, arbitrary environment values, logs, filesystem access, or new privileged operations.
- Escape, focus loss, explicit close, monitor removal, and shutdown are deterministic and idempotent.

Verification gates:

- Native tests cover the surface registry state machine, surface-scoped authorization, monitor hotplug, timer generations, and shutdown.
- Mounted Vue tests cover Quick Panel toggle and workspace interaction, HUD rendering, Diagnostics unavailable state, keyboard closure, visible failures, and timer/poll cleanup.
- Compositor smoke checks assert the persistent Bar set and, while each transient surface is open, its exact namespace, output, layer, keyboard mode, uniqueness, and disappearance after close.
- Manual two-output checks cover geometry, focus behavior, rendered content, zero exclusive zones for transient surfaces, HUD input transparency, and the unchanged 32px Bar exclusive zone.

## Deferred high-surface features

These remain intentionally deferred until a separate design establishes a narrow security and lifecycle boundary:

- StatusNotifierItem/system tray integration;
- notification daemon behavior;
- audio controls through PipeWire, WirePlumber, or other session services;
- launcher/search and process-launch policy;
- broad DBus or desktop-session control;
- a runtime plugin system or generic native extension API;
- a full theming/configuration system.
