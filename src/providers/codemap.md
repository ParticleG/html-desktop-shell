# `src/providers/`

## Responsibility

This directory is the native state, compositor-event and desktop-integration layer. It publishes shared cached state and exact typed operations without granting generic process, DBus, filesystem or session authority.

- `mod.rs` defines `Provider`, shared `ProviderRegistry`, clock/host providers, surface-scoped snapshots, typed desktop dispatch, workspace activation observers and idempotent shutdown.
- `niri.rs` defines `WorkspaceActivation`, `WorkspaceActivationHandlerId`, `WorkspaceActivationObservers`, `NiriPublication`, `NiriController`, `NiriProvider`, `NiriReducer`, and the synchronous focus action.
- `niri_collector.rs` owns `NiriCollector`, asynchronous GIO subprocess I/O/wait, generation retirement, reconnect backoff, `ActivationGate`, observer notification, and shutdown.
- `system.rs` reads Linux sysfs to produce battery and network snapshots.
- `desktop.rs` defines closed `DesktopRequest`, native `ScreenPoint` and `TrayOrientation`.
- `audio.rs` serializes fixed asynchronous `wpctl` observations/writes with fresh confirmation, deadlines, unavailable/recovery and child reaping.
- `tray.rs` owns standard SNI host/watcher integration, unique-owner-bound item caches, DBusMenu revisions/lazy submenus and exact asynchronous item/menu actions.
- `notifications.rs` owns a non-stealing standard notification server, bounded session history, expiry/read/dismiss/actions and sender-targeted callbacks.
- `applications.rs` enumerates installed GIO desktop entries, caches a visible/launchable allowlist, ranks searches and performs native asynchronous launch.
- `icons.rs` supplies bounded native RGBA8 pixels from safe theme/GIcon, ARGB/RGB and PNG inputs without exposing paths or URLs.

Production state has `clock`, `host`, `niri`, `battery`, `network`, `audio`, `tray`, `notifications` and `applications`. Diagnostics gets availability/count summaries for the new desktop providers, not application lists or audio controls.

## Design / Patterns

### Registry and shared ownership

- `Provider` supplies `name()`, cached `snapshot()` and an overridable `diagnostic_snapshot()`; audio/tray avoid constructing private/full payloads for Diagnostics.
- `ProviderRegistry::new(&monitors)` creates the existing providers and one shared controller for each desktop feature. `Rc` keeps state/resources shared across surfaces.
- `snapshot_for(kind)` fills a pre-sized map. Desktop and Niri reads do not launch IPC or wait for processes; clock/host/sysfs providers retain their existing behavior.
- `execute_desktop()` accepts only a typed closed operation and native point/launch context. Workspace observer tokens remain typed and disconnectable. Shutdown stops every controller once.

### Clock and host snapshots

- `ClockProvider::snapshot()` emits `{ "time": "HH:MM:SS" }`; `current_time()` uses local GLib time and falls back to `"--:--:--"` if acquisition or formatting fails.
- `HostProvider` retains the live GDK monitor `ListModel`. Each snapshot emits backend `"wayland-layer-shell"`, current `monitorCount`, and `messages::BRIDGE_VERSION` (5).

### Niri publication and reducer

- `NiriPublication` stores snapshot JSON and `workspace_indices` in one `RefCell` value. `publish()` replaces both together; `publish_unavailable()` installs the stable unavailable object and clears indices, so reads and workspace validation always observe one coherent generation.
- `NiriReducer` owns workspace and window maps keyed by stable Niri IDs. It handles complete replacements and incremental activation, focus, urgency, layout, open/change, and close events. Unknown event variants are ignored; malformed input and incremental events targeting unknown IDs retire the generation rather than publishing incoherent state.
- A fresh generation remains unavailable until both `WorkspacesChanged` and `WindowsChanged` cross the initial barrier. Publications deterministically sort workspaces by output, index, then ID and derive the global focused output, the complete snapshot, matching workspace indices, and an optional `focused_activation`.
- `focusedWindows.items` is output-sorted and derives `{ output, window }` from each active workspace's `active_window_id`, not global `NiriWindow.is_focused`. Missing active windows remain `null` items so an empty output does not inherit another output's title.

### Typed activation observers and suppression

