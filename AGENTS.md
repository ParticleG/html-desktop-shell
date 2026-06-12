# Repository Guidelines

## Project Overview

`html-desktop-shell` is a Rust prototype for a top desktop panel rendered with local HTML/CSS/JS inside a GTK4 WebKitGTK 6.0 view. The native window is a Wayland `zwlr_layer_shell_v1` client via `gtk4-layer-shell`.

Hard boundary: this is not a compositor. It supports layer-shell-capable Wayland compositors, including niri. It intentionally has no X11, raw TTY, Electron, normal-window, or compositor-less fallback.

## Architecture & Data Flow

High-level flow:

```text
Wayland compositor
  -> GTK4 layer-shell ApplicationWindow (top layer, 32px exclusive zone)
  -> WebKitGTK WebView
  -> local web/index.html + shell.css + shell.js
  -> WebKit script message handler named "shell"
  -> Rust bridge returns HOST_INFO_JSON
  -> JS updates #bridge-status
```

Key modules:

- `src/main.rs`: creates `gtk4::Application` with `APP_ID`, calls `shell_window::shell_window_new()` on activation, presents the window on success, logs and quits on error.
- `src/shell_window.rs`: owns layer-shell setup, panel geometry, WebView creation, bridge attachment, local `web/index.html` resolution, and WebKit load-failure logging.
- `src/bridge.rs`: registers the single WebKit message handler `shell` and returns static JSON: `{"shell":"html-desktop-shell","backend":"wayland-layer-shell"}`.
- `web/`: static panel UI. No bundler, no remote resources, no CDN.
- `test/`: niri compositor configs for manual QA, not automated test code.

Important invariants:

- Call `gtk4_layer_shell::is_supported()` before creating the panel behavior. Unsupported layer-shell is fatal and should remain a clean `Err` path.
- Keep the panel `32px` high. `PANEL_HEIGHT`, CSS heights, and the exclusive zone must stay in sync.
- The window is top-layer, anchored left/right/top, not bottom, with `KeyboardMode::OnDemand`.
- Connect the WebKit reply callback before registering the `shell` message handler.
- The bridge is intentionally minimal. Do not add filesystem, process, network, DBus, clipboard, screenshot, notification, or session-control access without an explicit design change.
- `web_index_path()` checks the current directory first, then `CARGO_MANIFEST_DIR`; preserve this development/run-from-elsewhere behavior.

## Key Directories

- `src/`: Rust host application.
  - `main.rs`: GTK application entry point.
  - `shell_window.rs`: layer-shell panel host and WebView loader.
  - `bridge.rs`: JS-to-Rust WebKit bridge.
- `web/`: local HTML/CSS/JS panel content.
  - `index.html`: fixed DOM anchors: `#app-name`, `#clock`, `#bridge-status`.
  - `shell.css`: 32px translucent top-bar layout.
  - `shell.js`: clock update and bridge-status update.
- `test/`: manual niri QA configs.
  - `niri-tty2-host.kdl`: host tty2 no-DE/display-manager test.
  - `niri-kvm-guest.kdl`: KVM guest no-DE/display-manager test, also starts `foot`.
- `docs/`: design history. `docs/html-desktop-shell-plan.md` is the detailed implementation plan and records rejected alternatives.

## Development Commands

Use Cargo directly; there are no wrapper scripts, workspace config, `build.rs`, Node/Bun/npm tooling, or web asset build steps.

```bash
cargo build
./target/debug/html-desktop-shell
```

Run from a Wayland compositor that exposes `zwlr_layer_shell_v1`. Running without a supported compositor should fail instead of opening a fallback window.

Dependency check on Arch, before installing WebKitGTK 6.0:

```bash
pacman -S --print --needed --print-format '%n %v %s' webkitgtk-6.0
sudo -v
sudo pacman -S --needed webkitgtk-6.0
pkg-config --modversion webkitgtk-6.0
```

Manual niri runs:

```bash
# Host tty2 manual test from the project root
niri --session --config ./test/niri-tty2-host.kdl

# Inside the designed KVM guest flow
niri --config ./test/niri-kvm-guest.kdl
```

