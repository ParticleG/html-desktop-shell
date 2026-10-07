# Native Rust Host (`src/`)

## Responsibility

This directory implements the native `html-desktop-shell` process: configuration/assets, GTK activation, four typed WebKit layer-shell surfaces, surface-scoped Bridge v5 and shared native providers. Desktop capabilities remain independent of frontend framework. Diagnostic invocations exit before the application loop.

## Entry Point and Module Map

- `main.rs` — process entry point. `main()` owns startup error handling, diagnostic short-circuiting, `gtk4::Application` creation, activation, SIGINT/SIGTERM main-loop sources, and the lifetime of `ShellHost`; it declares `surface`, `surface_manager`, and `surface_window` as the native surface modules.
- `config.rs` — command-line and TOML configuration pipeline. `load()` returns `LoadedConfig { config, diagnostics, app_args }`; `ShellConfig` carries panel height, layer, and keyboard mode used by Bar policy.
- `diagnostics.rs` — `DiagnosticFlags` modes for capability JSON, effective config TOML, and environment checks covering GTK initialization, layer-shell support, web assets, and monitor discovery.
- `assets.rs` — six-tier asset resolver. `web_assets()` selects the first existing `index.html`, rejects an unsafe selected entry without fallthrough, and returns canonical root-contained `ResolvedWebAssets`.
- `shell_host.rs` — application/manager boundary. `ShellHost` owns `Rc<RefCell<SurfaceManager>>`, the live GDK monitor model, its `items_changed` handler, and a GIO application hold that prevents runtime monitor/window gaps from ending the process; it delegates presentation, topology rebuild, and shutdown to the manager.
- `surface.rs` — typed identity and wire context. `SurfaceKind::{Bar, QuickPanel, WorkspaceHud, DiagnosticsDialog}`, `SurfaceKey`, the tagged `SurfaceContext` variants, and `SurfaceError` prevent kind/monitor identity from collapsing into an untyped window list.
- `surface_manager.rs` — all native surface ownership and orchestration. `SurfaceManager` owns construction dependencies, monitor bindings, persistent Bar instances, the typed transient `LifecycleRegistry`, HUD generations/timers, and the provider activation-handler ID.
- `surface_window.rs` — shared typed window factory. `SurfaceInstance` pairs context and window; `surface_window_for_monitor()` applies per-kind layer-shell policy and namespace, creates a secure WebView, attaches weak lifecycle/Bridge callbacks, and loads the trusted entry URI.
- `web_security.rs` — local-content security boundary. `TrustedWebContext` serves the read-only custom asset scheme and creates per-view trust latches with navigation, popup, response, load, URI, failure, and resource enforcement hooks.
- `bridge.rs` — trusted WebKit request/reply adapter; captures immutable surface identity and weak manager ownership, dispatches strict typed operations, derives native point/launch context and rechecks live view/trust before asynchronous replies.
- `messages.rs` — Bridge v5 types, 20-method surface-scoped capability matrix, strict object parameters, `NativeDispatch` and stable structured replies.
- [`providers/`](providers/codemap.md) — shared Niri/system/desktop state, exact workspace/audio/tray/notification/application operations, native icon pixels and cancellable lifecycle.

## Design and Patterns

