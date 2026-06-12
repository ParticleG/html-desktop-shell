# HTML/CSS/JS Desktop Shell 实施计划

## Context

用户要求把此前的可行性结论推进为可执行实施方案，并在方案确认后尝试执行；新增要求是确认该项目能否在完全没有 DE 或 WM 的环境运行、设计 tty2 或 KVM 测试方案，并把最终 plan 写入项目目录的 `docs` 文件夹。结论写死为：**不能在“裸 TTY、没有任何 Wayland compositor”的环境运行**，因为 GTK4/WebKitGTK layer-shell client 必须连接到 Wayland compositor；但它可以在**没有 GNOME/KDE/XFCE 等 DE、没有 display manager、没有现有桌面会话**的环境运行，只要启动一个支持 `zwlr_layer_shell_v1` 的最小 Wayland compositor。首期仍固定为 Rust + WebKitGTK 6.0 + GTK4 layer-shell 顶部 panel；测试默认先在当前 niri 会话跑通，再提供 tty2 的 niri 裸会话测试，KVM 作为不影响当前桌面的隔离测试方案。

当前本机事实：`~/coding/OtherProjects/html-desktop-shell` 项目目录已存在，包含 `Cargo.toml`、`src/`、`web/`、`test/` 和刚写入的 `docs/html-desktop-shell-plan.md`；当前会话是 Wayland/niri（`XDG_SESSION_TYPE=wayland`、`WAYLAND_DISPLAY=wayland-1`、`XDG_CURRENT_DESKTOP=niri`）；`niri` 26.04 和 `niri-session` 已安装，`niri validate --config /dev/null` 成功，`/usr/share/doc/niri/default-config.kdl` 中 `spawn-at-startup`、`spawn-sh-at-startup`、`Mod+Shift+E { quit; }` 语法已确认；`gtk4-layer-shell` 文档说明它只在支持 Layer Shell protocol 的 Wayland compositor 上工作，不支持任何 X11 desktop；niri wiki 有专门的 layer-shell components 页面，并说明 top layer 能保持在 overview 之上。已安装 `rustc`/`cargo` 1.96.0、`gtk4` 4.22.4、`gtk4-layer-shell` 1.3.0、`webkit2gtk-4.1` 2.52.4、`cmake` 4.3.3、`ninja` 1.13.2、`pkgconf` 2.5.1；未安装 GTK4 对应的 `webkitgtk-6.0`。只读依赖评估显示 Arch 仓库有 `extra/webkitgtk-6.0` 2.52.4-1，下载大小 35.81 MiB、安装后大小 130.33 MiB；其 52 个依赖当前都已安装，`pacman -S --print --needed --print-format '%n %v %s' webkitgtk-6.0` 当前只输出 `webkitgtk-6.0 2.52.4-1 37552755`。KVM 只读事实：`/dev/kvm` 存在，当前用户在 `kvm` 和 `libvirt` 组，`qemu-system-x86_64`、`virt-install`、`virsh` 已安装，`qemu-desktop`、`qemu-full`、`libvirt`、`edk2-ovmf`、`dnsmasq`、`swtpm` 已安装，OVMF files 存在。

## Approach

1. 采用 Rust + WebKitGTK 6.0 + GTK4 layer-shell，不再保留 C/CMake 或其它 engine 分支。
   - 原生 host 使用 Rust/Cargo，因为本机已有 Rust 1.96.0，用户偏好 Rust，并且已确认可用 safe Rust bindings 覆盖 GTK4、gtk4-layer-shell、WebKitGTK 6.0。
   - Web engine 使用 WebKitGTK 6.0，经 Rust crate `webkit6` 调用；它是 GTK4 web widget，可作为 GTK4 window child 放入 layer-shell window。
   - Layer-shell 使用 Rust crate `gtk4-layer-shell`；docs.rs 显示 0.8.0 依赖 `gtk4 ^0.11`，提供 `LayerShell` trait、`is_supported()`、`Edge`、`Layer`、`KeyboardMode`。
   - 不选 WPE：WPE 官方架构说明它不依赖 UI toolkit，没有 GTK widget；应用必须提供 rendering backend 并转发输入，首期会变成自写渲染/input plumbing。
   - 不选 CEF/Chromium：CEF README 说明 CEF 可嵌入 Chromium 或 off-screen render；CEF `CefRenderHandler` 要求实现 `GetViewRect()`、`OnPaint()`，可选 `OnAcceleratedPaint()` 等，首期会变成手动上传/合成画面和输入转发。
   - 不选 Tauri/wry：wry 当前 `dev` 分支 Cargo.toml 在 Linux 依赖 `webkit2gtk = 2.0.2`、`webkit2gtk-sys = 2.0.2`、`gtk = 0.18`，即 GTK3/webkit2gtk 路线；不能与 GTK4 `gtk4-layer-shell` 组合。
   - 不选 GTK3 layer-shell：RustSec `RUSTSEC-2024-0422` 标记 `gtk-layer-shell` GTK3 bindings 不再维护，并建议使用 `gtk4-layer-shell`。
   - 不选 Electron：Electron 创建自己的 top-level BrowserWindow，不能作为 GTK4 layer-shell window 的 child。

