# Repository Guidelines

## Project Overview

`html-desktop-shell` is a Rust prototype for a top desktop panel rendered with local static web assets inside a GTK4 WebKitGTK 6.0 view. The native window is a Wayland `zwlr_layer_shell_v1` client via `gtk4-layer-shell`.

Hard boundary: this is not a compositor. It supports layer-shell-capable Wayland compositors, including niri. It intentionally has no X11, raw TTY, Electron, normal-window, or compositor-less fallback.

## Architecture & Data Flow

High-level flow:

```text
Wayland compositor
  -> GTK4 layer-shell ApplicationWindow per active monitor
  -> WebKitGTK WebView
  -> local generated web assets (`web-dist/` in checkout, installed as `web/`)
  -> WebKit script message handler named "shell"
  -> Rust bridge returns versioned JSON responses
  -> frontend polls getState and updates panel widgets
```

Key modules:

- `src/main.rs`: creates `gtk4::Application` with `APP_ID`, loads config and diagnostics, owns `ShellHost` for the application lifetime.
- `src/shell_host.rs`: owns monitor enumeration, hotplug rebuilds, provider registry, and WebView asset lookup.
- `src/shell_window.rs`: creates one layer-shell/WebKit panel for a specific monitor.
- `src/bridge.rs`: registers the single WebKit message handler `shell` and routes versioned bridge requests.
- `src/messages.rs`: native bridge wire protocol, capabilities, and tests.
- `src/providers/`: native state providers used by `getState`.
- `web/`: official Vue 3 + Vite + TypeScript sample frontend source. Production assets are generated to `web-dist/` and installed as static web assets.
- `test/`: niri compositor configs for manual QA, not automated test code.

Important invariants:

- Call `gtk4_layer_shell::is_supported()` before creating the panel behavior. Unsupported layer-shell is fatal and should remain a clean `Err` path.
- Default panel height is 32px, but `panel_height` config may change it. Native exclusive zone/default size and CSS layout must stay consistent.
- Defaults are top layer, left/right/top anchors, no bottom anchor, and `KeyboardMode::OnDemand`; config may change layer and keyboard mode only through typed values.
- Connect the WebKit reply callback before registering the `shell` message handler.
- The bridge is deny-by-default. Do not add filesystem, process, network, DBus, clipboard, screenshot, notification, session-control, or generic eval access without an explicit design change.
- Web asset lookup checks `$HTML_DESKTOP_SHELL_WEB_DIR`, `$PWD/web-dist`, compile-time manifest `web-dist`, XDG/local data web dirs, then `/usr/share/html-desktop-shell/web`; missing asset errors should list every checked path.

## Key Directories

- `src/`: Rust host application.
  - `main.rs`: GTK application entry point.
  - `shell_window.rs`: layer-shell panel host and WebView loader.
  - `bridge.rs`: JS-to-Rust WebKit bridge.
- `web/`: official Vue 3 + Vite + TypeScript sample frontend source.
  - `packages/shell-api/`: framework-agnostic TypeScript API wrapper for the native bridge.
  - `src/layout.ts`: default official sample widget layout; Rust does not define widget placement.
  - `src/`: Vue sample components, store, router, i18n, styles, and view-model tests.
- `test/`: manual niri QA configs.
  - `niri-tty2-host.kdl`: host tty2 no-DE/display-manager test.
  - `niri-kvm-guest.kdl`: KVM guest no-DE/display-manager test, also starts `foot`.
- `docs/`: roadmap and design history. `docs/html-desktop-shell-feature-roadmap.md` is the active feature roadmap after the foundational layer-shell work.

## Development Commands

Use Bun for the official frontend sample and Cargo for the Rust host. The repository also has `scripts/smoke-current-niri.sh` for current-session smoke checks.

