# HTML Desktop Shell

HTML Desktop Shell is a typed multi-surface desktop shell for Wayland compositors that implement `zwlr_layer_shell_v1`. One Rust 2024 process owns Bar, Quick Panel, Workspace HUD, and Diagnostics Dialog GTK4 layer-shell windows and embeds an independently secured local WebKitGTK view in each. The official frontend uses Vue 3, but the native host does not require Vue.

## Current status

The current implementation owns four typed surfaces and exposes Bridge v4. Use the current source files and repository maps for architecture details, and rebuild the generated frontend when web sources change. No private endpoint or machine-specific location is required.

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

Quick Panel and Diagnostics share an idempotent close path for Escape, focus loss, explicit close, monitor rebuild, and shutdown. Opening Diagnostics from Quick Panel closes that Quick Panel. HUD publication waits for compositor-confirmed workspace activation, suppresses initial/recovery/duplicate events, and uses generation-guarded timers so an old timeout cannot close a replacement.

A monitor change builds a complete replacement Bar set before closing the old surfaces. On success, the manager closes old persistent and transient surfaces, removes timers, replaces the mapping, and presents the new Bars. After successful startup, `ShellHost` holds the GIO application alive across runtime monitor changes, including temporary windowless states. `Ctrl+C` (`SIGINT`) and `SIGTERM` request normal shutdown, which blocks new work, disconnects observers, removes timers, closes surfaces, and stops shared providers once.

## Native Bridge v4

Bridge v4 exposes exactly six methods: `getHostInfo`, `getCapabilities`, `getState`, `niriFocusWorkspace`, `setQuickPanelOpen`, and `setDiagnosticsDialogOpen`. Capabilities are surface-scoped:

| Method | Bar | Quick Panel | Workspace HUD | Diagnostics |
| --- | :---: | :---: | :---: | :---: |
| `getHostInfo` | yes | yes | yes | yes |
| `getCapabilities` | yes | yes | yes | yes |
| `getState` | yes | yes | no | yes |
| `niriFocusWorkspace` | yes | yes | no | no |
| `setQuickPanelOpen` | yes | yes | no | no |
| `setDiagnosticsDialogOpen` | yes | yes | no | yes |

Browser parameters cannot select a monitor or output; Rust derives those from immutable trusted caller context. Request envelopes must be JSON objects. Workspace and surface-action params must also be objects, rejecting arrays, missing or mistyped fields, and unknown fields before any action runs. Unauthorized or unknown methods, malformed JSON, stale surface monitors, unavailable niri, unknown workspaces, and failed actions produce structured errors. Diagnostics can use its open/close method only to close itself.

## Providers and frontend

The shared native providers publish clock, host, niri, battery, and network snapshots. The niri collector consumes `niri msg -j event-stream` through non-blocking GIO callbacks, publishes only synchronized workspace/window state, replaces its cache atomically, clears stale data on loss, and reconnects with bounded delay. The remaining providers read local system state.

`niri.focusedWindows.items` contains one `{ output, window }` item for each active workspace with an output. The window comes from that workspace's `active_window_id`, has optional `title`/`appId` fields, and is `null` when no active window is known. Bar and Quick Panel select their own output through `HostInfo.surface.output`; their Niri status label uses that same fixed context. `niri.focusedOutput` remains the compositor's global keyboard-focus output, not a surface's identity.

`web/` contains the official Vue 3/Vite/TypeScript frontend. `web/packages/shell-api/` is the framework-agnostic typed bridge wrapper and must remain independent of Vue-specific packages. Frontend layout and presentation are web-owned; Rust owns native surface policy and security.

## Configuration and assets

Native TOML controls Bar geometry and input policy. Transient geometry, lifetime, and input behavior are fixed by typed native policy. Configuration is selected from an explicit `--config` path, standard XDG/user configuration locations, then the packaged default.

The web resolver checks an explicit web-directory override, checkout/build locations, standard user data locations, then the system installation location. The selected directory must contain a readable regular `index.html`; all assets must be local. Missing assets, invalid configuration, no monitors, or absent layer-shell support are startup errors. Provider failures are reported in provider state rather than aborting the host.

## Installation and side effects

The documented local install copies the release binary under the user's local binary directory, copies `web-dist/` under the user's local data directory, and installs the default TOML under the user's config directory. It does not enable a service. Packaging examples include an Arch package, a user service, and a niri spawn fragment; none is enabled automatically.

Runtime creates GTK layer-shell windows, reads local configuration/assets and system provider state, and may launch the niri event stream. `niriFocusWorkspace` requests a compositor workspace change. The current implementation performs that action through one synchronous subprocess on the WebKit request path.

## Manual QA and limitations

Use `scripts/smoke-current-niri.sh` only in a prepared current niri session; it creates and closes shell surfaces and exercises compositor-visible state. The `test/*.kdl` files are manual niri session entry points, not VM provisioning. Confirm surface count, namespace, output placement, layers, keyboard modes, disappearance, focus behavior, exclusive zones, rendered content, and HUD pointer transparency.

Known limits include synchronous workspace action dispatch and smoke coverage that cannot prove rendered/focus/pointer behavior. Multi-output lifecycle changes still require compositor-backed post-fix QA. See [`docs/html-desktop-shell-feature-roadmap.md`](docs/html-desktop-shell-feature-roadmap.md) for dated verification and current remediation priorities, and [`codemap.md`](codemap.md) for source navigation.

## License

The repository is dual-licensed, at the recipient's option, under the Apache License 2.0 in `LICENSE-APACHE` or the MIT License in `LICENSE-MIT`. This dual-license boundary applies to the repository; third-party dependencies retain their own licenses.