2. 明确无 DE/WM 边界并固定测试策略。
   - `html-desktop-shell` 是 Wayland layer-shell client，不是 compositor；它不直接管理 DRM/KMS/libinput，也不能从裸 TTY 直接显示。
   - 可运行的“无 DE/WM”定义固定为：没有 GNOME/KDE/XFCE 等 desktop environment、没有 display manager、没有当前图形桌面服务；但必须启动一个支持 layer-shell 的 Wayland compositor。Wayland 下 compositor 同时承担 display server/window manager 角色，因此“完全没有 compositor/window manager”不可运行。
   - 当前主机测试 compositor 固定使用已安装的 niri，不额外安装 sway/labwc/cage。理由：niri 已安装、可从 TTY 启动、支持 layer-shell components；cage 是 kiosk compositor，未作为 layer-shell 验证目标；sway/labwc 当前未安装。
   - 首期执行顺序固定为：
     1. 当前 niri 会话内运行项目，验证正常 layer-shell panel。
     2. 写入 tty2/niri 测试配置文件，但不自动切换 VT；README 给出 tty2 手动测试步骤。
     3. 写入 KVM 隔离测试方案；不在首次执行中创建 VM，因为 Arch guest 安装和 ISO 下载是大状态变更，需单独执行。

3. 安装依赖前做只读交易校验；只有交易仍精确等于当前评估才安装。
   - 在任何安装前运行：
     ```bash
     pacman -S --print --needed --print-format '%n %v %s' webkitgtk-6.0
     ```
   - 只有输出恰好为一行 `webkitgtk-6.0 2.52.4-1 37552755` 时继续；这表示本轮执行只新增 `webkitgtk-6.0`，不升级、不删除、不新增其它依赖。
   - 若输出多于一行、包名不是 `webkitgtk-6.0`、版本不是 `2.52.4-1`，或出现 removals/upgrades 提示，停止实现并报告新的交易内容；不得安装。
   - 继续时使用交互式 TTY 刷新 sudo 并安装：
     ```bash
     sudo -v
     sudo pacman -S --needed webkitgtk-6.0
     ```
   - 安装后必须验证：
     ```bash
     pkg-config --modversion webkitgtk-6.0
     ```
     期望输出 `2.52.4`；否则停止，不创建普通 GTK fallback。

4. 创建项目 `~/coding/OtherProjects/html-desktop-shell`。
   - 若执行前该路径突然存在，先读取目录；如果不是本计划创建的项目，改用 `~/coding/OtherProjects/html-desktop-shell-prototype`，并同步替换本计划所有路径、niri test config 中的 `cd "$HOME/coding/OtherProjects/html-desktop-shell"` 路径和 README 命令。
   - 创建目录结构：
     ```text
     html-desktop-shell/
       Cargo.toml
       src/
         main.rs
         shell_window.rs
         bridge.rs
       web/
         index.html
         shell.css
         shell.js
       test/
         niri-tty2-host.kdl
         niri-kvm-guest.kdl
       docs/
         html-desktop-shell-plan.md
       README.md
     ```
   - `Cargo.toml` 固定内容：
     ```toml
     [package]
     name = "html-desktop-shell"
     version = "0.1.0"
     edition = "2024"

     [dependencies]
     gtk4 = "0.11"
     gtk4-layer-shell = "0.8"
     webkit6 = { version = "0.6", features = ["v2_52"] }
     javascriptcore6 = "0.6"
     glib = "0.22"
     ```
   - 不添加 workspace、build.rs、CMake、Meson、Tauri、wry、Electron 或 Node tooling。
   - 创建 `docs/html-desktop-shell-plan.md`，内容为审批后的完整执行计划 Markdown；如果执行前改用 `html-desktop-shell-prototype` 路径，先把计划中的项目路径、niri 配置路径和验证命令同步改成 prototype 路径，再写入该 docs 文件。

