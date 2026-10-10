# HTML Desktop Shell

HTML Desktop Shell is a typed multi-surface desktop shell for Wayland compositors that implement `zwlr_layer_shell_v1`. One Rust 2024 process owns Bar, Quick Panel, Workspace HUD, Diagnostics Dialog, and Tray Menu content surfaces, each with an independently secured local WebKitGTK view. Tray dismissal uses a native parent-relative popover grab on the Bar and native-only layer-shell capture layers for the Quick Panel, without exposing web content or a Bridge. The official frontend uses Vue 3, but the native host does not require Vue.

## Current status

The current implementation has five typed content surfaces and Bridge v6, including native audio, tray, notification, and installed-application capabilities. Release acceptance remains in progress. Use current source files and repository maps, and rebuild generated assets when web sources change.

The repository is active, non-archived, and not presented as a fork. The roadmap records dated delivery verification, remaining defects, and release gates for subsequent changes.

## Scope and support boundary

- Wayland layer-shell only; no X11, normal-window, DRM/KMS, or compositor-less TTY fallback.
- Local static assets only; no HTTP server or remote-content mode.
- niri is the primary compositor target. Other Wayland compositors can host the shell if they expose layer-shell, but niri-specific state and actions become unavailable without niri IPC.
- A raw TTY is not supported unless it starts a compositor such as niri.
- The Bridge is deny-by-default. It does not expose generic process, filesystem, DBus, network, clipboard, screenshot, session-control, window-control, or JavaScript-evaluation facilities.

## Requirements

- Rust/Cargo with Rust 2024 edition support
- Bun
- `pkg-config`/`pkgconf`
- GTK4, GTK4 layer-shell, and WebKitGTK 6.0 development files
- a Wayland compositor exposing `zwlr_layer_shell_v1`
- optionally, niri with a live `NIRI_SOCKET` for niri provider/action support
- optionally, WirePlumber's `wpctl` for default-output audio and a session DBus for tray/notification integration; missing or conflicting services produce explicit unavailable state

On Arch Linux the native packages are `gtk4`, `gtk4-layer-shell`, and `webkitgtk-6.0`; inspect the proposed package transaction before installation.

## Build, test, and run

From the repository root:

```bash
(cd web && bun install --frozen-lockfile)
(cd web && bun run typecheck)
(cd web && bun test)
(cd web && bun run build)
cargo test --locked
cargo build --locked
./target/debug/html-desktop-shell
```

`bun install` populates frontend dependencies. The frontend build writes generated static assets to `web-dist/`; that directory is not committed. Cargo creates `target/` output. Run the binary inside a layer-shell-capable Wayland session. An explicit configuration can be selected with:

```bash
./target/debug/html-desktop-shell --config ./test/panel-default.toml
```

Useful process diagnostics are:

```bash
./target/debug/html-desktop-shell --print-capabilities
./target/debug/html-desktop-shell --config ./test/panel-default.toml --print-config
./target/debug/html-desktop-shell --check
GSK_RENDERER=cairo ./target/debug/html-desktop-shell
```

Diagnostic flags print information and exit without presenting surfaces. `--check` initializes GTK, checks layer-shell support, resolves the web entrypoint, and reports monitor count. The Cairo renderer is a troubleshooting option rather than the default recommendation.

## Architecture and data flow

1. `main` loads configuration and handles diagnostic-only modes.
2. GTK activation creates `ShellHost`, verifies layer-shell support, obtains the GDK monitor model, and creates a shared provider registry and trusted WebKit context.
3. `SurfaceManager` creates one persistent Bar per usable monitor and owns all transient surfaces, workspace-HUD timers, monitor rebuilds, and compositor activation observation.
4. `surface.rs` defines typed surface identity and policy. `surface_window.rs` applies that policy and creates an independent WebView trust latch with immutable caller context.
5. JavaScript sends a request to the `shell` WebKit handler. Rust verifies the committed local entry document, authorizes the exact method for the caller surface, validates parameters, and returns a structured JSON envelope.
6. Native providers share cached state. Each WebView owns its frontend state and polling lifetime.

| Surface | Lifetime | Placement | Exclusive zone | Input policy |
| --- | --- | --- | ---: | --- |
| Bar | persistent per active monitor | configured layer, top/left/right anchors | configured panel height | configured keyboard mode, pointer interactive |
| Quick Panel | on demand, at most one per monitor | overlay, top/right | 0 | on-demand keyboard, pointer interactive |
| Workspace HUD | timed replacement per monitor | overlay, centered | 0 | no keyboard, empty native input region |
| Diagnostics Dialog | on demand, at most one per monitor | overlay, centered | 0 | on-demand keyboard, pointer interactive |
| Tray Menu | on demand, at most one globally | Bar: parent-relative, native-trigger anchored; Quick Panel: overlay, top/right | no reservation | menu keyboard focus; native autohide/grab (Bar) or all-output outside-click dismissal (Quick Panel) |

