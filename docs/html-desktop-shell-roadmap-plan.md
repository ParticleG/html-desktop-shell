# HTML Desktop Shell 长期开发路线计划

## Context

当前项目已是一个 Rust + GTK4 + WebKitGTK 6.0 + gtk4-layer-shell 原型。已读到的当前代码事实：`Cargo.toml` 仅依赖 `gtk4`、`gtk4-layer-shell`、`webkit6`、`javascriptcore6`、`glib`；`src/main.rs` 只创建一个 GTK application 并调用 `shell_window::shell_window_new(app)`；`src/shell_window.rs` 只创建一个 `gtk4::ApplicationWindow`，设置 top layer、32px 高度、exclusive zone、runtime `web/index.html` 查找；`src/bridge.rs` 只注册 `shell` handler 并返回固定 `HOST_INFO_JSON`；`web/` 目前只有 clock 与 bridge 状态。用户已完成当前 niri、tty2 裸 niri、KVM Arch guest 无 DE/display-manager 验证；下一长期路线必须在这个边界内推进：它仍是 Wayland layer-shell client，不变成 compositor，不添加 X11 fallback，不依赖完整 DE/display manager。

## Approach

本路线按可发布增量推进。每个阶段结束时项目必须仍可在当前 niri、tty2 niri、KVM guest 三类环境中运行；任何阶段不得用普通 GTK window fallback 伪装成功。

### Phase 0 — Baseline freeze and immediate multi-monitor cutover

1. Finish the already-planned startup multi-monitor cutover before adding any new shell feature.
   - Target files: `src/main.rs`, `src/shell_window.rs`, `README.md`.
   - Replace the single-window public entry:
     ```rust
     pub fn shell_window_new(app: &gtk4::Application) -> Result<gtk4::ApplicationWindow, String>;
     ```
     with:
     ```rust
     pub fn shell_windows_new(app: &gtk4::Application) -> Result<Vec<gtk4::ApplicationWindow>, String>;
     ```
   - Enumerate `gdk::Display::default().monitors()` at activation time and create one `gtk4::ApplicationWindow` per `gdk::Monitor`.
   - For each window call `window.set_monitor(Some(monitor));` before presenting it.
   - Set namespaces to `html-desktop-shell-panel-<index>` so `niri msg -j layers` can verify each output.
   - Keep `window.set_resizable(true)`; setting it false previously caused the panel to collapse to content width.
   - This phase handles only the startup monitor snapshot. Hotplug is Phase 1, not a hidden requirement here.

2. Preserve current runtime and security boundaries while doing the cutover.
   - Keep `src/bridge.rs` unchanged in this phase: `HANDLER_NAME = "shell"` and `HOST_INFO_JSON = {"shell":"html-desktop-shell","backend":"wayland-layer-shell"}`.
   - Keep runtime asset lookup semantics in `src/shell_window.rs`: first `$PWD/web/index.html`, then `env!("CARGO_MANIFEST_DIR")/web/index.html`.
   - Keep failure literals:
     ```text
     Wayland compositor does not support layer-shell
     missing web/index.html: checked <cwd path> and <manifest path>
     ```

### Phase 1 — Shell host lifecycle and monitor topology

1. Introduce an owned shell host object so windows can be tracked after startup.
   - Add `src/shell_host.rs`.
   - Move multi-window orchestration out of `shell_window.rs` into:
     ```rust
     pub struct ShellHost {
         app: gtk4::Application,
         panels: Vec<gtk4::ApplicationWindow>,
     }

     impl ShellHost {
         pub fn new(app: &gtk4::Application) -> Result<Self, String>;
         pub fn present(&self);
     }
     ```
   - Keep `src/shell_window.rs` as the per-panel factory only:
     ```rust
     pub fn shell_window_for_monitor(
         app: &gtk4::Application,
         monitor: &gtk4::gdk::Monitor,
         index: u32,
         uri: &str,
     ) -> Result<gtk4::ApplicationWindow, String>;
     ```
   - Update `src/main.rs` to retain the host with an `Rc<RefCell<Option<ShellHost>>>` captured by the `connect_activate` closure. Exact pattern:
     ```rust
     let shell_host = std::rc::Rc::new(std::cell::RefCell::new(None));
     let shell_host_for_activate = std::rc::Rc::clone(&shell_host);
     app.connect_activate(move |app| match ShellHost::new(app) {
         Ok(host) => {
             host.present();
             *shell_host_for_activate.borrow_mut() = Some(host);
         }
         Err(message) => {
             eprintln!("{message}");
             app.quit();
         }
     });
     ```
     Do not use `glib::ObjectExt::set_data` for this phase; the captured `Rc<RefCell<Option<ShellHost>>>` makes ownership explicit in safe Rust.