5. 实现 application 入口 `src/main.rs`。
   - 定义模块：
     ```rust
     mod bridge;
     mod shell_window;
     ```
   - 定义常量：
     ```rust
     const APP_ID: &str = "dev.ohmypi.HtmlDesktopShell";
     ```
   - `main()` 创建 `gtk4::Application`，连接 `activate`，调用 `shell_window::shell_window_new(app)`。
   - `shell_window_new()` 返回 `Ok(window)` 时调用 `window.present()`。
   - 返回 `Err(message)` 时输出到 stderr 并调用 `app.quit()`；不要 panic。

6. 实现 Wayland layer-shell panel host `src/shell_window.rs`。
   - 暴露精确函数：
     ```rust
     pub fn shell_window_new(app: &gtk4::Application) -> Result<gtk4::ApplicationWindow, String>;
     ```
   - 定义精确常量：
     ```rust
     const PANEL_HEIGHT: i32 = 32;
     const PANEL_NAMESPACE: &str = "html-desktop-shell-panel";
     ```
   - 首先调用 `gtk4_layer_shell::is_supported()`；若为 false，返回错误字符串：
     ```text
     Wayland compositor does not support layer-shell
     ```
   - 创建 `gtk4::ApplicationWindow` 后、窗口 realize 前，按顺序调用：
     ```rust
     use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

     window.init_layer_shell();
     window.set_namespace(Some(PANEL_NAMESPACE));
     window.set_layer(Layer::Top);
     window.set_anchor(Edge::Left, true);
     window.set_anchor(Edge::Right, true);
     window.set_anchor(Edge::Top, true);
     window.set_anchor(Edge::Bottom, false);
     window.set_margin(Edge::Top, 0);
     window.set_exclusive_zone(PANEL_HEIGHT);
     window.set_keyboard_mode(KeyboardMode::OnDemand);
     window.set_default_size(0, PANEL_HEIGHT);
     ```
   - 创建 `webkit6::WebView::new()`，调用 `bridge::attach_bridge(&web_view)`；若返回错误，stderr 输出错误但继续显示 panel。
   - 使用 `glib::filename_to_uri(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/index.html"), None)` 生成 file URI；如果文件不存在，返回 `missing web/index.html: <absolute path>`。
   - 连接 `web_view.connect_load_failed(|_, _event, failing_uri, error| { ...; false })`；stderr 输出 `WebKit load failed for <failing_uri>: <error>` 并返回 `false`。
   - 调用 `web_view.load_uri(&uri)`，然后 `window.set_child(Some(&web_view))`。

7. 实现最小权限桥 `src/bridge.rs`。
   - 暴露精确函数：
     ```rust
     pub fn attach_bridge(web_view: &webkit6::WebView) -> Result<(), &'static str>;
     ```
   - 定义精确常量：
     ```rust
     const HOST_INFO_JSON: &str = r#"{"shell":"html-desktop-shell","backend":"wayland-layer-shell"}"#;
     const HANDLER_NAME: &str = "shell";
     ```
   - `attach_bridge()` 调用 `web_view.user_content_manager()`；若为 `None`，返回 `Err("missing WebKit user content manager")`。
   - 先连接 handler，再注册 handler：
     ```rust
     manager.connect_script_message_with_reply_received(Some(HANDLER_NAME), |_manager, value, reply| {
         let Some(context) = value.context() else {
             reply.return_error_message("missing JavaScriptCore context");
             return true;
         };
         let result = javascriptcore6::Value::new_string(&context, Some(HOST_INFO_JSON));
         reply.return_value(&result);
         true
     });
     ```
   - 然后调用：
     ```rust
     manager.register_script_message_handler_with_reply(HANDLER_NAME, None)
     ```
     若返回 `false`，返回 `Err("failed to register WebKit script message handler: shell")`。
   - 不暴露文件系统、进程启动、DBus、网络、剪贴板、截图、通知、电源/session 控制；首期唯一 native 返回值就是 `HOST_INFO_JSON`。

