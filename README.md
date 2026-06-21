# HTML Desktop Shell

Rust + GTK4 + WebKitGTK 6.0 + GTK4 layer-shell prototype for a top desktop panel implemented with local static web assets.

## Dependencies

Runtime/build dependencies used by this prototype:

- `rustc` / `cargo`
- `pkgconf`
- `gtk4`
- `gtk4-layer-shell`
- `webkitgtk-6.0`
- `bun` for the official Vue/Vite frontend sample build
- A Wayland compositor that exposes `zwlr_layer_shell_v1`

Before installing `webkitgtk-6.0`, verify that the transaction still adds only this package:

```bash
pacman -S --print --needed --print-format '%n %v %s' webkitgtk-6.0
```

Expected output at implementation time:

```text
webkitgtk-6.0 2.52.4-1 37552755
```

This dry run meant one new package only: download size 35.81 MiB, installed size 130.33 MiB. If the output changes, stop and inspect the new transaction before installing.

Install and verify:

```bash
sudo -v
sudo pacman -S --needed webkitgtk-6.0
pkg-config --modversion webkitgtk-6.0
```

Expected `pkg-config` output:

```text
2.52.4
```

## License

Licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE`)
- MIT license (`LICENSE-MIT`)

at your option.

## Build and run

```bash
cd ~/coding/RustroverProjects/html-desktop-shell
cd web
bun install
bun run typecheck
bun test
bun run build
cd ..
cargo build
./target/debug/html-desktop-shell
```

Local user install, without enabling services:

```bash
cd ~/coding/RustroverProjects/html-desktop-shell
cd web
bun install
bun run typecheck
bun test
bun run build
cd ..
cargo build --release --locked
install -Dm755 target/release/html-desktop-shell "$HOME/.local/bin/html-desktop-shell"
mkdir -p "$HOME/.local/share/html-desktop-shell/web"
cp -a web-dist/. "$HOME/.local/share/html-desktop-shell/web/"
install -Dm644 packaging/html-desktop-shell.default.toml "$HOME/.config/html-desktop-shell/config.toml"
```

Then run `html-desktop-shell` from a layer-shell-capable Wayland session. Do not enable the user service until the manual current-session smoke passes.

The binary intentionally has no X11 or normal-window fallback. If the compositor does not support layer-shell, it exits with:

```text
Wayland compositor does not support layer-shell
```

## Support matrix

| Environment | Status | Notes |
| --- | --- | --- |
| Current niri Wayland session | Supported | First local validation target. |
| Manual tty2 niri session | Supported | No DE and no display manager; niri provides the required Wayland compositor and layer-shell protocol. |
| Any Wayland compositor with `zwlr_layer_shell_v1` | Supported | Runtime authority is `gtk4_layer_shell::is_supported()`. |
| Raw TTY with no compositor | Not supported | The app is a Wayland layer-shell client, not a DRM/KMS/libinput compositor. |
| X11 | Not supported | A future X11 backend must independently implement EWMH `_NET_WM_WINDOW_TYPE_DOCK` and `_NET_WM_STRUT_PARTIAL`; Wayland layer-shell logic cannot be reused as a fake X11 dock. |
| GNOME Wayland if layer-shell unsupported | Not supported | Needs compositor support for `zwlr_layer_shell_v1`. |
| Electron-only mode | Not supported | Electron creates its own top-level windows instead of a GTK4 layer-shell child widget. |
| Qt WebEngine / Quickshell plugin mode | Not supported | Not part of this Rust/WebKitGTK layer-shell implementation. |

No DE/display manager is required. A Wayland compositor is required. Completely compositor-less TTY cannot display this app.

## Configuration

Configuration controls native panel shape only. It does not enable behavior plugins, widget layout, or extra native capabilities.

Lookup order:

1. CLI `--config <path>`
2. `$XDG_CONFIG_HOME/html-desktop-shell/config.toml`
3. `~/.config/html-desktop-shell/config.toml`
4. built-in defaults

If a config file exists but is invalid, startup fails instead of silently falling back to defaults. The default config is:

```toml
panel_height = 32
layer = "top"
keyboard_mode = "on-demand"
```

Rust config owns only native panel shape: `panel_height`, `layer`, and `keyboard_mode`. Widget display, position, hiding, and styling are frontend-owned. The official sample layout is `web/src/layout.ts`; custom frontends may change that file or replace the entire static asset directory. A `[widgets]` TOML section is now rejected as an unknown field.

The repository includes `test/panel-default.toml` with the panel defaults for explicit-config verification:

```bash
./target/debug/html-desktop-shell --config ./test/panel-default.toml
```

## Web assets and local integration

`web/` contains the official Vue 3 + Vite + TypeScript sample frontend source. `web-dist/` is generated runtime output and is not committed. Runtime only loads local static assets; it never loads CDN scripts, remote icon APIs, remote fonts, or remote theme assets.

Runtime web asset lookup checks, in order:

1. `$HTML_DESKTOP_SHELL_WEB_DIR/index.html`
2. `$PWD/web-dist/index.html`
3. compile-time manifest `web-dist/index.html`
4. `$XDG_DATA_HOME/html-desktop-shell/web/index.html`
5. `~/.local/share/html-desktop-shell/web/index.html`
6. `/usr/share/html-desktop-shell/web/index.html`

Missing asset errors list every checked path.

### Custom frontend contract

Vue is the official sample frontend, not a core requirement. A custom frontend can use Vue, React, Svelte, Solid, plain HTML, or any other build output if it generates a static directory containing `index.html` and talks to the native bridge.

The stable frontend API is the framework-agnostic `@html-desktop-shell/shell-api` package. It wraps the WebKit message handler:

```js
window.webkit.messageHandlers.shell.postMessage({ id, method, params })
```

Requests and responses must follow the `@html-desktop-shell/shell-api` schema. Rust does not inspect the frontend framework or build tool; it only serves the selected static assets and answers the exact native bridge methods.

### Official sample stack

The bundled sample uses:

- Vue 3 + Vite + TypeScript, built by Bun with a committed `web/bun.lock`.
- PrimeVue styled mode with the Aura preset for local bundled components/theme.
- `@lucide/vue` for tree-shakable local SVG icon components.
- Pinia for `hostInfo`, provider state, bridge/action errors, and refresh state.
- Vue Router with `createMemoryHistory()` so routes never depend on file/browser URL mutation.
- `vue-i18n` Composition API mode with English messages.
- VueUse `useIntervalFn` for the one-second polling lifecycle.

Local integration files are provided but never auto-enabled:

- `packaging/html-desktop-shell.service`: systemd user service for the installed `/usr/bin/html-desktop-shell` binary.
- `packaging/niri-spawn-html-desktop-shell.kdl`: niri startup snippet for an installed `html-desktop-shell` command.
- `packaging/html-desktop-shell.default.toml`: installed default config example for `/usr/share/doc/html-desktop-shell/`.
- `packaging/PKGBUILD`: Arch package recipe that builds the frontend, installs the binary to `/usr/bin/html-desktop-shell`, installs generated web assets to `/usr/share/html-desktop-shell/web`, and installs license files and doc examples.

Packaging and build verification commands for this layout:

```bash
cd web
bun install
bun run typecheck
bun test
bun run build
cd ..
cargo fmt
cargo test
cargo build --release --locked
(cd packaging && makepkg --printsrcinfo)
(cd packaging && makepkg --verifysource)
HTML_DESKTOP_SHELL_BIN=./target/release/html-desktop-shell HTML_DESKTOP_SHELL_WEB_DIR="$PWD/web-dist" scripts/smoke-current-niri.sh
```

The repository also includes `scripts/smoke-current-niri.sh` for the current niri session. It starts the binary, waits, prints `niri msg -j layers`, and terminates the process. It does not switch VTs, create VMs, install packages, enable services, or modify user services.


## Current-session verification

Precondition: current session is Wayland/niri or another layer-shell-capable Wayland compositor.

If another top-layer panel is already running, such as `dms:bar`/dankbar, niri may stack this prototype below that panel. That still proves the layer-shell client is visible; stop the existing panel or use the tty2/KVM clean niri session to verify the prototype at the absolute top edge.

```bash
cd ~/coding/RustroverProjects/html-desktop-shell
cd web
bun install
bun run build
cd ..
cargo build
./target/debug/html-desktop-shell --config ./test/panel-default.toml
```

For machine-readable niri verification, run this while the panel process is running:

```bash
niri msg -j layers
```

Or run the local smoke helper after building:

```bash
HTML_DESKTOP_SHELL_BIN=./target/debug/html-desktop-shell HTML_DESKTOP_SHELL_WEB_DIR="$PWD/web-dist" scripts/smoke-current-niri.sh
```

Expected observable result:

- One 32px panel appears at the top of each detected monitor; with one monitor, this is one panel.
- Left text is `HTML Shell`.
- Center clock updates every second.
- Panel renders clickable niri workspaces for that panel's GDK monitor connector, focused window title/app id, local battery/network widgets, and compact bridge/monitor/niri output status. Without optional data it shows explicit unavailable state or hides battery when no battery exists.
- `niri msg -j layers` shows one top-layer surface per detected monitor with namespace `html-desktop-shell-panel-<index>`, such as `html-desktop-shell-panel-0`.
- Maximized windows do not cover the top 32px area on any panel output, proving the exclusive zone is active.
- Adding or removing monitors after startup triggers a full panel rebuild with the same `html-desktop-shell-panel-<index>` namespace pattern. If rebuild fails, the previous panel set remains running and the error is printed to stderr.

## Native bridge boundary

The only WebKit native message handler is `shell`. Browser code sends versioned JSON requests and receives JSON response envelopes. Supported methods are:

- `getHostInfo`: returns shell name, `wayland-layer-shell` backend, bridge version `2`, and panel context `{ "index": <number>, "output": <string|null> }`.
- `getCapabilities`: returns the supported method list.
- `getState`: returns provider snapshots for clock, host, niri, battery, and network state.
- `niriFocusWorkspace`: accepts `{ "workspaceId": <positive integer> }`, verifies the workspace id exists in the most recent `getState` niri workspace snapshot, then runs only `niri msg action focus-workspace <workspaceId>`.

Unknown or malformed requests return structured errors. Workspace action failures render a short panel error and do not stop polling. The bridge intentionally does not expose filesystem, process, network, DBus, clipboard, screenshot, notification, session-control, generic eval, generic command execution, or generic niri action access.

## Provider state

The official sample polls `getState` once per second through `@html-desktop-shell/shell-api`. The clock comes from the native `ClockProvider`; the browser does not synthesize its own clock for provider state.

State providers:

- `clock`: returns local time as `HH:MM:SS`.
- `host`: returns backend, active monitor count, and bridge version.
- `niri`: when `NIRI_SOCKET` exists, runs `niri msg -j focused-output`, `niri msg -j workspaces`, and `niri msg -j focused-window`. The bridge exposes parsed focused output, workspace id/index/name/output/focus state, and focused window title/app id; it does not pass raw niri JSON through to the browser. Without niri, it returns `{"available":false,"reason":"niri IPC unavailable"}` and does not block panel startup.
- `battery`: reads `/sys/class/power_supply`, averages detected battery percentages, reports charging/discharging/full/not-charging state, and returns `{"available":false,"reason":"no battery"}` when no battery exists.
- `network`: reads `/sys/class/net`, skips loopback, classifies wireless interfaces by the `wireless` sysfs directory and wired interfaces by ARPHRD Ethernet type `1`, then reports wired/wireless up/down counts.

The niri provider intentionally uses the installed `niri msg` command for this phase. Each niri part reports its own `{"available":false,"reason":"..."}` state on command or schema failure, so malformed niri output does not prevent panel startup. This is simple and source-compatible with the current system, but it is a polling diagnostic path, not a low-latency IPC subscription.

`getHostInfo().panel.output` comes from `GdkMonitor::connector()`. The official sample filters workspace buttons to that output, so a panel does not expose workspace buttons for another monitor. Rust does not append panel or widget query parameters to the asset URI.

## Process diagnostics

Diagnostic flags exit without presenting panels unless combined with a normal run command by a wrapper:

```bash
./target/debug/html-desktop-shell --print-capabilities
./target/debug/html-desktop-shell --config ./test/panel-default.toml --print-config
./target/debug/html-desktop-shell --check
```

- `--print-capabilities` prints the same method set as `getCapabilities`.
- `--print-config` prints the effective config after defaults and file parsing.
- `--check` initializes GTK, verifies layer-shell support, resolves web assets, reports monitor count, then exits without opening panels.

## Performance measurements

Measured in the current niri session on 2026-06-20 with two monitors and `./target/debug/html-desktop-shell --config ./test/panel-default.toml`. KVM measurements were provided from a separate no-DE KVM smoke run before the system-widget phase and remain a manual release gate.

| Metric | Current niri measured | Current niri gate | KVM measured | KVM gate |
| --- | ---: | ---: | ---: | ---: |
| Startup until first visible layer surface | 2.701 s | <= 3.38 s | 0.217 s | <= 0.28 s |
| Idle CPU over 60 s while providers update | 0.567% | <= 0.71% | 0.233% | <= 0.30% |
| Resident memory after 60 s | 365.8 MiB | <= 457.3 MiB | 145.9 MiB | <= 183 MiB |

The gates are the measured values plus 25% headroom.

Niri polling decision: 30 measured iterations of `focused-output`, `workspaces`, and `focused-window` took 17.293 ms mean / 19.357 ms max for all three commands. With two panels, the shared niri snapshot cache keeps idle CPU under the current-session gate, so this implementation keeps subprocess polling for now instead of adding an event stream reader. The cache TTL is 500 ms and is invalidated after `niriFocusWorkspace`.

## Renderer diagnostics

Default GTK/WebKit renderer behavior is unchanged. For a software-rendered diagnostic run only:

```bash
GSK_RENDERER=cairo ./target/debug/html-desktop-shell
```

Do not set `GSK_RENDERER` in the niri test configs by default. KVM Mesa/Vulkan warnings remain non-fatal when the panel renders and the bridge reports `wayland-layer-shell`.

## tty2 no-DE/display-manager verification

This test is manual because it switches virtual terminals and may require ending the current graphical session.

Precondition: either end the current niri graphical session with `Mod+Shift+E`, or accept that a second compositor may fail to take DRM/session control while the current session is still running.

From the physical console:

1. Press `Ctrl+Alt+F2` and log in on tty2.
2. Run:

   ```bash
   cd ~/coding/RustroverProjects/html-desktop-shell
   niri --session --config ./test/niri-tty2-host.kdl
   ```

3. Exit the test compositor with `Super+Shift+E` or `Ctrl+Alt+Delete`.

Expected result: one 32px top panel appears on each monitor detected by the tty2 niri session, with no DE and no display manager. If niri fails before showing the panel, report the niri error; that is not an application failure.

Boundary check from tty2: running the app directly without niri must not show a fallback window. It should fail with `Wayland compositor does not support layer-shell` or an equivalent GTK/Wayland display connection error.

## KVM isolated no-DE test design

This is a design for a separate, isolated run. Creating the Arch guest and downloading the ISO are intentionally not part of the first implementation pass.

Observed host preconditions for this machine:

- `/dev/kvm` exists.
- Current user is in `kvm` and `libvirt` groups.
- `qemu-system-x86_64`, `virt-install`, and `virsh` are installed.
- `qemu-desktop`, `qemu-full`, `virt-install`, `libvirt`, `edk2-ovmf`, `dnsmasq`, and `swtpm` are installed.
- OVMF files exist at `/usr/share/edk2/x64/OVMF_CODE.4m.fd` and `/usr/share/edk2/x64/OVMF_VARS.4m.fd`.

Use Arch ISO `archlinux-2026.06.01-x86_64.iso` from Arch release 2026.06.01. Expected SHA256:

```text
ec7a9c89aed7a59a76266ccf723c5e88480e47d7088c4482436f882fa37c3989
```

Create the VM from the host with user-session libvirt so the VM can read the project under `/home/particleg` without relaxing home-directory permissions:

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
  --filesystem source.dir=/home/particleg/coding/RustroverProjects/html-desktop-shell,target.dir=htmlshell \
  --network type=user,model=virtio \
  --graphics spice,gl.enable=yes,listen=none \
  --video virtio,accel3d=yes \
  --channel spicevmc \
  --controller type=virtio-serial \
  --noautoconsole
```