2. Add monitor hotplug reconciliation after the startup multi-monitor behavior is stable.
   - Listen to `gio::ListModel::connect_items_changed` on `gdk::Display::default().monitors()`.
   - On any monitor list change, rebuild the complete `panels` list:
     1. close/destroy existing panel windows;
     2. re-enumerate current monitors;
     3. recreate panels with namespaces `html-desktop-shell-panel-<index>`;
     4. present all panels.
   - Do not try to incrementally match old and new monitors in this phase; full rebuild avoids monitor identity bugs and keeps the behavior obvious.
   - If rebuild fails after a hotplug event, print the error to stderr and leave the previous panel set running if it still exists. Do not quit the app on hotplug failure.

3. Add explicit renderer diagnostic entrypoints, not default renderer overrides.
   - Keep default GTK/WebKit renderer behavior unchanged.
   - Add README commands for diagnostics only:
     ```bash
     GSK_RENDERER=cairo ./target/debug/html-desktop-shell
     ```
   - Do not set `GSK_RENDERER` in test niri configs by default; KVM Mesa/Vulkan warnings remain non-fatal when the panel renders.

### Phase 2 — Native bridge protocol and capability boundary

1. Replace the fixed string-only bridge reply with a versioned JSON request/response protocol.
   - Add dependencies to `Cargo.toml`:
     ```toml
     serde = { version = "1", features = ["derive"] }
     serde_json = "1"
     ```
   - Add `src/messages.rs` with these exact wire types:
     ```rust
     #[derive(serde::Deserialize)]
     pub struct NativeRequest {
         pub id: String,
         pub method: String,
         #[serde(default)]
         pub params: serde_json::Value,
     }

     #[derive(serde::Serialize)]
     pub struct NativeResponse<'a> {
         pub id: &'a str,
         pub ok: bool,
         #[serde(skip_serializing_if = "Option::is_none")]
         pub result: Option<serde_json::Value>,
         #[serde(skip_serializing_if = "Option::is_none")]
         pub error: Option<NativeError>,
     }

     #[derive(serde::Serialize)]
     pub struct NativeError {
         pub code: &'static str,
         pub message: String,
     }
     ```
   - Keep one WebKit handler name: `const HANDLER_NAME: &str = "shell";`.
   - Supported native methods at the end of this phase:
     - `getHostInfo` returns:
       ```json
       {"shell":"html-desktop-shell","backend":"wayland-layer-shell","bridgeVersion":1}
       ```
     - `getCapabilities` returns:
       ```json
       {"methods":["getHostInfo","getCapabilities"]}
       ```
   - Unknown methods return:
     ```json
     {"code":"unknown_method","message":"unknown native method: <method>"}
     ```
   - Malformed JSON returns:
     ```json
     {"code":"bad_request","message":"request must be a JSON object with string id and method"}
     ```

2. Update web-side bridge wrapper without adding frontend tooling.
   - Keep `web/` as plain HTML/CSS/JS through this phase; do not add Node, Bun, Vite, TypeScript, or a framework yet.
   - Replace current `window.shell.getHostInfo()` implementation in `web/shell.js` with:
     ```js
     let nextRequestId = 1;

     window.shell = {
       async request(method, params = {}) {
         const id = String(nextRequestId++);
         const raw = await window.webkit.messageHandlers.shell.postMessage({ id, method, params });
         const response = JSON.parse(raw);
         if (!response.ok) {
           const message = response.error?.message || "native bridge request failed";
           throw new Error(message);
         }
         return response.result;
       },
       getHostInfo() {
         return this.request("getHostInfo");
       },
       getCapabilities() {
         return this.request("getCapabilities");
       },
     };
     ```
   - Keep the visible UI outcome unchanged: successful host info still sets `#bridge-status` to `bridge: wayland-layer-shell`; failure still sets `bridge: unavailable`.