- **Host-to-manager ownership:** `main()` retains `ShellHost`; the host retains one manager, the monitor signal, and an application hold after successful startup. Only `SurfaceManager` owns windows, provider shutdown, transient lifecycle, and compositor activation observation. WebView and lifecycle closures downgrade manager ownership to `Weak`.
- **Typed registry:** `SurfaceKind` has four exact wire names and namespace prefixes; `SurfaceContext` carries `monitorIndex`, optional connector output, and for HUD the compositor-confirmed `workspaceIndex`; `SurfaceKey { kind, monitor_index }` keys independent instances. Persistent Bars remain separate from `LifecycleRegistry` transients.
- **Atomic topology rebuild:** `rebuild_for_monitors()` collects indexed valid monitors, constructs every replacement Bar first, then swaps monitor bindings and Bars, resets transient/HUD lifecycle, removes timers, closes all old instances, and presents the replacements. A failed construction closes only the attempted replacement set and leaves current ownership intact.
- **Transient lifecycle:** Quick Panel and Diagnostics open idempotently on the caller's current binding and close idempotently. Both close on Escape or after losing an acquired active state. Opening Diagnostics from Quick also closes that Quick Panel. Close callbacks remove only the registry instance whose GTK window identity still matches, preventing stale callbacks from deleting replacements.
- **Compositor-confirmed HUD:** the provider observer emits only later focused-workspace changes. `show_workspace_hud()` resolves the activation output against current bindings, advances a per-monitor generation, destroys the preceding HUD and timer, creates a context containing the confirmed workspace index, and schedules a 1.5-second close. Generation checks make old callbacks harmless.
- **Per-kind layer-shell policy:** every undecorated transparent-content window is monitor-bound and named `<namespace-prefix>-<monitor-index>`, using `html-desktop-shell-panel`, `html-desktop-shell-quick-panel`, `html-desktop-shell-workspace-hud`, or `html-desktop-shell-diagnostics`. Bar spans the top edge with configured layer, height, exclusive zone, and keyboard mode. Quick Panel is a fixed 384×480 top-right overlay below the Bar with on-demand keyboard. HUD is a centered 280×96 overlay with no keyboard or exclusive zone, cannot be targeted, and receives an empty GDK input region on realize. Diagnostics is a fixed centered 560×480 overlay with on-demand keyboard and no exclusive zone.
- **Bridge v5 scoped authority:** Bar/Quick Panel allow all 20 exact methods; HUD keeps host/capabilities; Diagnostics keeps host/capabilities/state/self-close and gets new provider availability/count summaries. Unauthorized cells return `surface_forbidden`; strict params deny unknown fields.
- **No generic native authority:** operations never expose arbitrary commands, arguments, paths, URIs, DBus identities, monitor coordinates or session control. Desktop caches authorize IDs/revisions; surface policy stays native-owned.
- **Shared provider registry:** all surfaces clone one `ProviderRegistry`; reads clone the same atomic Niri cache without waiting for compositor IPC. Provider details, activation suppression, and shutdown are in the [provider map](providers/codemap.md).
- **Canonical custom-scheme assets and one-shot trust:** `ResolvedWebAssets` restricts requests to regular contained files under the exact scheme authority. Each view trusts only the committed initial entry navigation; redirects, popups, URI/response mismatch, untrusted resources, failures, or unexpected policy decisions permanently revoke Bridge access.
- **Fail-fast startup, degradable runtime:** invalid configuration, unsupported layer shell, missing display/assets/usable monitors, and initial Bar failure reach the process boundary. Later bridge/load/topology/transient failures are logged or returned as structured errors without granting fallback authority.

## Data and Control Flow

### Normal application startup and topology

1. `main()` loads configuration, runs requested diagnostics, constructs `gtk4::Application`, registers activation, and enters `run_with_args()`.
2. `ShellHost::new()` validates layer-shell/display availability, gets `display.monitors()`, creates one `ProviderRegistry` and `TrustedWebContext`, then calls `SurfaceManager::new()`.
3. The manager's `build_initial_bars()` delegates to `rebuild_for_monitors()`, which creates a `SurfaceContext::Bar` and `SurfaceInstance` for each valid indexed monitor.
4. The manager connects `ProviderRegistry::connect_workspace_activation()` with a weak callback to `show_workspace_hud()`. `ShellHost` separately connects monitor `items_changed` with a weak callback to `rebuild_for_monitors()`.
5. `ShellHost::present()` delegates to `SurfaceManager::present_bars()`. Subsequent topology events build replacements, swap current bindings/Bars, cancel and clear all transient timers and generations, close old transients and Bars, then present the new Bars.

### Typed surface construction and operations

1. `surface_window_for_monitor()` creates a decorated-free layer-shell window, binds its monitor, derives the namespace from `SurfaceKind::namespace_prefix()` and `monitor_index`, and applies the kind's geometry, layer, exclusive-zone, keyboard, focus-close, and input policy.
2. It obtains a separately latched secure WebView, installs lifecycle hooks carrying `SurfaceKey` plus a weak manager, attaches Bridge callbacks with the complete `SurfaceContext`, loads `TRUSTED_ENTRY_URI`, and returns `SurfaceInstance`.
3. `setQuickPanelOpen` and `setDiagnosticsDialogOpen` first call `current_binding()` to reject stale callers. Open paths build one typed transient from retained manager dependencies and insert it only if the context remains current; close paths remove only the requested key.
4. A post-baseline `WorkspaceActivation { output, workspace_index }` calls `show_workspace_hud()`. The manager resolves output to a live binding, replaces that monitor's HUD generation and timer, presents the new HUD, and closes it after `WORKSPACE_HUD_DURATION` only if the timer generation remains current.

### JavaScript-to-native Bridge v5 request

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

The parser removes diagnostic flags from GTK arguments. `diagnostics::run_if_requested()` prints the Bridge v5 all-surface capability matrix, strict effective config and/or environment checks, then exits before application construction.

### Current native tests

- `surface.rs` asserts exact JSON shapes and typed accessors for all four contexts.
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