Quick Panel and Diagnostics share an idempotent close path for Escape, focus loss, explicit close, monitor rebuild, and shutdown. Opening Diagnostics from Quick Panel closes that Quick Panel. HUD publication waits for compositor-confirmed workspace activation, suppresses initial/recovery/duplicate events, and uses generation-guarded timers so an old timeout cannot close a replacement.

Tray Menu is independent of Quick Panel. It renders exported DBusMenu instead of calling an application's `ContextMenu`, which may be absent or create an ordinary window. Both presentations share the same narrow Tray Menu authority and an independent trusted WebView, but they present differently. A Bar menu is a native parent-relative `GtkPopover`/`xdg_popup` anchored to the triggered Bar control with native autohide and grab; it is a distinct owned native presentation, not an ordinary window and not a layer namespace, so it never appears in compositor layer inventories. A Quick Panel menu remains its existing Overlay/Exclusive zone-`0` top/right workarea menu, with native-only transparent `-1` dismiss layers that cover every active output, including reserved panel areas, without reserving space. In both cases the first outside press closes the menu and is consumed rather than reaching the desktop. Escape, focus loss, explicit close, menu/owner disappearance, rebuild, and shutdown remove the menu and every capture layer. The Bar popup exists because a compositor-reserved top region (for example DMS) can displace the Bar from the output origin: layer-shell configure supplies local width and height, not the global origin, so an all-output `-1` menu cannot derive the Bar's real position without guessing another client's exclusive offsets. Native parent-relative placement resolves that boundary without guessing foreign geometry. Existing Bar/Quick/HUD/Diagnostics policies and desktop service ownership are unchanged.

A monitor change builds a complete replacement Bar set before closing the old surfaces. On success, the manager closes old persistent and transient surfaces, removes timers, replaces the mapping, and presents the new Bars. After successful startup, `ShellHost` holds the GIO application alive across runtime monitor changes, including temporary windowless states. `Ctrl+C` (`SIGINT`) and `SIGTERM` request normal shutdown, which blocks new work, disconnects observers, removes timers, closes surfaces, and stops shared providers once.

## Native Bridge v6

Bridge v6 has 21 exact methods. Bar and Quick Panel retain 20 methods; the new Tray Menu adds a close-only operation and has five narrowly scoped methods:

| Method | Bar | Quick Panel | Workspace HUD | Diagnostics | Tray Menu |
| --- | :---: | :---: | :---: | :---: | :---: |
| `getHostInfo` | yes | yes | yes | yes | yes |
| `getCapabilities` | yes | yes | yes | yes | yes |
| `getState` | yes | yes | no | yes | no |
| `niriFocusWorkspace` | yes | yes | no | no | no |
| `setQuickPanelOpen` | yes | yes | no | no | no |
| `setDiagnosticsDialogOpen` | yes | yes | no | yes | no |
| Audio volume/mute | yes | yes | no | no | no |
| Tray item actions / host menu open | yes | yes | no | no | no |
| `getTrayMenu` / `activateTrayMenuItem` | yes | yes | no | no | yes |
| `closeTrayMenu` | no | no | no | no | yes |
| Notification history/read/dismiss/action | yes | yes | no | no | no |
| Installed application search/launch | yes | yes | no | no | no |

Browser parameters cannot select a monitor or output; Rust derives those from immutable trusted caller context. Request envelopes must be JSON objects. Workspace and surface-action params must also be objects, rejecting arrays, missing or mistyped fields, and unknown fields before any action runs. Unauthorized or unknown methods, malformed JSON, stale surface monitors, unavailable niri, unknown workspaces, and failed actions produce structured errors. Diagnostics can use its open/close method only to close itself.

Desktop methods accept exact object parameters, never browser-selected commands,
arguments, paths, URIs, DBus identities, or global coordinates. Audio writes are
limited to integer percentages 0–100; actual readings may exceed 100. Tray IDs
are native-owned and bound to a current unique bus owner. Menu reads support
root/submenu preparation; clicks require the actual cached revision and an
enabled, visible leaf. Notifications and application IDs are likewise checked
against native session/catalog state before dispatch. Tray Menu requests must
match its immutable native-bound `trayItemId`; it cannot read general state or
control another item. `showTrayItemContextMenu({itemId})` opens the host menu and
returns `null`; `closeTrayMenu({})` is close-only and returns `null`. Version 6
is a clean protocol/SDK/asset cutover, with no v5 compatibility shim.

SNI display names and ToolTip titles remain literal text, preserving underscores,
angle brackets, and entity-like strings within the text bound. Only ToolTip
descriptions undergo markup-to-text conversion. DBusMenu labels are also literal,
but additionally normalize mnemonic underscores and control characters. The
reference UI renders all tray text as text, never HTML.

The complete parameter/result/security contract is recorded in the desktop
capability extension of
[`docs/html-desktop-shell-multi-surface-implementation-plan.md`](docs/html-desktop-shell-multi-surface-implementation-plan.md).

## Providers and frontend

The shared native providers publish clock, host, niri, battery, network, audio, tray, notification-count, and installed-application-count snapshots. The niri collector consumes `niri msg -j event-stream` through non-blocking GIO callbacks, publishes only synchronized workspace/window state, replaces its cache atomically, clears stale data on loss, and reconnects with bounded delay. Desktop integration reads are cache-only; IPC, subprocess observation, catalog enumeration, and application launch are asynchronous.