3. Add bridge tests before adding privileged capabilities.
   - Add Rust unit tests in `src/messages.rs` for:
     - valid `getHostInfo` request parses;
     - response serializes `ok: true` and omits `error`;
     - unknown method response uses `unknown_method`.
   - Verification command:
     ```bash
     cargo test
     ```

### Phase 3 — Configuration and local web app structure

1. Add a small explicit configuration file for panel shape, not behavior plugins.
   - Add dependency:
     ```toml
     toml = "0.9"
     ```
   - Add `src/config.rs` with:
     ```rust
     pub struct ShellConfig {
         pub panel_height: i32,
         pub layer: PanelLayer,
         pub keyboard_mode: PanelKeyboardMode,
     }

     pub enum PanelLayer {
         Top,
         Bottom,
         Overlay,
     }

     pub enum PanelKeyboardMode {
         None,
         OnDemand,
         Exclusive,
     }
     ```
   - Default config values must exactly preserve current behavior:
     ```toml
     panel_height = 32
     layer = "top"
     keyboard_mode = "on-demand"
     ```
   - Lookup order:
     1. CLI `--config <path>`;
     2. `$XDG_CONFIG_HOME/html-desktop-shell/config.toml`;
     3. `~/.config/html-desktop-shell/config.toml`;
     4. built-in defaults.
   - If a config file exists but is invalid, return an error and quit; do not silently fall back to defaults.

2. Split `web/shell.js` into local ES modules while keeping no build step.
   - New files:
     ```text
     web/js/bridge.js
     web/js/clock.js
     web/js/app.js
     ```
   - Update `web/index.html` to load:
     ```html
     <script type="module" src="js/app.js"></script>
     ```
   - Keep `web/shell.css` as the single stylesheet until theming is introduced.
   - `web/js/app.js` owns startup only: initialize clock, query `getHostInfo`, update bridge status.

3. Document configuration with exact default file contents.
   - README must include:
     ```toml
     panel_height = 32
     layer = "top"
     keyboard_mode = "on-demand"
     ```
   - Verification must include running with an explicit config:
     ```bash
     ./target/debug/html-desktop-shell --config ./test/panel-default.toml
     ```

### Phase 4 — Useful shell state providers

1. Implement provider trait and state snapshot bridge.
   - Add `src/providers/mod.rs`:
     ```rust
     pub trait Provider {
         fn name(&self) -> &'static str;
         fn snapshot(&self) -> serde_json::Value;
     }
     ```
   - Add bridge method:
     ```text
     getState
     ```
     returning:
     ```json
     {
       "clock": {"time": "<HH:MM:SS>"},
       "host": {"backend": "wayland-layer-shell"}
     }
     ```
   - During this phase, `getState` is pull-based from JS every 1000 ms. Do not add native push events until the request/response protocol and provider snapshots are stable.

2. Add providers in this fixed order.
   - `ClockProvider` first: it replaces the visible JS-only clock after provider plumbing works. `web/js/app.js` must poll `getState` every 1000 ms and render `state.clock.time`; remove the old local `new Date().toLocaleTimeString(...)` clock update so only one clock source exists.
   - `HostProvider` second: reports backend, monitor count, and bridge version:
     ```json
     {"backend":"wayland-layer-shell","monitorCount":<number>,"bridgeVersion":1}
     ```
   - `NiriProvider` third, only when niri is detected:
     - Detection: `NIRI_SOCKET` environment variable exists.
     - If not detected, provider returns:
       ```json
       {"available":false,"reason":"niri IPC unavailable"}
       ```
     - This provider is optional; absence of niri must not prevent the panel from rendering on other layer-shell compositors.
   - Before implementing `NiriProvider`, read niri IPC documentation and the installed niri command/API. If direct IPC API is not stable or not documented, use `niri msg -j` for a first diagnostic-only provider and record the performance limitation in README.