```bash
cd web
bun install
bun run typecheck
bun test
bun run build
cd ..
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

No project-specific Rust lint or format configuration is present. For Rust-only changes, use standard Cargo/Rust tooling available in the environment (`cargo build`, and `cargo fmt`/`cargo clippy` when those components are installed). For web changes, use the Bun/Vite frontend workflow and keep generated `web-dist/` out of git.

## Code Conventions & Common Patterns

Rust:

- Edition: 2024 (`Cargo.toml`).
- Naming: `snake_case` functions/modules, `SCREAMING_SNAKE_CASE` constants, GTK types from gtk-rs.
- Error handling: return `Result` with simple strings at module boundaries; log with `eprintln!`; avoid panics in runtime paths.
- No async Rust runtime and no worker threads. GTK/WebKit callbacks run on the GTK main loop.
- Prefer explicit constants for magic values: `APP_ID`, `PANEL_NAMESPACE_PREFIX`, `BRIDGE_VERSION`, `HANDLER_NAME`.
- Keep fallibility visible. Layer-shell unsupported and missing runtime `index.html` assets are fatal; bridge attach failure is logged but the panel may still render.

Web:

- Core runtime loads only local static assets. The official sample uses Vue 3 + TypeScript + Vite, source lives in `web/`, the Bun lockfile is committed, production assets are generated to `web-dist/`, and CDN/remote scripts/assets are not allowed.
- The stable frontend API is framework-agnostic `@html-desktop-shell/shell-api`; it must not import Vue, PrimeVue, Pinia, Router, i18n, VueUse, or Lucide.
- Official sample component names/layout live in `web/src/layout.ts`; Rust does not define widget placement, visibility, or styling.
- CSS must adapt to the native panel height; Rust config/exclusive zone and visual layout must stay consistent.

State management:

- Native state is held by `ShellHost`, provider snapshots, config, and monitor list handles.
- Browser-side state is derived from `getHostInfo` and `getState` polling and should remain small, explicit DOM text/classes.
- No persistence layer, IPC server, async Rust runtime, or background thread system exists.

## Important Files

- `Cargo.toml`: crate manifest and dependency versions (`gtk4`, `gtk4-layer-shell`, `webkit6`, `javascriptcore6`, `glib`).
- `Cargo.lock`: locked Rust dependency graph; keep it updated with dependency changes.
- `README.md`: authoritative user-facing build, run, dependency, support-matrix, and manual verification instructions.
- `docs/html-desktop-shell-feature-roadmap.md`: active post-foundation feature roadmap. Check before adding panel widgets or native bridge capabilities.
- `src/shell_window.rs`: highest-risk file for compositor behavior, window geometry, and WebKit loading.
- `src/bridge.rs`: security-sensitive boundary between web UI and native host.
- `test/*.kdl`: manual compositor-session entry points.

## Runtime/Tooling Preferences

- Required runtime/build stack: Rust/Cargo, Bun, GTK4, `gtk4-layer-shell`, WebKitGTK 6.0, `pkgconf`, and a Wayland compositor with `zwlr_layer_shell_v1`.
- Target compositor for local validation: niri.
- Package manager in docs: Arch `pacman`; dry-run the `webkitgtk-6.0` transaction before installing.
- Web runtime: WebKitGTK inside GTK4, not Node, Electron, Tauri, wry, WPE, CEF, or Qt WebEngine.
- Do not add fallback backends casually. X11 would require a separate EWMH dock/strut implementation; it cannot reuse the Wayland layer-shell path.
- Do not add generic bridge methods or filesystem/process/network/DBus/clipboard/screenshot/session/eval access.

## Testing & QA

There are Rust tests and Bun frontend tests. `test/` contains manual niri configs only.

Required smoke check after behavior changes:

1. `cd web && bun install && bun run typecheck && bun test && bun run build`
2. `cargo build`
3. Run `./target/debug/html-desktop-shell` under a layer-shell-capable Wayland session.
4. Verify visually:
   - 32px top panel appears.
   - Left text is `HTML Shell`.
   - Center clock updates once per second.
   - Right status includes `bridge: wayland-layer-shell`.
   - Maximized windows do not cover the top 32px, proving the exclusive zone is active.

Manual environment checks:

- Host no-DE/display-manager path: run `niri --session --config ./test/niri-tty2-host.kdl` from tty2.
- Boundary check: running the binary directly from a raw TTY without a compositor must not display a fallback window.
- KVM path is documented in `README.md` and uses `test/niri-kvm-guest.kdl`; it is designed for isolated validation, not a routine local test.

If adding automated tests later, start with pure logic around path resolution and bridge payload behavior. Full layer-shell/WebKit behavior needs a compositor-backed integration environment.