No project-specific lint or format configuration is present. For Rust-only changes, use standard Cargo/Rust tooling available in the environment (`cargo build`, and `cargo fmt`/`cargo clippy` when those components are installed). For web changes, edit the static files directly.

## Code Conventions & Common Patterns

Rust:

- Edition: 2024 (`Cargo.toml`).
- Naming: `snake_case` functions/modules, `SCREAMING_SNAKE_CASE` constants, GTK types from gtk-rs.
- Error handling: return `Result` with simple strings at module boundaries; log with `eprintln!`; avoid panics in runtime paths.
- No async Rust runtime and no worker threads. GTK/WebKit callbacks run on the GTK main loop.
- Prefer explicit constants for magic values: `APP_ID`, `PANEL_HEIGHT`, `PANEL_NAMESPACE`, `HOST_INFO_JSON`, `HANDLER_NAME`.
- Keep fallibility visible. Layer-shell unsupported and missing `web/index.html` are fatal; bridge attach failure is logged but the panel may still render.

Web:

- Plain HTML/CSS/JS only. No bundler, framework, package manager, or remote assets.
- DOM IDs are stable integration points: `app-name`, `clock`, `bridge-status`, `shell-bar`.
- JS uses `async`/`await` only for the WebKit bridge call and catches bridge failures by showing `bridge: unavailable`.
- CSS assumes a 32px panel; update CSS and Rust constants together.

State management:

- Native state is effectively static after window creation.
- Browser-side state is limited to the live clock text and bridge status text.
- No dependency injection framework, global service registry, persistence layer, IPC server, or background task system exists.

## Important Files

- `Cargo.toml`: crate manifest and dependency versions (`gtk4`, `gtk4-layer-shell`, `webkit6`, `javascriptcore6`, `glib`).
- `Cargo.lock`: locked Rust dependency graph; keep it updated with dependency changes.
- `README.md`: authoritative user-facing build, run, dependency, support-matrix, and manual verification instructions.
- `docs/html-desktop-shell-plan.md`: design rationale and rejected alternatives. Check before changing architecture or support boundaries.
- `src/shell_window.rs`: highest-risk file for compositor behavior, window geometry, and WebKit loading.
- `src/bridge.rs`: security-sensitive boundary between web UI and native host.
- `test/*.kdl`: manual compositor-session entry points.

## Runtime/Tooling Preferences

- Required runtime/build stack: Rust/Cargo, GTK4, `gtk4-layer-shell`, WebKitGTK 6.0, `pkgconf`, and a Wayland compositor with `zwlr_layer_shell_v1`.
- Target compositor for local validation: niri.
- Package manager in docs: Arch `pacman`; dry-run the `webkitgtk-6.0` transaction before installing.
- Web runtime: WebKitGTK inside GTK4, not Node, Bun, Electron, Tauri, wry, WPE, CEF, or Qt WebEngine.
- Do not add fallback backends casually. X11 would require a separate EWMH dock/strut implementation; it cannot reuse the Wayland layer-shell path.
- Avoid introducing generated assets or build steps unless the architecture changes explicitly require them.

## Testing & QA

There are currently no automated Rust tests, JS tests, CI files, or coverage configuration. `test/` contains manual niri configs only.

Required smoke check after behavior changes:

1. `cargo build`
2. Run `./target/debug/html-desktop-shell` under a layer-shell-capable Wayland session.
3. Verify visually:
   - 32px top panel appears.
   - Left text is `HTML Shell`.
   - Center clock updates once per second.
   - Right text changes from `bridge: pending` to `bridge: wayland-layer-shell`.
   - Maximized windows do not cover the top 32px, proving the exclusive zone is active.

Manual environment checks:

- Host no-DE/display-manager path: run `niri --session --config ./test/niri-tty2-host.kdl` from tty2.
- Boundary check: running the binary directly from a raw TTY without a compositor must not display a fallback window.
- KVM path is documented in `README.md` and uses `test/niri-kvm-guest.kdl`; it is designed for isolated validation, not a routine local test.

If adding automated tests later, start with pure logic around path resolution and bridge payload behavior. Full layer-shell/WebKit behavior needs a compositor-backed integration environment.