- `WorkspaceActivation { output, workspace_index }` is the only compositor event exported from providers. `WorkspaceActivationObservers` stores `Rc<dyn Fn(WorkspaceActivation)>` handlers in a `BTreeMap` keyed by monotonically checked `WorkspaceActivationHandlerId`; connect and disconnect are surfaced through `ProviderRegistry`.
- Each collector generation resets `ActivationGate`. The first coherent publication establishes its focused activation as a baseline and emits nothing. Later notification occurs only when the new activation is `Some` and differs from the baseline.
- Consequently initial startup, reconnect baselines, duplicate focus publications, and transitions to no focused workspace/unavailable state do not create HUD events. `is_current()` and `retiring` checks also suppress stale-generation and post-shutdown callbacks.
- `publish_update()` commits the coherent publication before collecting cloned observer callbacks. It releases collector/publication/observer borrows before invocation, so callbacks may re-enter provider/manager code or disconnect themselves safely.

### Collector lifecycle, reconnect, and shutdown

- `NiriCollector` starts `niri msg -j event-stream` with stdout/stderr pipes. GIO asynchronously reads stdout lines, drains stderr bytes, and waits for child completion on the GLib main context.
- `CompletionGate` allows exactly one reconnect only after stdout and child wait have both completed, independent of completion order. Generation and retirement checks discard late callbacks.
- Launch, stream, parse/reducer, or child-exit failure publishes unavailable state, retires and cancels the current generation, and reconnects with exponential delay bounded from 250 ms to 5 seconds. Backoff resets only on the generation's first coherent publication.
- `NiriCollector::shutdown()` is idempotent: it marks stopped, advances generation, marks retiring, releases stream handles, publishes unavailable state, removes a reconnect timer, cancels GIO, and force-exits the child. `ProviderRegistry::shutdown()` is called by manager shutdown; `NiriControllerInner::drop()` is the final idempotent safeguard.

### Exact workspace action

- `NiriController::focus_workspace()` rejects zero, unavailable state, and indices absent from the latest atomic publication. Only after validation does it synchronously run exactly `niri msg action focus-workspace <workspaceIndex>`.
- Success returns `{ "workspaceIndex": value }`; launch or non-zero exit maps to `niri_action_failed`. It does not run a read command or synthesize activation—HUD creation waits for a later collector-confirmed activation.

### Battery and network snapshots

- `BatteryProvider` and `NetworkProvider` default to `/sys/class/power_supply` and `/sys/class/net`; private path-injecting constructors support isolated tests.
- `battery_snapshot()` selects `type=Battery`, parses and clamps capacity, normalizes status, emits sorted devices, averages percentage, and aggregates status with precedence charging, discharging, all-full, not-charging, then unknown.
- `network_snapshot()` excludes loopback, classifies wireless directories, Ethernet sysfs type `1`, or other devices, normalizes `operstate`, derives `isUp`, counts wired/wireless up/down state, and sorts by kind and name.
- System providers do not cache: each aggregate snapshot reads current sysfs state.

### Desktop integration

