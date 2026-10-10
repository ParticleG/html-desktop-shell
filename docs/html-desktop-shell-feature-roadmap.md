# HTML Desktop Shell Feature Roadmap

## Audited baseline

The completed foundation, typed multi-surface delivery, and desktop-capability
extension establish the audited v5 baseline before the v6 tray-menu correction:

- a Rust 2024, single-process GTK4/WebKitGTK Wayland layer-shell host;
- one persistent typed Bar per GDK monitor plus independently secured Quick
  Panel, Workspace HUD, and Diagnostics Dialog layer-shell surfaces;
- a shared `ProviderRegistry` serving system/niri state plus native audio,
  tray/menu, notification, and installed-application state;
- deny-by-default Bridge v5 with 20 exact methods and a surface-scoped
  capability matrix;
- niri snapshots and compositor-confirmed workspace activations published by one
  contained GIO event-stream collector with explicit unavailable/recovery state
  and bounded reconnects;
- native configuration limited to `panel_height`, `layer`, and
  `keyboard_mode`, with closed native policy for transient geometry and input;
- canonical six-tier lookup for local static frontend assets, served through a
  read-only custom scheme with enforced navigation, origin, CSP, and per-view
  bridge trust policy;
- an official Vue 3/Vite/Bun reference frontend using Pinia, `SurfaceRoot`,
  per-view polling, eight configured status widgets, and desktop controls;
- a framework-independent Bridge v5 contract, with `examples/plain-html`
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

Status: policy enforced through the typed surface delivery and the approved
Bridge v5 desktop extension. Surface and desktop writes use separate exact
typed operations; unknown methods remain denied, and every future proposal
remains subject to the criteria below.

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

### P4 — Framework-independent desktop capabilities (completed)

Approved on 2026-10-08. Deliver complete native audio control, StatusNotifierItem
tray integration, a standard notification server/center, and installed desktop
application launch/search before adding reference controls to `web/`.

The core is the native host and public capability protocol, not the Vue UI.
`web/src/` is a replaceable reference frontend; the shell API remains independent
of Vue, Pinia, layout, and component lifecycle. Other local static frontends use
the same contract and native authorization.

Boundaries:

- Audio controls only the default WirePlumber output through fixed asynchronous
  `wpctl` operations; volume writes are integer percentages from 0 to 100.
- Tray discovery uses the standard watcher/host protocol. Join an existing
  watcher; provide one only when absent. Render bounded native icon pixels and
  DBusMenu trees without accepting browser-selected bus names, paths, or image
  URLs.
- The notification server implements `org.freedesktop.Notifications` without
  replacing another daemon. An existing owner produces explicit unavailable
  state. Keep bounded session-only history, literal text, expiry, dismissal,
  read state, and sender-bound actions; no persistence, sound, or URI launch.
- Application search uses the installed GIO desktop-entry database. Launch only
  IDs in the current native visible/launchable allowlist, through GIO with no
  browser-supplied command, argument, file, environment, or URI.
- Bridge v5 adds exact typed methods. Bar and Quick Panel receive the desktop
  operations; HUD remains read-only host/capabilities; Diagnostics receives
  availability/count summaries, not notification bodies or application paths.
- New IPC and launch operations are asynchronous; provider reads remain
  cache-only. Shutdown cancels collectors, pending operations, timers, bus
  registrations, and observation handlers.
- The reference frontend reuses Bar and Quick Panel rather than adding a generic
  window API or making Vue state authoritative for native business logic.

Acceptance:

- Verify real DBus registration, notifications, tray item/menu signals, actions,
  ownership conflicts and shutdown on isolated session buses.
- Verify actual WirePlumber state/actions and unavailable/recovery behavior.
- Verify controlled installed desktop-entry search/launch, removal, unknown-ID
  rejection, and no leaked command lines or filenames.
- Reject malformed, array, unknown-field, unavailable, stale and unauthorized
  requests before privileged dispatch.
- Migrate SDK, official frontend, framework-free example and documentation as a
  clean v5 cutover. Preserve the prior four-surface lifecycle and local trust
  policy.
- Exercise the Vue reference controls through mounted caller tests and rendered
  runtime checks, including unavailable state and visible action errors.

Delivery verification on 2026-10-08:

- Native implementation/contract coverage passed 124 Rust tests, Clippy, and the
  locked build; real DBus/subprocess/catalog/launch checks were completed before
  the reference frontend.
