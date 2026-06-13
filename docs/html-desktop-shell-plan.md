# HTML/CSS/JS Desktop Shell 下一阶段实施计划

## Context

当前首期目标已经跑通：`html-desktop-shell` 在当前 niri 会话、tty2 裸 niri 会话、KVM Arch guest 的无 DE/display manager 环境中都能作为 GTK4/WebKitGTK layer-shell panel 渲染，并显示 `bridge: wayland-layer-shell`。当前代码仍是单窗口模型：`src/main.rs` 只调用一次 `shell_window::shell_window_new(app)`，`src/shell_window.rs` 只创建一个 `gtk4::ApplicationWindow`，且没有调用 `gtk4_layer_shell::LayerShell::set_monitor()`，所以多显示器时只会由 compositor 放到一个默认 output。下一阶段路线固定为：先把 shell host 从“单 panel 原型”升级为“启动时每个 GDK monitor 一个 layer-shell panel”，继续保持 Rust + GTK4 + WebKitGTK 6.0 + gtk4-layer-shell，不扩展 native bridge 权限、不引入 DE/display-manager 依赖、不添加 X11 fallback。

## Approach

1. 将公开入口从单窗口改为多窗口创建。
   - 在 `src/shell_window.rs` 删除公开函数：
     ```rust
     pub fn shell_window_new(app: &gtk4::Application) -> Result<gtk4::ApplicationWindow, String>;
     ```
   - 新增公开函数，作为 `main.rs` 唯一调用入口：
     ```rust
     pub fn shell_windows_new(app: &gtk4::Application) -> Result<Vec<gtk4::ApplicationWindow>, String>;
     ```
   - 运行实现前必须执行符号引用检查：
     ```bash
     # working directory: /home/particleg/coding/OtherProjects/html-desktop-shell
     # Expected references before edit: src/shell_window.rs definition and src/main.rs callsite only.
     ```
     Use LSP references if available; otherwise `search` for literal `shell_window_new` must return exactly `src/shell_window.rs` and `src/main.rs`.
   - Update the only callsite in `src/main.rs`: replace `shell_window::shell_window_new(app)` with `shell_window::shell_windows_new(app)`. On `Ok(windows)`, iterate and call `window.present()` for every window:
     ```rust
     Ok(windows) => {
         for window in windows {
             window.present();
         }
     }
     ```
   - Keep the current error path unchanged: print the returned error string to stderr and call `app.quit()`; do not panic.

2. Enumerate current GDK monitors at activation time and create one panel per monitor.
   - In `src/shell_window.rs`, import the GTK reexports and traits needed for monitor enumeration without adding Cargo dependencies:
     ```rust
     use gtk4::{
         gdk,
         gio::prelude::ListModelExt,
         glib::prelude::CastNone,
         prelude::*,
     };
     ```
     Keep the existing `std::{env, path::PathBuf}`, `gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell}`, and `webkit6::prelude::*` imports as needed.
   - `shell_windows_new()` must first call `gtk4_layer_shell::is_supported()` once. If false, return the existing literal unchanged:
     ```text
     Wayland compositor does not support layer-shell
     ```
   - Get the default display with:
     ```rust
     let display = gdk::Display::default().ok_or_else(|| "missing default GDK display".to_owned())?;
     let monitors = display.monitors();
     ```
   - If `monitors.n_items() == 0`, return exact error:
     ```text
     no GDK monitors available
     ```
   - Resolve the local web UI once before creating windows by reusing the existing `web_index_path()` helper and `glib::filename_to_uri(...)` path. Do not perform one filesystem lookup per monitor.
   - Iterate `0..monitors.n_items()`. For each item, downcast with `monitors.item(index).and_downcast::<gdk::Monitor>()`.
     - If a list item is missing or not a `gdk::Monitor`, print to stderr and skip it:
       ```text
       skipping non-monitor GDK list item at index <index>
       ```
     - After iteration, if no window was created, return exact error:
       ```text
       no usable GDK monitors available
       ```
   - This step intentionally handles only the monitor snapshot present at application activation. Monitor hotplug after startup requires app restart in this increment; do not add live hotplug state or signal handlers yet.

