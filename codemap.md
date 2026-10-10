# Repository Atlas: `html-desktop-shell`

## Project Responsibility

`html-desktop-shell` is a Rust 2024 Linux host for five trusted local WebKit content surfaces: Bar, Quick Panel, Workspace HUD, Diagnostics Dialog, and an independent Tray Menu. GTK4 and `gtk4-layer-shell` own native placement/input; a Bar tray menu is a native parent-relative popup and the Quick menu's auxiliary dismiss layers contain no WebView or Bridge. The executable owns configuration, topology, native providers, lifecycle, and versioned authority. The official frontend is Vue 3/Vite; the SDK is framework-independent.

This repository is deliberately a local desktop-surface host, not a compositor or general desktop runtime. It has no X11, ordinary-window, raw-TTY, Electron, remote-content, or compositor-less fallback.

## Runtime and Data Flow

1. `src/main.rs` loads the three-field shell configuration (`panel_height`, `layer`, and `keyboard_mode`), handles diagnostic-only flags, creates `gtk4::Application`, and retains a `ShellHost` for the application lifetime.
2. `ShellHost` verifies layer-shell support, obtains the GDK monitor model, creates the shared `ProviderRegistry` and `TrustedWebContext`, and transfers all surface ownership to one `Rc<RefCell<SurfaceManager>>`. The host retains that manager, the monitor model, the topology signal subscription, and a GIO application hold so runtime monitor/window gaps do not end the process.
3. `SurfaceManager` owns Bars, typed transients, HUD generations, and one global Tray Menu whose Bar presentation is a native parent-relative popup and whose Quick presentation keeps an Overlay/Exclusive zone-`0` menu with all-output native dismiss capture. Monitor-list or connector metadata changes transactionally replace affected bindings/surfaces; unchanged bindings preserve the current set.
4. `surface_window::surface_window_for_monitor()` constructs every typed layer-backed `ApplicationWindow`, applies its layer-shell anchors, size, exclusive zone, keyboard and input policies, assigns a kind-and-monitor namespace, creates a separately trust-latched WebView, and loads `html-desktop-shell://assets/index.html` from the read-only custom scheme. The Bar tray menu is instead a native parent-relative popover from `tray_popover_for_parent()` with no layer namespace.
5. `bridge::attach_bridge()` gates the committed local document and dispatches Bridge v6 with immutable caller context and weak manager ownership. The 21-method contract retains 20 Bar/Quick methods and gives Tray Menu five bound-menu-only operations.
6. `getState` snapshots the shared provider registry. Clock/host/sysfs retain their existing sources; Niri and desktop providers return cached publications. Audio, tray/menu IPC, notifications, installed-catalog enumeration, and application launch have independent native controllers. Diagnostics constructs only desktop availability/count summaries.
7. The Niri collector publishes coherent state only after its initial workspace/window barrier. Later focused-workspace changes notify typed activation observers, while the initial baseline, unavailable state, duplicates, stale generations, and shutdown are suppressed. `SurfaceManager` maps a confirmed activation's output to the current monitor and replaces its input-transparent HUD for 1.5 seconds.
8. `SurfaceRoot` selects one of five content views from validated native context. Bar/Quick/Diagnostics own state polling; HUD uses confirmed workspace context; Tray Menu reads only its bound DBusMenu session and never calls `getState`.

## System Entry Points