- The 70-test Bun suite, typecheck, and production frontend build passed. After
  changing the WebKit search input, all 10 launcher tests and typecheck/build
  passed again.
- A real GTK/WebKit host using production source and the trusted local scheme
  exercised Bar/Quick Panel controls, native canvas pixels, fresh audio writes,
  literal notification/read/action/dismiss flow, revision-bound tray menus,
  actual GIO application launch, and restricted Diagnostics.
- Current-session rendered checks showed real tray/audio/application state and
  the existing notification-daemon conflict. Watcher/notification owners were
  unchanged before and after the run; no daemon was stopped or replaced.
- Runtime checks caught and fixed low-contrast Tabs and WebKit's native search
  decoration requesting a `data:` SVG. The launcher retains searchbox semantics
  without relaxing the local-only resource policy.
- Final current-state checks passed 125 Rust tests and the complete 70-test Bun
  suite with typecheck, builds, formatting, and Clippy. A failing-before/passing-
  after DBusMenu regression and real hosted run verified literal angle brackets
  and entity strings survive native parsing and Vue rendering; SNI ToolTip
  markup remains separately normalized.

These checks do not claim deliberate notification-daemon migration,
accessibility-bus health, or the separate full compositor focus/geometry/hotplug/
pointer-transparency release matrix. Backend and rendered verification details
are recorded in the implementation plan.

### Post-P4 — Regression and release acceptance (in progress)

The approved next stage covers current-version regressions and the existing
two-output surface matrix, not new privileged APIs or desktop-service migration.

On 2026-10-08, SNI display-name/ToolTip-title literal normalization was corrected
and all 127 Rust tests, formatting, Clippy, the locked build, startup checks,
and the existing two-output Bar smoke passed. Real SNI/Bridge/Vue/native-snapshot
checks and Quick Panel/Diagnostics tuple, handoff, and close checks passed on
DP-2 and DP-3.

The user confirmed physical DP-3 unplug/reconnect for the tested binary. Manual
checks also exposed delayed native output metadata, cross-output duplicate
workspace indices, and application-owned tray context windows. Output binding
and workspace scoping were corrected and verified through production Bridge/UI
checks on both outputs; all 127 native and 70 web tests passed afterward.

The tray correction is a hybrid inside the accepted Bridge v6 boundary. The
shared Tray Menu authority is unchanged. A Bar menu is a native parent-relative
`GtkPopover`/`xdg_popup` with native autohide and grab; a Quick Panel menu keeps
its host-rendered Overlay/Exclusive zone-`0` workarea overlay with transparent
native input layers over all active outputs. In both presentations the first
outside press closes only and is consumed, never reaching the desktop. Bridge v6
adds exact menu identity/self-close without broadening existing scopes. No menu
or capture layer reserves desktop space.

Post-popup checks passed: 141 Rust tests, formatting, locked all-target Clippy,
locked build/startup, 78 web tests / 499 assertions with typecheck/build, and
32 smoke tests / 99 assertions. Earlier Quick workarea/input and HUD
content/replacement/timeout/pointer-transparency observations remain scoped to
those surfaces; actual Bar-popup evidence was collected separately.

Real Bar pointer and keyboard QA exposed a coordinate-domain defect when another
Top layer reserves space: the actual Bar moved to y=40, but its all-output
zone-`-1` menu omitted that displacement, opening at y=32 instead of y=72,
because layer-shell configure supplies local width/height and not the global
origin. The user approved the Bar-only native parent-relative popup, which
resolves that boundary without guessing `dms:dankisland`'s exclusive offsets;
foreign exclusive offsets are never inferred. Actual Bar-popup checks now cover
native parent-relative placement, right-click and keyboard invocation, submenu
action-before-close, outside-press consumption on both outputs, retained-old-view
denial, delayed-reply isolation, forced external focus loss, path/owner retirement,
and same-popup constrained growth/collapse. Fcitx popup content was inspected on
both outputs without executing its actions. Shared Quick singleton/capture
lifecycle and synchronous caller-retiring replies were also observed.

The final production host showed one correct Bar per output and clean shutdown,
but the user deferred post-fix physical unplug/reconnect on 2026-10-09. Physical
hotplug remains unobserved for the corrected runtime; earlier observations are
not substituted. The exact contract and evidence are in the implementation plan.
Full release acceptance is not complete.

## Deferred high-surface features

These remain intentionally deferred until a separate design establishes a narrow security and lifecycle boundary:

- broad DBus or desktop-session control;
- a runtime plugin system or generic native extension API;
- a full theming/configuration system.