3. Add visible UI widgets only after provider snapshots exist.
   - Add DOM regions in `web/index.html`:
     ```html
     <section id="left-widgets"></section>
     <section id="center-widgets"></section>
     <section id="right-widgets"></section>
     ```
   - Move existing app name to left, clock to center, bridge/provider status to right.
   - Keep CSS height at the configured panel height and preserve horizontal full-width behavior.

### Phase 5 — Session integration and packaging

1. Add user-session integration files without making them auto-install.
   - Add:
     ```text
     packaging/html-desktop-shell.service
     packaging/niri-spawn-html-desktop-shell.kdl
     ```
   - `html-desktop-shell.service` must be a user service and must not assume a DE:
     ```ini
     [Unit]
     Description=HTML Desktop Shell
     After=graphical-session.target

     [Service]
     ExecStart=%h/.local/bin/html-desktop-shell
     Restart=on-failure

     [Install]
     WantedBy=default.target
     ```
   - The niri snippet must use the project binary path only as an example, not as a hardcoded install requirement.
   - Do not enable the service from build scripts or tests.

2. Add Arch packaging after binary install path is decided.
   - Add `PKGBUILD` only after the binary can run from an installed location and still find assets via a configured asset directory.
   - Before adding `PKGBUILD`, add runtime asset lookup for:
     1. `$HTML_DESKTOP_SHELL_WEB_DIR`;
     2. `$PWD/web`;
     3. compile-time manifest `web`;
     4. installed `/usr/share/html-desktop-shell/web`.
   - Missing asset error must list every checked path.

3. Add release gate script.
   - Add `scripts/smoke-current-niri.sh` only after multi-monitor namespaces are implemented.
   - Script may start the binary, sleep, run `niri msg -j layers`, and kill the process.
   - Script must not switch VT, create VMs, install packages, or modify user services.

### Phase 6 — Hardening, permissions, and performance

1. Keep native bridge deny-by-default.
   - Every new bridge method must be listed in `getCapabilities`.
   - Do not add generic methods named `runCommand`, `readFile`, `writeFile`, `dbusCall`, `httpRequest`, or `eval`.
   - Any future privileged action must have one exact method name, one exact JSON parameter schema, and one explicit UI caller.

2. Add process and rendering diagnostics.
   - Add CLI flags:
     ```text
     --print-capabilities
     --print-config
     --check
     ```
   - `--check` must verify layer-shell support, web asset availability, and monitor count, then exit without opening panels.
   - `--print-config` must print the effective config after defaults and file parsing.
   - `--print-capabilities` must print the same methods as bridge `getCapabilities`.

3. Add performance budgets to README after measurement exists.
   - Measure in current niri and KVM guest:
     - startup time until first visible panel;
     - idle CPU while clock updates;
     - resident memory after 60 seconds.
   - Do not state budgets before measurement. After measurement, set release gates to current measured value plus 25% headroom.

### Phase 7 — v1.0 release criteria

1. v1.0 is reached only when all of these are true:
   - one panel per startup monitor works;
   - monitor hotplug full rebuild works and tests cover adding/removing monitor topology by manual monitor plug/unplug or compositor output enable/disable where available;
   - runtime config works from explicit `--config` and XDG config path;
   - bridge protocol is versioned and tested;
   - at least host info, capabilities, and state snapshot methods are implemented;
   - current niri, tty2 niri, and KVM guest smoke tests pass;
   - no X11 fallback exists;
   - raw TTY without compositor still fails instead of opening a normal window.

2. v1.0 documentation must include only commands that were actually executed on this project:
   - host build/run;
   - current niri smoke;
   - tty2 manual smoke;
   - KVM guest smoke;
   - config file example;
   - packaging/install command if packaging exists.

## Critical files & anchors