8. 实现本地 web UI。
   - `web/index.html` 包含三个可观察元素，id 固定：
     ```html
     <div id="app-name">HTML Shell</div>
     <div id="clock">--:--:--</div>
     <div id="bridge-status">bridge: pending</div>
     ```
   - `web/index.html` 引入 `shell.css` 和 `shell.js`，不加载远程资源，不使用 CDN。
   - `web/shell.css` 固定 32px 高度、横向 flex 布局、透明/半透明深色背景；body margin 为 0。
   - `web/shell.js` 每秒更新 `#clock`。
   - `web/shell.js` 定义：
     ```js
     window.shell = {
       async getHostInfo() {
         const raw = await window.webkit.messageHandlers.shell.postMessage({ method: "getHostInfo" });
         return JSON.parse(raw);
       },
     };
     ```
   - 页面启动时调用 `window.shell.getHostInfo()`；成功后把 `#bridge-status` 改为 `bridge: wayland-layer-shell`，失败后改为 `bridge: unavailable`。

9. 添加 tty2 和 KVM 用 niri 配置文件。
   - `test/niri-tty2-host.kdl` 固定内容；本轮已用 `niri validate` 验证同等内容语法有效：
     ```kdl
     spawn-sh-at-startup "cd \"$HOME/coding/OtherProjects/html-desktop-shell\" && ./target/debug/html-desktop-shell"

     binds {
         Mod+Shift+E { quit; }
         Ctrl+Alt+Delete { quit; }
     }
     ```
   - `test/niri-kvm-guest.kdl` 固定内容；用于 Arch KVM guest 内 `~/html-desktop-shell` 路径，同时启动 `foot` 终端便于在 guest 内执行 `wayland-info` 等诊断命令：
     ```kdl
     spawn-sh-at-startup "cd \"$HOME/html-desktop-shell\" && ./target/debug/html-desktop-shell"
     spawn-at-startup "foot"

     binds {
         Mod+Shift+E { quit; }
         Ctrl+Alt+Delete { quit; }
     }
     ```
   - 不使用 `/dev/null` 作为实际测试配置，因为空配置会丢失退出快捷键；只把 `/dev/null` 验证记录作为“niri 能接受最小配置”的事实。

10. 明确不实现 X11 backend，只保留实现锚点。
   - 首期目标是当前 niri Wayland 会话和 tty2 niri 裸会话里的 layer-shell 端到端验证。
   - 不添加 X11 普通窗口 fallback。
   - README 中写明：X11 后端后续必须独立实现 EWMH `_NET_WM_WINDOW_TYPE_DOCK` 与 `_NET_WM_STRUT_PARTIAL`，不能复用 Wayland layer-shell 逻辑伪装完成。

11. 编写 README，并把完整计划保存到 docs。
   - README 只包含：依赖、构建/运行命令、支持矩阵、当前会话验证、tty2 验证、KVM 验证方案。
   - README 记录依赖影响：`webkitgtk-6.0` 当前只新增一个包，下载 35.81 MiB，安装后 130.33 MiB；如果 dry-run 输出变化，停止安装。
   - README 支持矩阵固定：
     - Supported: current niri Wayland session; manual tty2 niri session; any Wayland compositor with `zwlr_layer_shell_v1`.
     - Not supported: raw TTY with no compositor; X11; GNOME Wayland if layer-shell unsupported; Electron-only mode; Qt WebEngine/Quickshell plugin mode.
   - README 必须明确回答：无 DE/display manager 可以；完全没有 Wayland compositor 不可以。
   - `docs/html-desktop-shell-plan.md` 必须保存审批后的完整执行计划 Markdown；README 不重复完整计划，只指向 `docs/html-desktop-shell-plan.md`。
   - 不添加 TODO 注释；未实现项只在 README 当前支持矩阵中说明。

## Critical files & anchors