Open the VM console from the current graphical session:

```bash
virt-manager --connect qemu:///session --show-domain-console html-shell-test
```

Inside the guest after a minimal Arch install, install only the compositor/runtime/build/diagnostic set:

```bash
sudo pacman -Syu
sudo pacman -S --needed base-devel rustup pkgconf gtk4 gtk4-layer-shell webkitgtk-6.0 niri seatd wayland-utils mesa noto-fonts ttf-dejavu foot qemu-guest-agent spice-vdagent
rustup default stable
sudo systemctl enable --now seatd
sudo systemctl enable --now qemu-guest-agent
sudo mkdir -p /mnt/htmlshell
sudo mount -t 9p -o trans=virtio,version=9p2000.L htmlshell /mnt/htmlshell
rm -rf "$HOME/html-desktop-shell"
cp -a /mnt/htmlshell "$HOME/html-desktop-shell"
cd "$HOME/html-desktop-shell"
cargo clean
cargo build
niri --config ./test/niri-kvm-guest.kdl
```

Expected KVM result: the guest boots to TTY, starts niri only, no DE/display manager is installed, and one 32px panel appears on each detected monitor with provider-backed status containing `bridge: wayland-layer-shell`. With the default virtio display this normally means one panel, namespace `html-desktop-shell-panel-0`.