3. Factor the existing layer-shell/WebKit setup into a private per-monitor factory.
   - Add private helper in `src/shell_window.rs`:
     ```rust
     fn shell_window_for_monitor(
         app: &gtk4::Application,
         monitor: &gdk::Monitor,
         index: u32,
         uri: &str,
     ) -> Result<gtk4::ApplicationWindow, String>;
     ```
   - Move the current window setup from `shell_window_new()` into this helper. Keep these existing behaviors unchanged:
     - `window.set_title(Some("HTML Desktop Shell"));`
     - `window.set_decorated(false);`
     - `window.set_resizable(true);` — this must remain true; setting it false previously caused the panel to collapse to natural content width.
     - `window.set_layer(Layer::Top);`
     - left/right/top anchors true, bottom anchor false.
     - `window.set_exclusive_zone(PANEL_HEIGHT);`
     - `window.set_keyboard_mode(KeyboardMode::OnDemand);`
     - `window.set_default_size(0, PANEL_HEIGHT);`
     - `web_view.set_hexpand(true);`
     - `web_view.set_vexpand(true);`
     - `bridge::attach_bridge(&web_view)` errors are printed to stderr but do not prevent panel display.
     - `web_view.connect_load_failed(...)` prints `WebKit load failed for <failing_uri>: <error>` and returns `false`.
   - Add the monitor binding before the window is presented, after `init_layer_shell()` and before anchors are set:
     ```rust
     window.set_monitor(Some(monitor));
     ```
   - Replace the single namespace constant with a prefix constant:
     ```rust
     const PANEL_NAMESPACE_PREFIX: &str = "html-desktop-shell-panel";
     ```
     For each window, set namespace to:
     ```rust
     let namespace = format!("{PANEL_NAMESPACE_PREFIX}-{index}");
     window.set_namespace(Some(namespace.as_str()));
     ```
     Expected observable layer namespaces become `html-desktop-shell-panel-0`, `html-desktop-shell-panel-1`, ... in `niri msg layers`. This small allocation is acceptable because it happens once per monitor at startup and makes multi-monitor verification unambiguous.
   - Do not change `src/bridge.rs`; the only native bridge remains `HANDLER_NAME = "shell"` returning the current `HOST_INFO_JSON` literal.

4. Preserve runtime asset lookup and make the multi-monitor path use it consistently.
   - Keep existing helper semantics in `src/shell_window.rs`: `web_index_path()` first checks `$PWD/web/index.html`, then falls back to `env!("CARGO_MANIFEST_DIR")/web/index.html`.
   - Keep the missing-asset error shape introduced after the KVM fix:
     ```text
     missing web/index.html: checked <cwd path> and <manifest path>
     ```
   - Convert the resolved path to URI once in `shell_windows_new()` using the existing error style:
     ```rust
     let uri = glib::filename_to_uri(&html_path, None).map_err(|error| {
         format!(
             "failed to create file URI for {}: {error}",
             html_path.display()
         )
     })?;
     ```

5. Update README and niri test expectations for multi-monitor behavior.
   - In `README.md`, change current-session, tty2, and KVM expected results from “a/the 32px panel appears” to “one 32px panel appears on each detected monitor”; for KVM with the default virtio display this normally means one panel.
   - Document the new layer namespace pattern `html-desktop-shell-panel-<index>` and use `niri msg -j layers` as the machine-readable verification command.
   - Keep existing notes about `dms:bar`/dankbar stacking, portal warnings, and Mesa/Vulkan warnings. Do not make `GSK_RENDERER=cairo` the default; it remains an optional diagnostic command.
   - Add one sentence to the multi-monitor note: monitor hotplug after app startup is not handled in this increment; restart the app after changing monitor topology.

## Critical files & anchors