- `~/coding/OtherProjects/html-desktop-shell/src/shell_window.rs` — layer-shell surface setup, WebKitGTK widget creation, local HTML loading, and no-compositor failure string.
- `~/coding/OtherProjects/html-desktop-shell/src/bridge.rs` — the only native bridge, `attach_bridge()` and `HOST_INFO_JSON`.
- `~/coding/OtherProjects/html-desktop-shell/test/niri-tty2-host.kdl` — host tty2 no-DE/display-manager test entrypoint.
- `~/coding/OtherProjects/html-desktop-shell/test/niri-kvm-guest.kdl` — KVM guest no-DE/display-manager test entrypoint.
- `~/coding/OtherProjects/html-desktop-shell/docs/html-desktop-shell-plan.md` — project-local copy of the approved execution plan; must reflect any prototype path contingency.

## Verification

1. Dependency transaction check.
   - Working directory: `~/coding/OtherProjects`
   - Command:
     ```bash
     pacman -S --print --needed --print-format '%n %v %s' webkitgtk-6.0
     ```
   - Expected output exactly:
     ```text
     webkitgtk-6.0 2.52.4-1 37552755
     ```
   - If output differs, stop before installation.

2. Dependency install and pkg-config check.
   - Commands require TTY because sudo may prompt:
     ```bash
     sudo -v
     sudo pacman -S --needed webkitgtk-6.0
     pkg-config --modversion webkitgtk-6.0
     ```
   - Expected `pkg-config` output:
     ```text
     2.52.4
     ```

3. Build.
   - Working directory: `~/coding/OtherProjects/html-desktop-shell`
   - Command:
     ```bash
     cargo build
     ```
   - Expected: `target/debug/html-desktop-shell` exists and build finishes without errors.

4. Plan persistence check.
   - Working directory: `~/coding/OtherProjects/html-desktop-shell`
   - Commands:
     ```bash
     test -f docs/html-desktop-shell-plan.md
     ```
   - Expected content: the file contains the title `# HTML/CSS/JS Desktop Shell 实施计划`, the no-compositor boundary sentence `不能在“裸 TTY、没有任何 Wayland compositor”的环境运行`, and the tty2 command `niri --session --config ./test/niri-tty2-host.kdl`.

5. Current-session behavior check.
   - Working directory: `~/coding/OtherProjects/html-desktop-shell`
   - Current session should be Wayland/niri; runtime authority is `gtk4_layer_shell::is_supported()`.
   - Command:
     ```bash
     ./target/debug/html-desktop-shell
     ```
   - Expected observable result:
     - screen top shows a 32px panel;
     - left text is `HTML Shell`;
     - center clock updates every second;
     - right text changes from `bridge: pending` to `bridge: wayland-layer-shell`;
     - maximized windows do not cover the top 32px area, proving exclusive zone is active.

6. Host tty2 no-DE/display-manager test.
   - This test is manual and must not be run automatically by the execution agent because it switches VT and may require ending the current graphical session.
   - Precondition: current niri graphical session is ended with `Mod+Shift+E`, or the user accepts that a second compositor may fail to take DRM master while the current session is still running.
   - Manual steps from physical console:
     1. Press `Ctrl+Alt+F2` and log in on tty2.
     2. Run:
        ```bash
        cd ~/coding/OtherProjects/html-desktop-shell
        niri --session --config ./test/niri-tty2-host.kdl
        ```
     3. To exit the test compositor, press `Super+Shift+E` or `Ctrl+Alt+Delete`; these are the only configured quit bindings.
   - Expected result: the same 32px top panel appears in a session with no DE and no display manager; if niri fails before showing the panel, report the niri error and do not treat it as an application failure.
   - Failure case that proves the boundary: running `./target/debug/html-desktop-shell` directly from tty2 without niri must fail with:
     ```text
     Wayland compositor does not support layer-shell
     ```
     or equivalent GTK/Wayland display connection failure; it must not show a fallback normal window.