The original desktop-capability delivery and dated real-IPC evidence are in the
[v5 section](../../docs/html-desktop-shell-multi-surface-implementation-plan.md#desktop-capability-extension--bridge-v5).
Current host-menu ownership and scoped v6 authorization are in the
[tray-menu extension](../../docs/html-desktop-shell-multi-surface-implementation-plan.md#native-tray-menu-extension--bridge-v6).
Audio controls only the default output. Tray joins the current watcher or owns
an absent/released name without replacement; menu preparation and clicks use
native owner bindings and actual revision/visibility/enabled state. Notifications
request only a free standard server name and keep at most 100 session records.
Applications transport plain metadata across the GIO worker boundary and create
authoritative `gio_unix::DesktopAppInfo` on GTK in yielded batches; no GObject
is assumed Send. Catalog invalidation clears the launch allowlist until refresh.
Asynchronous callbacks are weak/generation guarded and all resources are
cancellable or explicitly reaped/released on shutdown.
SNI display names and ToolTip titles preserve literal text and underscores;
only ToolTip descriptions undergo markup-to-text conversion. DBusMenu labels
retain literal angle brackets and entity-like strings, with separate
mnemonic/control-character normalization. Frontends never interpret tray markup.
Host menu opening never forwards vendor `ContextMenu`. Tray cache authority
continues to bind native item IDs, current unique owners, menu paths and revisions;
the surface manager separately owns menu placement, lifecycle, and the Quick
menu's input captures (the Bar menu relies on its native popover grab).

## Data & Control Flow

1. `ShellHost::new()` passes the live GDK monitor model to `ProviderRegistry::new()`. The registry creates one `NiriController`, whose collector starts one event-stream subprocess.
2. `SurfaceManager::new()` connects one workspace-activation handler and retains its typed ID. Every WebView Bridge callback receives a clone of the same registry.
3. The collector reduces stdout lines. Before the workspace/window barrier it publishes nothing. On the first coherent update it atomically publishes state/indices, records the activation baseline, resets reconnect backoff, and suppresses notification.
4. Later coherent updates atomically publish first. If `ActivationGate` detects a changed focused activation, the collector clones callbacks after releasing borrows and emits `WorkspaceActivation { output, workspace_index }`.
5. The manager callback resolves `output` to a current monitor binding and builds a typed Workspace HUD carrying the confirmed index. Missing/stale outputs, stopped lifecycle, duplicates, unavailable state, and stale collector generations produce no HUD.
6. `getState` reaches `ProviderRegistry::snapshot()`, which returns fresh clock/host/system values plus the cached Niri value immediately:
   - `clock.time`
   - `host.backend`, `host.monitorCount`, `host.bridgeVersion`
   - `niri.available`, with global `focusedOutput`, workspaces, and output-scoped `focusedWindows` when coherent
   - `battery.available` plus aggregate/devices when present
   - `network.available` plus counts/interfaces when present
7. A scoped `niriFocusWorkspace` request validates `workspaceIndex` against the same publication's indices, executes the one-shot action, and returns before any HUD; only a later confirmed event may notify the manager.
8. Collector failure publishes unavailable state, retires the child, and schedules one bounded reconnect after stdout/wait completion. A replacement generation must establish a fresh coherent baseline before later changes can emit activation.
9. `SurfaceManager::shutdown()` disconnects its typed activation handler before `ProviderRegistry::shutdown()` invalidates collector callbacks, timers, I/O, and child state.

## Failure Degradation

- Niri launch, I/O, parse/reducer, or child-exit failure degrades only `niri` to `{ "available": false, "reason": "niri IPC unavailable" }`, clears indices, and begins bounded recovery.
- Atomic publication constrains actions to known workspace indices. Errors remain `invalid_params`, `niri_unavailable`, `unknown_workspace`, or `niri_action_failed`.
- Missing sysfs roots and malformed required data return provider-local unavailable objects rather than failing the aggregate snapshot. No battery or no non-loopback interfaces are stable normal-unavailable states; optional status fields degrade to `"unknown"`.
- Observer suppression is part of correctness: unavailable transitions, absent focused activation, duplicate publications, reconnect baselines, stale generations, and shutdown cannot spuriously drive a Workspace HUD.

## Current Tests

- `mod.rs` covers registry aggregation, clock shape, and typed observer disconnection.
- `niri.rs` covers initial barriers, deterministic exact schema, full replacements, derived focus mutations, ignored unknown variants, malformed/unknown-target failure, atomic workspace-action validation, and stable action error codes.
- `niri_collector.rs` covers bounded/resetting backoff, order-independent single reconnect, initial/duplicate/unavailable/reconnect activation suppression, callback borrow release and self-disconnect, stale-generation/shutdown suppression, idempotent unavailable shutdown, and a GIO lifecycle fixture that reconnects, recovers, and reaps its child.
- `system.rs` covers battery parsing/unavailable cases and wired/wireless counting/unavailable cases with isolated filesystem fixtures.

## Integration Points

- **GTK/GDK:** `HostProvider` reads the live `gio::ListModel` from `gdk::Display::monitors()`.
- **GLib/GIO:** clock formatting, event-stream subprocess pipes, asynchronous reads/wait, cancellation, reconnect timers, and main-context callbacks.
- **Linux sysfs:** `system.rs` reads `/sys/class/power_supply` and `/sys/class/net`.
- **Niri compositor:** one asynchronous `niri msg -j event-stream` process supplies reads; the exact synchronous focus-workspace subprocess remains the Niri write.
- **Surface manager:** typed observer callbacks carry only confirmed `WorkspaceActivation`; manager shutdown disconnects its handler before provider shutdown.
- **Native protocol:** `messages::BRIDGE_VERSION` appears in host snapshots; `NativeDispatch`/`DesktopRequest` separate strict authorization/validation from cached or asynchronous provider execution. No generic native identity or command crosses this boundary.
