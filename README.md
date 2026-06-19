# HTML Desktop Shell

Rust + GTK4 + WebKitGTK 6.0 + GTK4 layer-shell prototype for a top desktop panel implemented with local HTML/CSS/JS.

## Dependencies

Runtime/build dependencies used by this prototype:

- `rustc` / `cargo`
- `pkgconf`
- `gtk4`
- `gtk4-layer-shell`
- `webkitgtk-6.0`
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

## Build and run

```bash
cd ~/coding/RustroverProjects/html-desktop-shell
cargo build
./target/debug/html-desktop-shell
```

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

Configuration controls panel shape only. It does not enable behavior plugins or extra native capabilities.

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

The repository includes `test/panel-default.toml` with those defaults for explicit-config verification:

```bash
./target/debug/html-desktop-shell --config ./test/panel-default.toml
```

## Web assets and local integration

Runtime web asset lookup checks, in order:

1. `$HTML_DESKTOP_SHELL_WEB_DIR/index.html`
2. `$PWD/web/index.html`
3. compile-time manifest `web/index.html`
4. `/usr/share/html-desktop-shell/web/index.html`

Missing asset errors list every checked path.

Local integration files are provided but never auto-installed:

- `packaging/html-desktop-shell.service`: systemd user service example using `%h/.local/bin/html-desktop-shell`.
- `packaging/niri-spawn-html-desktop-shell.kdl`: niri startup snippet for a development checkout; replace the command with your installed binary path.
- `packaging/PKGBUILD`: Arch package recipe that installs the binary to `/usr/bin/html-desktop-shell` and web assets to `/usr/share/html-desktop-shell/web`.

The repository also includes `scripts/smoke-current-niri.sh` for the current niri session. It starts the binary, waits, prints `niri msg -j layers`, and terminates the process. It does not switch VTs, create VMs, install packages, enable services, or modify user services.


## Current-session verification

Precondition: current session is Wayland/niri or another layer-shell-capable Wayland compositor.

If another top-layer panel is already running, such as `dms:bar`/dankbar, niri may stack this prototype below that panel. That still proves the layer-shell client is visible; stop the existing panel or use the tty2/KVM clean niri session to verify the prototype at the absolute top edge.

```bash
cd ~/coding/RustroverProjects/html-desktop-shell
cargo build
./target/debug/html-desktop-shell --config ./test/panel-default.toml
```

For machine-readable niri verification, run this while the panel process is running:

```bash
niri msg -j layers
```

Or run the local smoke helper after building:

```bash
scripts/smoke-current-niri.sh
```

Expected observable result:

- One 32px panel appears at the top of each detected monitor; with one monitor, this is one panel.
- Left text is `HTML Shell`.
- Center clock updates every second.
- Right text changes from `bridge: pending` to provider-backed status containing `bridge: wayland-layer-shell`, monitor count, and niri availability/focused output when available.
- `niri msg -j layers` shows one top-layer surface per detected monitor with namespace `html-desktop-shell-panel-<index>`, such as `html-desktop-shell-panel-0`.
- Maximized windows do not cover the top 32px area on any panel output, proving the exclusive zone is active.
- Adding or removing monitors after startup triggers a full panel rebuild with the same `html-desktop-shell-panel-<index>` namespace pattern. If rebuild fails, the previous panel set remains running and the error is printed to stderr.

## Native bridge boundary

The only WebKit native message handler is `shell`. Browser code sends versioned JSON requests and receives JSON response envelopes. Supported methods are:

- `getHostInfo`: returns shell name, `wayland-layer-shell` backend, and bridge version `1`.
- `getCapabilities`: returns the supported method list.
- `getState`: returns provider snapshots for clock, host, and optional niri state.

Unknown or malformed requests return structured errors. The bridge intentionally does not expose filesystem, process, network, DBus, clipboard, screenshot, notification, session-control, or generic eval access.

## Provider state

The web UI polls `getState` once per second. The clock now comes from the native `ClockProvider`; the browser no longer uses its own `Date` clock.

State providers:

- `clock`: returns local time as `HH:MM:SS`.
- `host`: returns backend, active monitor count, and bridge version.
- `niri`: when `NIRI_SOCKET` exists, runs `niri msg -j focused-output` as a diagnostic provider and reports the focused output name. Without niri, it returns `{"available":false,"reason":"niri IPC unavailable"}` and does not block panel startup.

The niri provider intentionally uses the installed `niri msg` command for this phase. This is simple and source-compatible with the current system, but it is a polling diagnostic path, not a low-latency IPC subscription.

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

Measured in the current niri session on 2026-06-19 with two monitors and `./target/debug/html-desktop-shell --config ./test/panel-default.toml`:

| Metric | Measured | Current niri gate |
| --- | ---: | ---: |
| Startup until first visible layer surface | 2.922 s | <= 3.7 s |
| Idle CPU over 60 s while provider clock updates | 0.45% | <= 0.6% |
| Resident memory after 60 s | 346.8 MiB | <= 434 MiB |

The gates are the measured values plus 25% headroom. KVM performance gates remain pending until the KVM smoke path is measured.

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