In a minimal guest, GTK may print `Cannot get portal org.freedesktop.portal.*` warnings when no `xdg-desktop-portal` service is running. This is expected for the current no-DE test and is not a failure because the app does not use portal-backed features.

Mesa/Vulkan warnings such as `radv/amdgpu: failed to initialize device` or `VK_ERROR_INITIALIZATION_FAILED` mean GTK/WebKit tried a Vulkan path that the VM graphics stack did not provide. If the panel renders and the bridge reports `wayland-layer-shell`, this is not a layer-shell failure. For a software-rendered protocol test, launch with `GSK_RENDERER=cairo ./target/debug/html-desktop-shell`; if graphics do not render at all, recreate the VM without `gl.enable=yes` and `accel3d=yes`.

KVM failure handling:

- If VM graphics fail with 3D enabled, retry VM creation without `gl.enable=yes` and `accel3d=yes`; this tests protocol behavior but not the GPU path.
- If `niri --config ./test/niri-kvm-guest.kdl` starts but the panel reports no layer-shell support, run `wayland-info` in the guest niri session and inspect whether `zwlr_layer_shell_v1` is listed. If absent, stop and report a compositor/protocol mismatch.
- If the 9p mount fails, stop the KVM test and report the 9p/libvirt filesystem failure. Do not invent a different transfer path in this execution.

## Release validation status

Latest completed functional checks:

- Current niri smoke: passed with one panel on each detected monitor.
- Monitor topology rebuild: passed by physically unplugging and reconnecting the external display. The rebuilt panel may appear above dankbar until dankbar recreates its own layer surface; this is compositor stacking behavior with another top-layer client, not an html-desktop-shell fallback path.
- tty2 no-DE niri smoke: passed.
- Raw TTY boundary: passed; running without a compositor exits with GTK display connection failure and opens no fallback window.
- KVM no-DE smoke: passed on a separate machine.
- KVM performance measurement: passed on a separate machine with startup `0.217 s`, idle CPU `0.233%`, and RSS `145.9 MiB`.
