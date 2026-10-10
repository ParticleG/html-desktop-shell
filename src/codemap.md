# Native Rust Host (`src/`)

## Responsibility

This directory implements the native process: configuration/assets, GTK activation, five typed WebKit content surfaces, a Bar parent-relative tray popover plus native-only Quick menu dismiss layers, scoped Bridge v6, and shared providers. Desktop logic remains framework-independent; diagnostic modes exit before the application loop.

## Entry Point and Module Map

- `main.rs` — process entry point. `main()` owns startup error handling, diagnostic short-circuiting, `gtk4::Application` creation, activation, SIGINT/SIGTERM main-loop sources, and the lifetime of `ShellHost`; it declares `surface`, `surface_manager`, and `surface_window` as the native surface modules.
- `config.rs` — command-line and TOML configuration pipeline. `load()` returns `LoadedConfig { config, diagnostics, app_args }`; `ShellConfig` carries panel height, layer, and keyboard mode used by Bar policy.
- `diagnostics.rs` — `DiagnosticFlags` modes for capability JSON, effective config TOML, and environment checks covering GTK initialization, layer-shell support, web assets, and monitor discovery.
- `assets.rs` — six-tier asset resolver. `web_assets()` selects the first existing `index.html`, rejects an unsafe selected entry without fallthrough, and returns canonical root-contained `ResolvedWebAssets`.
- `shell_host.rs` — application/manager boundary. `ShellHost` owns `Rc<RefCell<SurfaceManager>>`, the live GDK monitor model, its `items_changed` handler, per-monitor connector observers, one coalesced rebuild source, and a GIO application hold; it delegates presentation, topology rebuild, and shutdown to the manager.
- `surface.rs` — typed identity and immutable context, including native-bound `TrayMenu.trayItemId`; keys never collapse into an untyped window list.
- `surface_manager.rs` — all native surface ownership and orchestration. `SurfaceManager` owns monitor bindings, persistent Bars, the typed transient `LifecycleRegistry`, HUD generations/timers, and provider activation observation. Global Tray Menu state lives in `TrayMenuState`, which holds a `TrayMenuInstance` rather than occupying the normal transient registry. `present_tray_menu()` selects the Bar parent-relative popup path or the unchanged Quick layer path; `advance_tray_popover_geometry()` verifies the mapped surface is an autohide `GdkPopup` with the Bar's actual native parent surface before loading its already-owned trusted TrayMenu WebView, while `advance_tray_menu_geometry()` keeps the Quick workarea/capture state machine. `surface_view_is_current()` validates the exact presentation, WebView, and context, and `retire_tray_menu_view()`/`retire_tray_menu_presentation()` guard close. It also owns temporarily suspended keyboard policies and the fitting timer.
- `surface_window.rs` — shared typed window factory. `SurfaceInstance` pairs context and window; `surface_window_for_monitor()` applies per-kind layer-shell policy and namespace, creates a secure WebView, attaches weak lifecycle/Bridge callbacks, and loads the trusted entry URI. `TrayMenuPresentation::{LayerWindow, ParentPopup}` and `TrayMenuInstance` distinguish the Quick layer menu from the Bar popup. `tray_popover_for_parent()` builds the Bar parent-relative `GtkPopover` with native autohide/closed/grab and no layer namespace. Its single-child `TrayMenuContent` widget reports natural rather than hard-minimum dimensions, accepts compositor-constrained allocation, forwards focus, and preserves the exact owned WebView boundary.
- `tray_overlay.rs` — trusted local control measurement and native viewport conversion, typed menu placement/workarea conversion, bounded content fitting, and buffer-backed native dismiss windows without a WebView or Bridge. `MenuAnchorSpace::{Parent, Output}` selects parent-relative (Bar) versus output-domain (Quick) anchoring; `capture_anchor()`, `menu_placement()`, `apply_placement()`, `popover_size()`, `apply_popover_size()`, and `dismiss_window()` implement the shared and Quick-only paths.
- `tray_overlay.rs::TriggerCapture` — captures native pointer/key trigger state before asynchronous menu work. On Bar only, native Menu / Shift+F10 forwards to the focused trusted tray button and prevents WebKit's hidden editor popover from competing for the popup grab; bare F10 is unaffected.
- `web_security.rs` — local-content security boundary. `TrustedWebContext` serves the read-only custom asset scheme and creates per-view trust latches with navigation, popup, response, load, URI, failure, and resource enforcement hooks.
- `bridge.rs` — trusted WebKit request/reply adapter; captures immutable surface identity and weak manager ownership, dispatches strict typed operations, derives native point/launch context and rechecks live view/trust before asynchronous replies.
- `messages.rs` — Bridge v6's 21 exact methods, caller/ID-scoped capability matrix, strict parameters, and structured replies.
- [`providers/`](providers/codemap.md) — shared Niri/system/desktop state, exact workspace/audio/tray/notification/application operations, native icon pixels and cancellable lifecycle.