- `/home/particleg/coding/OtherProjects/html-desktop-shell/src/main.rs` — application activation currently owns the single-window lifecycle; future host object and multi-window presentation start here.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/src/shell_window.rs` — current layer-shell/WebKit surface factory, runtime asset lookup, and all monitor/window placement behavior.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/src/bridge.rs` — current minimal native bridge; all future native capabilities must preserve deny-by-default semantics here or in a replacement bridge module.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/web/index.html` and `/home/particleg/coding/OtherProjects/html-desktop-shell/web/shell.js` — current plain local web UI; split into ES modules only after bridge protocol is stable.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/README.md` — verified support matrix and no-DE/no-compositor boundary; every phase must keep it aligned with observed behavior.

## Verification

1. Every implementation phase must run:
   ```bash
   cd /home/particleg/coding/OtherProjects/html-desktop-shell
   cargo fmt
   cargo build
   ```
   Expected: build succeeds and `target/debug/html-desktop-shell` exists.

2. Any phase changing Rust logic must run:
   ```bash
   cd /home/particleg/coding/OtherProjects/html-desktop-shell
   cargo test
   ```
   Expected: all unit tests pass. If no tests exist yet in the phase, add tests for the changed serialization/config/provider code before claiming the phase complete.

3. Any phase changing panel windows, monitors, layer-shell settings, config, or asset lookup must run current-session smoke:
   ```bash
   cd /home/particleg/coding/OtherProjects/html-desktop-shell
   ./target/debug/html-desktop-shell &
   pid=$!
   sleep 2
   niri msg -j layers
   kill "$pid"
   ```
   Expected after Phase 0: one `html-desktop-shell-panel-<index>` top-layer surface per detected monitor. Before Phase 0 is implemented, the expected current behavior remains one `html-desktop-shell-panel` surface on the compositor-selected output.

4. Any phase changing no-DE behavior, layer-shell setup, config loading, or packaging must repeat tty2 manual smoke:
   ```bash
   cd ~/coding/OtherProjects/html-desktop-shell
   cargo build
   niri --session --config ./test/niri-tty2-host.kdl
   ```
   Expected: niri starts without DE/display manager and the panel appears. Exit with `Super+Shift+E` or `Ctrl+Alt+Delete`.

5. Any phase changing runtime asset lookup, packaging, renderer diagnostics, bridge protocol, or provider state must repeat KVM guest smoke:
   ```bash
   sudo mount -t 9p -o trans=virtio,version=9p2000.L htmlshell /mnt/htmlshell
   rm -rf "$HOME/html-desktop-shell"
   cp -a /mnt/htmlshell "$HOME/html-desktop-shell"
   cd "$HOME/html-desktop-shell"
   cargo clean
   cargo build
   niri --config ./test/niri-kvm-guest.kdl
   ```
   Expected: guest remains no DE/display-manager, panel renders, and `bridge: wayland-layer-shell` appears. Portal warnings and Mesa/Vulkan warnings remain non-fatal if the panel renders.

## Assumptions & contingencies

- Product scope is fixed as a Wayland layer-shell desktop shell client. It will not become a compositor, will not manage DRM/KMS/libinput directly, and will not gain a normal-window fallback for raw TTY.
- Multi-monitor startup support is the next implementation step because the current code creates one `ApplicationWindow` and user testing confirmed one-monitor-only behavior. Hotplug comes after startup multi-monitor support unless the user explicitly reprioritizes it in a separate plan.
- Plain local HTML/CSS/JS remains the frontend architecture through the bridge/config/provider phases. Do not add Bun, Node, Vite, TypeScript, Vue, React, or a bundler until a later plan explicitly moves the web UI to a built frontend.
- Niri integration is the first compositor-specific provider because all completed host, tty2, and KVM tests used niri. Other layer-shell compositors remain supported for rendering, but compositor-specific state may be unavailable and must degrade to an explicit unavailable state instead of failing startup.
- If a future phase needs a privileged bridge capability, implement the smallest named method for that exact capability and add it to `getCapabilities`; never add generic filesystem, process, DBus, network, screenshot, or eval access.
- If a planned crate API differs from installed versions, keep the chosen Rust/GTK4/WebKitGTK/gtk4-layer-shell stack and adjust only the import or symbol names. Do not switch to Electron, WPE, CEF, GTK3, X11, or compositor-specific rendering as a workaround.