7. KVM isolated no-DE test design.
   - This design is written to README but not executed in the first implementation pass; creating an Arch guest is a separate large system change.
   - Host prechecks already observed true in this session:
     - `/dev/kvm` exists.
     - current user groups include `kvm` and `libvirt`.
     - `qemu-system-x86_64`, `virt-install`, and `virsh` are installed.
     - `qemu-desktop`, `qemu-full`, `virt-install`, `libvirt`, `edk2-ovmf`, `dnsmasq`, and `swtpm` are installed.
     - OVMF files exist at `/usr/share/edk2/x64/OVMF_CODE.4m.fd` and `/usr/share/edk2/x64/OVMF_VARS.4m.fd`.
   - Use Arch ISO `archlinux-2026.06.01-x86_64.iso` from Arch current release 2026.06.01; expected SHA256 is `ec7a9c89aed7a59a76266ccf723c5e88480e47d7088c4482436f882fa37c3989`.
   - VM creation command template uses user-session libvirt so the VM can read the project under `/home/particleg` without relaxing `/home/particleg` permissions. Run these commands from the host:
     ```bash
     mkdir -p /home/particleg/.local/share/libvirt/images
     virt-install \
       --connect qemu:///session \
       --name html-shell-test \
       --memory 4096 \
       --vcpus 4 \
       --cpu host-passthrough \
       --machine q35 \
       --boot uefi \
       --osinfo archlinux \
       --cdrom /home/particleg/Downloads/archlinux-2026.06.01-x86_64.iso \
       --disk path=/home/particleg/.local/share/libvirt/images/html-shell-test.qcow2,size=40,format=qcow2,bus=virtio \
       --filesystem source.dir=/home/particleg/coding/OtherProjects/html-desktop-shell,target.dir=htmlshell \
       --network type=user,model=virtio \
       --graphics spice,gl.enable=yes,listen=none \
       --video virtio,accel3d=yes \
       --channel spicevmc \
       --controller type=virtio-serial
     ```
   - Inside the guest after minimal Arch install, install only the compositor/runtime/build/diagnostic set:
     ```bash
     sudo pacman -Syu
     sudo pacman -S --needed base-devel rustup pkgconf gtk4 gtk4-layer-shell webkitgtk-6.0 niri seatd wayland-utils mesa noto-fonts ttf-dejavu foot qemu-guest-agent spice-vdagent
     rustup default stable
     sudo systemctl enable --now seatd
     sudo systemctl enable --now qemu-guest-agent
     sudo mkdir -p /mnt/htmlshell
     sudo mount -t 9p -o trans=virtio,version=9p2000.L htmlshell /mnt/htmlshell
     cp -a /mnt/htmlshell "$HOME/html-desktop-shell"
     cd "$HOME/html-desktop-shell"
     cargo build
     niri --config ./test/niri-kvm-guest.kdl
     ```
   - Expected KVM result: the guest boots to TTY, starts niri only, no DE/display manager is installed, and the panel appears with `bridge: wayland-layer-shell`.
   - KVM failure handling:
     - If VM graphics fail with 3D enabled, retry VM creation without `gl.enable=yes` and `accel3d=yes`; record that this tests protocol behavior but not GPU path.
     - If `niri --config ./test/niri-kvm-guest.kdl` starts but the panel reports no layer-shell support, run `wayland-info` in the guest niri session and inspect whether `zwlr_layer_shell_v1` is listed; if absent, stop and report compositor/protocol mismatch.
     - If 9p mount fails, stop the KVM test and report the 9p/libvirt filesystem failure; do not invent a different transfer path in this execution.

## Assumptions & contingencies

- If `pacman -S --print --needed --print-format '%n %v %s' webkitgtk-6.0` no longer outputs exactly `webkitgtk-6.0 2.52.4-1 37552755`, do not install; report the changed transaction and leave project creation unstarted.
- If `cargo build` fails because Rust crate APIs differ from docs.rs 2026-06-11 docs, keep the selected stack and adjust only symbol names to the installed crate versions; do not switch to C, WPE, CEF, wry, Electron, or GTK3.
- If current niri does not support layer-shell at runtime, do not create a fallback window; report that this compositor/session does not satisfy the first-slice protocol requirement.
- If tty2 test is attempted while the current niri session is still active and niri fails to acquire DRM/session control, stop the tty2 test and use the KVM plan instead; do not debug by changing the app into a normal window.
- If `~/coding/OtherProjects/html-desktop-shell` appears before execution and is not this project, use `~/coding/OtherProjects/html-desktop-shell-prototype` and update all commands, README paths, docs plan paths, and both niri config files accordingly.