## Design and Patterns

- **Host-to-manager ownership:** `main()` retains `ShellHost`; the host retains one manager, monitor/connector observers, a cancellable pending rebuild, and an application hold. Connector metadata may arrive after monitor enumeration, so notifications rebuild immutable contexts rather than mutating WebView caller identity. Only `SurfaceManager` owns windows and provider shutdown; queued callbacks retain weak manager ownership.
- **Typed registry:** five exact content kinds have immutable monitor/output context; HUD adds workspace index, Tray Menu adds a native-bound item ID. The Bar menu is a native parent-relative popup owned separately from the Quick layer menu; the Quick menu's auxiliary dismiss windows have no web context or Bridge. Persistent Bars remain separate from transient ownership.
- **Atomic topology rebuild:** `rebuild_for_monitors()` collects indexed valid monitors and preserves the existing set when monitor identity, index, and output are unchanged. For changed bindings, it constructs every replacement Bar first, swaps current bindings/Bars, resets transient/HUD lifecycle, removes timers, closes old instances, and presents replacements. Construction failure preserves the current set.
- **Transient lifecycle:** Quick Panel and Diagnostics open idempotently on the caller's current binding and close idempotently. Both close on Escape or after losing an acquired active state. Opening Diagnostics from Quick also closes that Quick Panel. Close callbacks remove only the registry instance whose GTK window identity still matches, preventing stale callbacks from deleting replacements.
- **Compositor-confirmed HUD:** the provider observer emits only later focused-workspace changes. `show_workspace_hud()` resolves the activation output against current bindings, advances a per-monitor generation, destroys the preceding HUD and timer, creates a context containing the confirmed workspace index, and schedules a 1.5-second close. Generation checks make old callbacks harmless.
- **Per-kind native policy:** existing Bar/Quick/HUD/Diagnostics policies remain unchanged. A Bar tray menu is a native parent-relative `GtkPopover`/`xdg_popup` anchored to the triggered Bar control with native autohide/grab and no layer namespace; the Quick Panel menu remains a trigger-anchored Overlay/Exclusive zone-`0` workarea layer, and native-only transparent zone-`-1` dismiss layers cover every active output without keyboard focus. Neither presentation reserves desktop space, and neither guesses a foreign client's exclusive offsets or global origin. A transparent Cairo snapshot submits a real buffer; GTK mapping flags alone do not prove input capture.
- **Bridge v6 scoped authority:** Bar/Quick allow 20 methods, HUD two, Diagnostics four, Tray Menu five. Menu reads/activation must match its captured item ID and current native owner/revision; general state and other actions are forbidden.
- **No generic native authority:** operations never expose arbitrary commands, arguments, paths, URIs, DBus identities, monitor coordinates or session control. Desktop caches authorize IDs/revisions; surface policy stays native-owned.
- **Shared provider registry:** all surfaces clone one `ProviderRegistry`; reads clone the same atomic Niri cache without waiting for compositor IPC. Provider details, activation suppression, and shutdown are in the [provider map](providers/codemap.md).
- **Canonical custom-scheme assets and one-shot trust:** `ResolvedWebAssets` restricts requests to regular contained files under the exact scheme authority. Each view trusts only the committed initial entry navigation; redirects, popups, URI/response mismatch, untrusted resources, failures, or unexpected policy decisions permanently revoke Bridge access.
- **Fail-fast startup, degradable runtime:** invalid configuration, unsupported layer shell, missing display/assets/usable monitors, and initial Bar failure reach the process boundary. Later bridge/load/topology/transient failures are logged or returned as structured errors without granting fallback authority.