- `/home/particleg/coding/OtherProjects/html-desktop-shell/src/main.rs:11` — only caller of `shell_window_new`; must switch to `shell_windows_new` and present all returned windows.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/src/shell_window.rs:12-78` — current single-window layer-shell/WebKit setup; refactor into monitor enumeration plus private per-monitor factory.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/src/bridge.rs:3-25` — leave bridge literal and handler unchanged to avoid expanding native privileges while changing display topology.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/README.md:73-91` — current-session expected result must reflect one panel per detected monitor and namespace verification.
- `/home/particleg/coding/OtherProjects/html-desktop-shell/README.md:93-113,181-190` — tty2/KVM expected results and warning analysis must reflect the multi-monitor snapshot behavior without changing the no-DE/no-compositor boundary.

## Verification

1. Symbol and build verification.
   - Working directory: `/home/particleg/coding/OtherProjects/html-desktop-shell`
   - Before editing, confirm `shell_window_new` references are only the current definition and `src/main.rs` callsite.
   - After editing, run:
     ```bash
     cargo fmt
     cargo build
     ```
   - Expected: build succeeds; `target/debug/html-desktop-shell` exists; no references to the old public `shell_window_new(app)` signature remain.

2. Current niri multi-monitor smoke test.
   - Working directory: `/home/particleg/coding/OtherProjects/html-desktop-shell`
   - Command:
     ```bash
     ./target/debug/html-desktop-shell &
     pid=$!
     sleep 2
     niri msg -j layers
     kill "$pid"
     ```
   - Expected observable output: `niri msg -j layers` contains one top-layer surface per monitor with namespaces `html-desktop-shell-panel-0`, `html-desktop-shell-panel-1`, ... and `keyboard_interactivity` equal to `OnDemand`. On the current dual-monitor host, the user should visually see a 32px bar on both monitors; if dankbar/dms:bar is running, the prototype may stack below it but must still span each monitor horizontally.

3. Host tty2 no-DE/display-manager regression test.
   - Manual from physical console; do not run automatically from the execution agent.
   - Precondition: either end the current graphical session or accept that a second compositor may fail to acquire DRM/session control.
   - Commands on tty2:
     ```bash
     cd ~/coding/OtherProjects/html-desktop-shell
     cargo build
     niri --session --config ./test/niri-tty2-host.kdl
     ```
   - Expected: niri starts without DE/display manager, `html-desktop-shell` autostarts, and one 32px panel appears on each monitor detected by that tty2 niri session. Exit with `Super+Shift+E` or `Ctrl+Alt+Delete`.

4. KVM guest regression test.
   - Inside the existing Arch guest after recopying the host project from the 9p mount:
     ```bash
     sudo mount -t 9p -o trans=virtio,version=9p2000.L htmlshell /mnt/htmlshell
     rm -rf "$HOME/html-desktop-shell"
     cp -a /mnt/htmlshell "$HOME/html-desktop-shell"
     cd "$HOME/html-desktop-shell"
     cargo clean
     cargo build
     niri --config ./test/niri-kvm-guest.kdl
     ```
   - Expected: the guest still runs with no DE/display manager, the panel renders, and `bridge: wayland-layer-shell` appears. With the default single virtio display, expect only `html-desktop-shell-panel-0`. Portal and Mesa/Vulkan warnings remain non-fatal if the panel renders.

## Assumptions & contingencies

- The next implementation uses the GDK monitor snapshot at activation time only. If the user needs live monitor hotplug in the same run, do not invent it inside this plan; finish this startup multi-monitor cutover first, then create a separate plan for `GListModel::items-changed` reconciliation.
- If `display.monitors()` returns zero items, the app exits with `no GDK monitors available` rather than creating an unspecified compositor-chosen window. This makes a broken display/session state obvious.
- If any `gio::ListModel` item fails to downcast to `gdk::Monitor`, skip that item and continue; if every item fails, exit with `no usable GDK monitors available`.
- If `window.set_monitor(Some(monitor))` causes a crate API mismatch during implementation, keep the `gtk4-layer-shell` stack and adjust only the import/type paths to the installed `gtk4-layer-shell 0.8.0` API; do not replace it with compositor-specific niri IPC, X11 fallback, Electron, WPE, or CEF.