- `src/main.rs` — native executable and GTK application entry point.
- `src/config.rs` and `src/diagnostics.rs` — CLI/TOML startup pipeline and diagnostic-only modes.
- `src/shell_host.rs`, `src/surface.rs`, `src/surface_manager.rs`, `src/surface_window.rs`, and `src/web_security.rs` — application-to-manager ownership, typed surface identity and lifecycle, per-kind layer-shell windows, and the shared trusted WebKit context.
- `src/bridge.rs` and `src/messages.rs` — trust-gated transport and Bridge v6 surface/ID-scoped dispatch.
- `src/providers/` — system/Niri state and workspace actions plus asynchronous default-output audio, standard SNI/DBusMenu, a non-stealing notification server, installed-application search/launch, and bounded icon pixels.
- `web/index.html` and `web/src/main.ts` — official frontend document and Vue bootstrap entries.
- `web/packages/shell-api/src/index.ts` — framework-neutral typed browser client for the native bridge.
- `examples/plain-html/index.html` — custom-frontend example entry that needs no build tool or framework.
- `packaging/PKGBUILD` — Arch build and package entry point; installs the executable, built web assets, user service, and reference files.
- `scripts/smoke-current-niri.sh` and `scripts/niri-layer-smoke.ts` — manual current-session launcher and validator with Bar and transient-surface smoke profiles.

## Architecture Boundaries and Invariants

- **Wayland layer-shell only:** startup must fail cleanly when layer-shell, a display, usable monitors, or safe local web assets are unavailable, and when a selected configuration is invalid. There is no fallback backend.
- **Local static assets only:** `ResolvedWebAssets` canonicalizes a contained asset root and entry file; WebKit receives them only through the read-only `html-desktop-shell://assets/` scheme, never a file URI. The official bundle and packaged themes/icons must not depend on a CDN or runtime network fetch.
- **Native policy, frontend presentation:** Rust owns monitor bindings, typed surface creation and teardown, namespaces, exclusive zones, keyboard/input policy, bridge authority, and host state. Frontends render the native-selected surface and own view presentation and polling.
- **Deny-by-default Bridge v6:** Bar/Quick allow 20 methods, HUD two, Diagnostics four, and Tray Menu exactly host/capabilities, bound menu read/activation, and self-close. No arbitrary commands, paths, URIs, DBus identities, output selection, or global coordinates.
- **Shared single-process state:** every surface clones one `ProviderRegistry`. The Niri collector and desktop controllers own their cached state, asynchronous I/O, invalidation, cancellation, and shutdown. Niri observers run after provider borrows are released and suppress initial/reconnect baselines, duplicates, unavailable state, stale generations, and shutdown.
- **Typed independent lifecycle:** persistent Bars and typed transients remain separate. Tray Menu is not Quick Panel: its Bar presentation is a native parent-relative `GtkPopover`/`xdg_popup` with native autohide/grab, and its Quick presentation captures outside clicks on every output; both consume the first outside press to close. Every menu close/invalidation/rebuild/shutdown retires the live popup/menu and removes all Quick native capture layers. HUD generations and empty input regions retain their existing policy.
- **Topology replacement and shutdown:** connector metadata and monitor-list changes coalesce into transactional rebuilds; unchanged bindings avoid needless replacement. Host drop disconnects monitor/connector observers, cancels pending work, and invokes idempotent manager/provider shutdown.
- **Strict configuration:** unknown TOML fields are rejected, and the runtime configuration surface is limited to panel height, layer, and keyboard mode.
- **Security boundary:** the shared `TrustedWebContext` applies a strict local-only CSP, rejects non-contained or non-regular scheme paths, and streams contained assets. Per-WebView policy, popup, response, load, URI, failure, and resource hooks allow only the one-shot trusted entry navigation and same-origin resources; any redirect, popup, mismatched URI, or other policy violation permanently revokes bridge access for that view. Bridge closures hold only `Weak<RefCell<SurfaceManager>>`, so a page cannot prolong native surface ownership.
- **Testing boundary:** Rust tests cover typed surface/protocol/security/lifecycle behavior and native desktop parsing, ownership, invalidation, actions, and cancellation. Bun tests cover the SDK and mounted Vue callers. Real-controller and hosted-WebKit verification is dated in the implementation plan; full compositor geometry/focus/hotplug/input QA remains distinct from automated contract tests.

## Root Files and Supporting Directories