`niri.focusedWindows.items` contains one `{ output, window }` item for each active workspace with an output. The window comes from that workspace's `active_window_id`, has optional `title`/`appId` fields, and is `null` when no active window is known. Bar and Quick Panel select their own output through `HostInfo.surface.output`; their Niri status label uses that same fixed context. `niri.focusedOutput` remains the compositor's global keyboard-focus output, not a surface's identity.

`web/` contains the official Vue 3/Vite/TypeScript frontend. `web/packages/shell-api/` is the framework-agnostic typed bridge wrapper and must remain independent of Vue-specific packages. Frontend layout and presentation are web-owned; Rust owns native surface policy and security.

Audio uses WirePlumber's `wpctl` for the default output only. Tray supports
StatusNotifierItem/DBusMenu, joins an existing watcher, and supplies a watcher only
when absent. The notification server implements `org.freedesktop.Notifications`
without replacing another daemon: existing ownership is explicitly unavailable.
History is bounded and session-only. Restart after a deliberate notification
service migration; this host never stops another daemon.

Search/launch uses visible, launchable installed GIO desktop entries. It does not
execute browser-supplied command lines. Icons are bounded native RGBA pixels for
canvas rendering, not filesystem or remote image URLs. Diagnostics receives only
the new providers' availability/count summaries, not tray applications,
notification bodies, or audio control state.

The reference Bar adds default-output audio, tray icons/actions, and unread
notification counts. Quick Panel provides lazy Audio, System tray, Notifications,
and Applications tabs. Native menu trees remain inside Quick Panel; the launcher
searches installed applications and supports Arrow keys/Enter. Native failures
and provider-unavailable reasons are visible without granting generic authority.

The launcher uses a text input with `role="searchbox"` because WebKit's native
search decoration requests a `data:` SVG. This avoids revoking the local-only
trust latch; the resource policy is not relaxed.

## Configuration and assets

Native TOML controls Bar geometry and input policy. Transient geometry, lifetime, and input behavior are fixed by typed native policy. Configuration is selected from an explicit `--config` path, standard XDG/user configuration locations, then the packaged default.

The web resolver checks an explicit web-directory override, checkout/build locations, standard user data locations, then the system installation location. The selected directory must contain a readable regular `index.html`; all assets must be local. Missing assets, invalid configuration, no monitors, or absent layer-shell support are startup errors. Provider failures are reported in provider state rather than aborting the host.

## Installation and side effects

The documented local install copies the release binary under the user's local binary directory, copies `web-dist/` under the user's local data directory, and installs the default TOML under the user's config directory. It does not enable a service. Packaging examples include an Arch package, a user service, and a niri spawn fragment; none is enabled automatically.

Runtime creates GTK layer-shell windows, reads local configuration/assets and system provider state, and may launch the niri event stream. `niriFocusWorkspace` requests a compositor workspace change. The current implementation performs that action through one synchronous subprocess on the WebKit request path.

The desktop providers may launch fixed `wpctl` subprocesses, register a standard
tray host/watcher and an unoccupied notification service on the session bus, and
monitor the installed application catalog. Only an explicit allowlisted launch
request starts an installed application. These operations do not install
packages, enable services, replace daemons, or persist notification history.

## Manual QA and limitations

Use `scripts/smoke-current-niri.sh` only in a prepared current niri session; it creates and closes shell surfaces and exercises compositor-visible state. The `test/*.kdl` files are manual niri session entry points, not VM provisioning. Confirm surface count, namespace, output placement, layers, keyboard modes, disappearance, focus behavior, exclusive zones, rendered content, and HUD pointer transparency. The `--surface tray-menu` profile inventories only the Quick Panel menu's `html-desktop-shell-tray-menu-{monitorIndex}` layer namespace; a Bar tray menu is a native parent-relative popover with no layer namespace, so tuple inspection can neither confirm nor deny it, and a missing namespace is not a Bar popup presence pass.

Known limits include synchronous workspace action dispatch and smoke coverage that cannot prove rendered/focus/pointer behavior. A bare namespace tuple cannot establish an actual popup, grab, focus, or geometry. Separate native Bar-popup QA on 2026-10-09 observed parent-relative placement, keyboard actions, consumed outside presses on both outputs, external-focus autohide, stale-view/reply isolation, and constrained grow/collapse; Fcitx content was inspected without executing actions. Post-fix physical unplug/reconnect was deferred by the user and remains a release gate, not a pass inferred from automated checks or older hotplug. See [`docs/html-desktop-shell-feature-roadmap.md`](docs/html-desktop-shell-feature-roadmap.md) for dated verification and current remediation priorities, and [`codemap.md`](codemap.md) for source navigation.

## License

The repository is dual-licensed, at the recipient's option, under the Apache License 2.0 in `LICENSE-APACHE` or the MIT License in `LICENSE-MIT`. This dual-license boundary applies to the repository; third-party dependencies retain their own licenses.