## Data and Control Flow

### Normal application startup and topology

1. `main()` loads configuration, runs requested diagnostics, constructs `gtk4::Application`, registers activation, and enters `run_with_args()`.
2. `ShellHost::new()` validates layer-shell/display availability, gets `display.monitors()`, creates one `ProviderRegistry` and `TrustedWebContext`, then calls `SurfaceManager::new()`.
3. The manager's `build_initial_bars()` delegates to `rebuild_for_monitors()`, which creates a `SurfaceContext::Bar` and `SurfaceInstance` for each valid indexed monitor.
4. The manager connects the provider's workspace-activation observer. `ShellHost` observes monitor-list and connector-property changes and coalesces rebuilds on the GLib idle queue, including startup metadata reconciliation.
5. `ShellHost::present()` delegates to `SurfaceManager::present_bars()`. Subsequent topology events build replacements, swap current bindings/Bars, cancel and clear all transient timers and generations, close old transients and Bars, then present the new Bars.

### Typed surface construction and operations

1. `surface_window_for_monitor()` creates a decorated-free layer-shell window, binds its monitor, derives the namespace from `SurfaceKind::namespace_prefix()` and `monitor_index`, and applies the kind's geometry, layer, exclusive-zone, keyboard, focus-close, and input policy.
2. It obtains a separately latched secure WebView, installs lifecycle hooks carrying `SurfaceKey` plus a weak manager, attaches Bridge callbacks with the complete `SurfaceContext`, loads `TRUSTED_ENTRY_URI`, and returns `SurfaceInstance`.
3. `setQuickPanelOpen` and `setDiagnosticsDialogOpen` first call `current_binding()` to reject stale callers. Open paths build one typed transient from retained manager dependencies and insert it only if the context remains current; close paths remove only the requested key.
4. `showTrayItemContextMenu` captures the trigger and calls `present_tray_menu()`. A Bar caller selects `TrayMenuPresentation::ParentPopup` and builds a `tray_popover_for_parent()` menu anchored with `MenuAnchorSpace::Parent`; `advance_tray_popover_geometry()` confirms the mapped autohide `GdkPopup` has the Bar's actual native parent surface before loading its already-owned trusted WebView. A Quick Panel caller keeps the `LayerWindow` path with `MenuAnchorSpace::Output`, `advance_tray_menu_geometry()`, and all-output dismiss layers. The first outside press closes and is consumed on both outputs, and `surface_view_is_current()` plus the retirement guards reject stale replies.
5. A post-baseline `WorkspaceActivation { output, workspace_index }` calls `show_workspace_hud()`. The manager resolves output to a live binding, replaces that monitor's HUD generation and timer, presents the new HUD, and closes it after `WORKSPACE_HUD_DURATION` only if the timer generation remains current.

### JavaScript-to-native Bridge v6 request