- `Cargo.toml` / `Cargo.lock` define and lock the Rust 2024 GTK4, layer-shell, WebKitGTK, JavaScriptCore, GLib/GLib Unix, libc signal constants, Serde, JSON, and TOML application.
- `README.md` is the user-facing build, configuration, diagnostics, packaging, and manual-operation guide.
- `AGENTS.md` records repository engineering constraints and points contributors to this atlas.
- `docs/` contains design and roadmap material; it is not part of runtime loading.
- `test/` contains manual Niri session/configuration fixtures and a measurement helper, not the application implementation.
- `LICENSE-MIT` and `LICENSE-APACHE` provide the repository's dual-license texts.
- Generated `web-dist/` and Cargo `target/` outputs are build artifacts rather than source directories.

## Directory Map

| Directory | Responsibility | Detailed map |
| --- | --- | --- |
| `src/` | Rust lifecycle, five typed content surfaces, trusted Bridge v6, native input capture, and diagnostics. | [`src/codemap.md`](src/codemap.md) |
| `src/providers/` | Shared system/Niri/desktop controllers, cached publications, exact native actions, and cancellation. | [`src/providers/codemap.md`](src/providers/codemap.md) |
| `web/` | Bun workspace and Vite root for the official Vue 3 panel, producing local assets in `web-dist/`. | [`web/codemap.md`](web/codemap.md) |
| `web/src/` | Vue bootstrap, localization, root host initialization, typed surface dispatch, state presentation, per-view polling, and styles. | [`web/src/codemap.md`](web/src/codemap.md) |
| `web/src/composables/` | View-scoped polling and local desktop-action lifecycle. | [`web/src/composables/codemap.md`](web/src/composables/codemap.md) |
| `web/src/plugins/` | Application-wide PrimeVue installation and local theme policy. | [`web/src/plugins/codemap.md`](web/src/plugins/codemap.md) |
| `web/src/stores/` | Pinia host boundary for root initialization, surface context, provider state, bridge/action errors, refresh serialization, and typed surface operations. | [`web/src/stores/codemap.md`](web/src/stores/codemap.md) |
| `web/src/views/` | `SurfaceRoot` dispatch plus Bar, Quick Panel, Workspace HUD, and Diagnostics surface views. | [`web/src/views/codemap.md`](web/src/views/codemap.md) |
| `web/src/widgets/` | Status renderers, audio/tray/notification/launcher controls, and canvas icons. | [`web/src/widgets/codemap.md`](web/src/widgets/codemap.md) |
| `web/packages/` | Framework-independent reusable packages in the Bun workspace. | [`web/packages/codemap.md`](web/packages/codemap.md) |
| `web/packages/shell-api/` | Package manifest and public contract for `@html-desktop-shell/shell-api`. | [`web/packages/shell-api/codemap.md`](web/packages/shell-api/codemap.md) |
| `web/packages/shell-api/src/` | Typed Bridge v6 models, validators, transport, and 21 exact operations. | [`web/packages/shell-api/src/codemap.md`](web/packages/shell-api/src/codemap.md) |
| `examples/` | Developer-facing custom-frontend examples kept outside the packaged official UI. | [`examples/codemap.md`](examples/codemap.md) |
| `examples/plain-html/` | Dependency-free local HTML/CSS/JavaScript frontend demonstrating the static asset and bridge contracts. | [`examples/plain-html/codemap.md`](examples/plain-html/codemap.md) |
| `examples/plain-html/js/` | Plain example's bridge request wrapper and one-shot DOM status renderer. | [`examples/plain-html/js/codemap.md`](examples/plain-html/js/codemap.md) |
| `packaging/` | Arch package recipe, systemd user unit, Niri startup snippet, and inactive reference configuration. | [`packaging/codemap.md`](packaging/codemap.md) |
| `scripts/` | Assertive current-Niri smoke validator and launcher, including bounded polling, diagnostics, and process-group cleanup. | [`scripts/codemap.md`](scripts/codemap.md) |
