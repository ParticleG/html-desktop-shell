# `packaging/`

## Responsibility

This directory defines the Arch Linux build/install recipe and optional desktop-session startup integration for `html-desktop-shell`.

- `PKGBUILD` builds the Bun/Vite web frontend and the release Rust host, then assembles the Arch package filesystem.
- `html-desktop-shell.service` provides a systemd user unit for starting and restarting the installed host.
- `niri-spawn-html-desktop-shell.kdl` provides an alternative, manually adopted niri startup example.
- `html-desktop-shell.default.toml` is a reference configuration shipped as documentation; the package does not install it as an active user or system configuration.

## Design / Patterns

### Arch package recipe

`PKGBUILD` targets `x86_64` and declares the runtime libraries `gtk4`, `gtk4-layer-shell`, and `webkitgtk-6.0`, with `bun` and `cargo` as build-time dependencies. Its local source entry, `"$pkgname::file://$PWD/.."`, packages the repository containing this directory, while `sha256sums=('SKIP')` reflects that local-source workflow.

The recipe separates work into the conventional makepkg phases:

- `build()` installs the locked frontend dependencies, runs frontend type checking and tests, creates the frontend production build, and then runs `cargo build --release --locked`.
- `package()` copies already-built artifacts and integration examples into their final package paths.

### Startup integration

The two startup files are opt-in alternatives rather than package-install side effects:

- `html-desktop-shell.service` is a systemd user service with `ExecStart=/usr/bin/html-desktop-shell`, `Restart=on-failure`, ordering after `graphical-session.target`, and installation under `default.target`.
- `niri-spawn-html-desktop-shell.kdl` invokes `spawn-at-startup "html-desktop-shell"`. Its comments explicitly require copying the snippet into the user's niri configuration after manual testing; installing the package alone does not activate it.

### Reference configuration

`html-desktop-shell.default.toml` records the packaged example defaults:

- `panel_height = 32`
- `layer = "top"`
- `keyboard_mode = "on-demand"`

It is deliberately installed under `/usr/share/doc`, not `/etc` or a user configuration directory, so activation requires an explicit user copy or adaptation.

## Data & Control Flow

### Build and package flow

1. makepkg resolves the declared GTK/WebKit runtime dependencies and Bun/Cargo build dependencies.
2. `build()` enters `web/` and runs `bun install --frozen-lockfile`, `bun run typecheck`, `bun test`, and `bun run build`.
3. `build()` returns to the repository root and runs `cargo build --release --locked`, producing `target/release/html-desktop-shell`.
4. `package()` installs the native executable as `/usr/bin/html-desktop-shell`.
5. `package()` copies `web-dist/` recursively into `/usr/share/html-desktop-shell/web/`.
6. `package()` installs the systemd user unit at `/usr/lib/systemd/user/html-desktop-shell.service`.
7. The default TOML and niri snippet are installed as `/usr/share/doc/html-desktop-shell/html-desktop-shell.default.toml` and `/usr/share/doc/html-desktop-shell/niri-spawn-html-desktop-shell.kdl`.
8. The repository README is installed under the same documentation directory, while the MIT and Apache-2.0 license texts are installed under `/usr/share/licenses/html-desktop-shell/`.

### Installed startup flow

- **systemd user path:** after the user enables the installed unit, `default.target` pulls it in; systemd executes `/usr/bin/html-desktop-shell` and restarts it only when it fails.
- **niri path:** after the user manually adds the example statement to niri configuration, niri resolves `html-desktop-shell` from `PATH` and launches it at compositor startup.

Neither `package()` nor either example creates or enables an active configuration automatically.

## Integration Points

| Artifact | Build/install/start integration |
| --- | --- |
| `PKGBUILD` | Entry point for Arch `makepkg`; consumes `web/`, the Cargo project, `web-dist/`, repository documentation/licenses, and the other files in this directory. |
| `/usr/bin/html-desktop-shell` | Installed native host executable and the command used by both startup mechanisms. |
| `/usr/share/html-desktop-shell/web/` | Installed frontend asset tree copied from the repository's `web-dist/` build output. |
| `/usr/lib/systemd/user/html-desktop-shell.service` | Discoverable systemd user unit; its `[Install]` section binds enablement to `default.target`. |
| `/usr/share/doc/html-desktop-shell/html-desktop-shell.default.toml` | Inactive reference config containing the panel height, layer, and keyboard-mode defaults. |
| `/usr/share/doc/html-desktop-shell/niri-spawn-html-desktop-shell.kdl` | Inactive niri example that users manually incorporate into compositor configuration. |