1. Trusted page JavaScript posts `{ id, method, params }` to WebKit's `shell` reply handler.
2. `bridge::attach_bridge()` requires the exact trusted state and URI, serializes JavaScriptCore to JSON, and calls `messages::handle_native_request()` with the receiving context and four narrow closures: state snapshot, workspace focus, Quick open/close, and Diagnostics open/close.
3. `handle_native_request()` requires an object-root token before complete JSON decoding and checks `methods_for(surface.kind())` before dispatch. `getHostInfo` returns backend, Bridge version, and exact surface context; `getCapabilities` returns only that context's methods; `getState` snapshots providers. Workspace and surface-action params require `Value::is_object()` before borrowed typed deserialization, so serde's array representation cannot reach actions; workspace indices remain positive and open params remain strict `{ open: bool }`.
4. The action closure validates workspace membership in the latest atomic Niri publication before running exactly `niri msg action focus-workspace <value>`. Surface closures upgrade the weak manager and return `surface_unavailable` if ownership or the caller's monitor binding is gone.
5. `NativeDispatch::Desktop` carries an authorized typed request to the registry. Native context supplies tray coordinates/application launch context. Cached or asynchronous providers return a structured envelope, and the transport rechecks the still-live trusted WebView before replying.

### Teardown

1. SIGINT/SIGTERM sources request `Application::quit()` on the GLib main loop. After the loop returns, `main()` removes those sources and drops the host. `ShellHost::drop()` disconnects the monitor observer before calling `SurfaceManager::shutdown()`; its application hold is released after cleanup.
2. Manager shutdown is idempotent: it marks lifecycle stopped, takes the typed activation-handler ID, Bars, transients, and HUD timers, disconnects activation observation, removes timers, closes windows, and calls `ProviderRegistry::shutdown()`.
3. Provider shutdown cancels reconnect and asynchronous I/O, force-exits the Niri child, invalidates stale callbacks, and publishes unavailable state.

### Diagnostic-only invocation

Diagnostic modes print Bridge v6 capabilities, effective config, and/or environment checks before normal application construction.

### Current native tests

- `surface.rs` asserts exact JSON shapes and typed accessors for all five typed contexts.
- `messages.rs` covers v5 host/context, scoped capabilities, forbidden cells, strict object/numeric/identity parameters, zero privileged dispatch on invalid input, Diagnostics self-open denial and Unicode action-key bounds.
- `surface_manager.rs` exercises duplicate/stale insert handling, idempotent close/stop, HUD generations and stale timers, topology reset, and indexed monitor collection.
- Provider tests cover Niri publication/lifecycle and sysfs behavior, plus audio parsing/confirmation/cancellation, tray owner/menu invalidation/leaf authorization, notification replacement/expiry/sender actions, installed-catalog filtering/ranking/refresh/launch invalidation and icon pixel bounds. Real production-controller IPC/subprocess/launch smoke is recorded in the implementation plan.
- `web_security.rs`, configuration, assets, and diagnostics retain their boundary-focused unit coverage; live Bar/transient compositor behavior is exercised by the manual Niri smoke profiles rather than claimed as a standing result.

## Integration Points

- **GTK4/GDK/GIO:** application lifecycle, monitor enumeration and observation, typed `ApplicationWindow` ownership, empty HUD input regions, GLib HUD timers, and provider subprocess I/O.
- **gtk4-layer-shell:** support check plus per-kind monitor assignment, namespaces, layers, anchors, margins, exclusive zones, and keyboard modes.
- **WebKitGTK / JavaScriptCore:** isolated custom-scheme rendering, per-view navigation/resource enforcement, and the trust-gated `shell` request/reply transport.
- **Filesystem and environment:** strict TOML config, canonical contained web assets, custom-scheme streaming, and Linux system-state reads.
- **Serde/TOML/JSON:** strict configuration decoding, tagged surface contexts, strict Bridge parameters, and stable request/response serialization.
- **Provider subsystem:** `ShellHost` constructs the registry, `SurfaceManager` owns its activation subscription and shutdown, and `bridge.rs` exposes only scoped snapshot/action callbacks. See [`src/providers/codemap.md`](providers/codemap.md).
