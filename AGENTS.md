# Repository Guidelines

## Project Overview

`html-desktop-shell` is a Rust 2024, GTK4, gtk4-layer-shell, and WebKitGTK 6.0 desktop-shell host for Wayland compositors exposing `zwlr_layer_shell_v1`. Maintainers must use the current files and repository maps rather than older single-panel descriptions. It owns four typed surfaces—Bar, Quick Panel, Workspace HUD, and Diagnostics Dialog—and serves only local static web assets.

This is not a compositor. It has no X11, raw-TTY, normal-window, Electron, or compositor-less fallback. niri is the primary integration target, but the host can run on another layer-shell compositor with niri-specific provider/action state unavailable.

## Architecture & Data Flow

`src/main.rs` loads configuration and diagnostics, then GTK activation creates `ShellHost`. `src/shell_host.rs` verifies layer-shell support, owns monitor enumeration, creates the shared provider registry and trusted WebKit context, and delegates all surface lifecycle work to `SurfaceManager`.

`src/surface_manager.rs` owns one persistent Bar per usable monitor, all transient surfaces, HUD generations/timers, monitor rebuilds, and compositor activation observation. `src/surface.rs` defines typed surface identity, immutable context, capability scope, geometry, lifetime, layer, anchors, keyboard policy, and pointer policy. `src/surface_window.rs` applies those policies and creates a separate WebView/trust latch for each surface.

Bridge flow:

```text
Wayland compositor
  -> SurfaceManager and typed GTK layer-shell windows
  -> independent local WebKitGTK WebViews
  -> exact trusted entry-document gate
  -> WebKit handler "shell"
  -> Bridge v4 method/parameter/caller-surface authorization
  -> shared cached providers or a typed surface/compositor action
  -> structured JSON response
```

Bridge v4 has six exact methods. `getHostInfo` and `getCapabilities` exist on all surfaces; Workspace HUD has no state or action methods; Diagnostics has state and may close itself; Bar and Quick Panel can read state, focus a validated niri workspace, and open/close supported transient surfaces. Browser input never chooses a trusted monitor or output.

Important lifecycle invariants:

- Build a full replacement Bar set before tearing down the current set on monitor changes.
- Close Quick Panel and Diagnostics idempotently on Escape, focus loss, explicit close, rebuild, or shutdown.
- Opening Diagnostics from Quick Panel closes the Quick Panel only after the open succeeds.
- Publish HUDs only from compositor-confirmed post-synchronization workspace activation; suppress initial, recovery, and duplicate events.
- Guard HUD timeout callbacks by generation and apply an empty native input region after realization.
- Shutdown disconnects observers, removes timers, closes all surfaces, and stops providers once; stale queued callbacks do nothing.
- Each WebView gets an independent trust latch and immutable `SurfaceContext`.

## Key Directories

- `src/`: Rust host.
  - `surface.rs`: typed surface policy and context.
  - `surface_manager.rs`: topology, persistent/transient lifecycle, monitor rebuilds, and HUD observation.
  - `surface_window.rs`: GTK layer-shell/WebKit window factory.
  - `shell_host.rs`: application-lifetime host and shared resources.
  - `bridge.rs` and `messages.rs`: security-sensitive Bridge v4 attachment, protocol, validation, and responses.
  - `providers/`: clock, host, niri, battery, and network state; niri includes the event-stream collector.
  - `web_security.rs`: local scheme, navigation/resource blocking, and trust checks.
- `web/`: official Vue 3/Vite/TypeScript frontend.
  - `packages/shell-api/`: framework-independent typed native API.
  - `src/`: stores, views, widgets, composables, plugins, routing, i18n, and tests.
- `web-dist/`: generated runtime assets; do not commit.
- `scripts/`: current-niri smoke harness and assertions.
- `test/`: manual niri configs and sample TOML, not VM provisioning.
- `packaging/`: inactive-by-default Arch, user-service, niri-spawn, and default-config examples.
- `examples/plain-html/`: non-Vue frontend example.
- `docs/`: implementation plan and active feature roadmap.

## Development Commands

Run from the repository root:

```bash
(cd web && bun install --frozen-lockfile)
(cd web && bun run typecheck)
(cd web && bun test)
(cd web && bun run build)
cargo test --locked
cargo build --locked
./target/debug/html-desktop-shell
```

The frontend build writes `web-dist/`; Cargo writes `target/`. Runtime requires an active layer-shell Wayland session. Diagnostic modes are:

```bash
./target/debug/html-desktop-shell --print-capabilities
./target/debug/html-desktop-shell --config ./test/panel-default.toml --print-config
./target/debug/html-desktop-shell --check
```

Use `scripts/smoke-current-niri.sh` only in a prepared niri session because it launches and closes surfaces and invokes niri inspection/actions. Packaging and local-install commands copy binaries, generated assets, config, and integration examples; no service is enabled automatically.

## Code Conventions & Common Patterns

Rust:

- Follow Rust 2024 naming and formatting conventions.
- Keep GTK/WebKit work on the GLib main loop; the niri collector uses non-blocking GIO callbacks rather than an async Rust runtime.
- Return explicit errors at module boundaries and avoid panics in runtime paths.
- Keep surface policy typed and centralized. Do not scatter string-based surface checks through bridge or UI code.
- Preserve transactional monitor rebuild and idempotent close/shutdown behavior.
- Keep provider reads cache-only; they must not wait for niri IPC.
- Keep the bridge deny-by-default and validate exact object shape, caller capability, trusted entry document, monitor context, and workspace allowlist.

Web:

- Use Bun and the committed lockfile. Keep `web-dist/` generated.
- The shell API package must remain framework-agnostic; it must not import Vue, Pinia, Router, i18n, or UI libraries.
- Frontend code owns layout and presentation; Rust owns native geometry, surface lifetime/input policy, and security.
- Use local assets only. Do not add remote scripts, remote navigation, or a generic native-command escape hatch.
- Keep per-view polling and stores isolated even though native provider state is shared.

## Important Files

- `Cargo.toml` and `Cargo.lock`: Rust crate, native dependencies, and locked graph.
- `README.md`: current user-facing support, build, architecture, QA, limitations, and dual-license boundary.
- `codemap.md`: current repository atlas; child `codemap.md` files provide subsystem navigation.
- `src/surface_manager.rs`: highest-risk lifecycle and topology implementation.
- `src/surface.rs`: authoritative typed policies and capability relationship.
- `src/surface_window.rs`: native geometry, input region, WebKit creation, and per-view trust setup.
- `src/bridge.rs`, `src/messages.rs`, and `src/web_security.rs`: native/web security boundary.
- `docs/html-desktop-shell-feature-roadmap.md`: outstanding defects and release gates.
- `scripts/niri-layer-smoke.ts` and its tests: compositor-visible automated smoke behavior.
- `LICENSE-APACHE` and `LICENSE-MIT`: alternative repository licenses, at the recipient's option.

## Runtime/Tooling Preferences

Use Cargo for Rust and Bun for the official frontend. Required native libraries are GTK4, gtk4-layer-shell, and WebKitGTK 6.0, discoverable through pkg-config. The target runtime is a Wayland compositor with layer-shell, preferably niri. Do not introduce X11, Electron, Tauri, Qt WebEngine, generic HTTP serving, or fallback window backends into this architecture without an explicit project-level redesign.

Configuration and assets follow explicit-path, XDG/user, checkout/build, and system-package resolution rules documented in the README. Error messages for unresolved assets should retain the checked candidate list without embedding machine-specific documentation.

## Testing & QA

For permanent changes, run the relevant frontend tests/typecheck/build and Rust tests/build. Pure protocol, policy, serialization, collector, and view-model behavior belongs in automated tests. Full GTK layer-shell/WebKit and multi-monitor behavior requires compositor-backed QA.

In a prepared niri session, verify all four surface types, one Bar per usable monitor, namespaces, layers, output placement, exclusive zones, keyboard modes, transient idempotence, focus-loss/Escape closure, Diagnostics-from-Quick-Panel ordering, HUD content/generation timeout, native pointer transparency, monitor hotplug transactionality, and clean shutdown. Also test malformed/unknown/unauthorized Bridge calls, remote/navigation/resource rejection, stale monitors, unavailable niri, collector desynchronization/reconnect, and provider shutdown.

The existing smoke validator does not prove rendered content, focus, exclusive-zone geometry, or pointer transparency. Keep those as explicit observations. Two-active-output post-fix QA remains a release gate for subsequent lifecycle/output changes; preserve dated verification records and do not convert an unobserved procedure into a pass claim.
