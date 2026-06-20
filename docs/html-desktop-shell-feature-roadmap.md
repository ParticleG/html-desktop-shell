# HTML Desktop Shell Feature Roadmap

## Current baseline

The previous foundation roadmap is complete. The project now has a stable Wayland layer-shell host with:

- one panel per active monitor at startup;
- full panel rebuild when GDK monitor topology changes;
- no X11, compositor-less TTY, Electron, or normal GTK window fallback;
- runtime config from `--config`, XDG config, or built-in defaults;
- versioned WebKit bridge with `getHostInfo`, `getCapabilities`, and `getState`;
- native state providers for clock, host, and diagnostic niri output state;
- local ES modules under `web/js/` with no frontend build step;
- runtime web asset lookup for development and installed layouts;
- systemd user service, niri snippet, Arch `PKGBUILD`, and current-session smoke script;
- diagnostic CLI flags and measured current-niri/KVM performance gates.

The remaining problem is product value: the current bar is still mostly a foundation/status strip. The next roadmap turns it into a useful desktop panel while preserving the native bridge and compositor boundaries.

## Invariants

Every phase must keep these boundaries intact:

- This app remains a Wayland `zwlr_layer_shell_v1` client, not a compositor.
- No X11, raw TTY, normal-window, Electron, or DE/display-manager fallback.
- Web assets remain local plain HTML/CSS/JS. No bundler, CDN, remote scripts, framework, Node/Bun/npm toolchain, or generated frontend assets.
- Native bridge remains deny-by-default.
- No generic native bridge methods such as `runCommand`, `readFile`, `writeFile`, `dbusCall`, `httpRequest`, `eval`, or generic `niriAction`.
- Any privileged action must have one exact method name, one exact JSON parameter schema, and one explicit UI caller.
- Missing optional environment capabilities must degrade to explicit unavailable state, not prevent panel startup.
- Current niri, tty2 no-DE niri, raw TTY boundary, and KVM smoke remain release gates when a phase changes runtime behavior.

## Phase 8 — Read-only niri workspace and focused-window status

### Goal

Replace the generic right-side provider text with useful read-only session status:

```text
workspaces: 1 2 [3] 4 · Terminal — cargo test
```

This phase adds no native actions. It only exposes state that niri already reports.

### Native changes

1. Extend `NiriProvider` beyond `focused-output`.
   - Continue using subprocesses for this phase:
     ```bash
     niri msg -j focused-output
     niri msg -j workspaces
     niri msg -j focused-window
     ```
   - Do not switch to `niri msg event-stream` yet.
   - If any command fails or returns malformed JSON, keep the panel running and return structured unavailable/error state for that part.

2. Parse only the fields the UI needs.
   - Add minimal Rust structs for niri JSON:
     - workspace id/index/name/output;
     - focused/active state;
     - focused window title/app id;
     - focused output name.
   - Do not pass raw niri JSON through the bridge.
   - Treat missing optional fields as absent; treat schema breaks as provider errors, not panics.

3. Keep bridge surface unchanged.
   - `getState` remains the only state method.
   - `getCapabilities` does not change.
   - No new bridge method is added in Phase 8.

### Web changes

1. Render dedicated right-side widgets.
   - Workspaces with focused workspace highlighted.
   - Focused window title/app id.
   - Compact fallback when niri is unavailable.

2. Keep the panel within the configured height.
   - Use ellipsis for long titles.
   - Keep horizontal full-width behavior.
   - Avoid layout shifts when the title changes every second.

### Tests

- Rust unit tests for parsing:
  - valid `workspaces` JSON;
  - valid `focused-window` JSON;
  - empty focused-window state;
  - malformed JSON/unexpected schema returns unavailable provider state.
- Web smoke with mocked `getState`:
  - focused workspace is visibly marked;
  - focused window text renders;
  - unavailable niri state renders without throwing.

### Manual verification

- Current niri:
  - switching workspaces updates the highlighted workspace;
  - switching windows updates the focused window text;
  - moving focus between `eDP-1` and `DP-2` updates the focused output/workspace relationship.
- tty2 no-DE niri:
  - workspaces and focused window state render without DE services.
- KVM:
  - panel renders workspace/window state, or explicit unavailable state if no focused window exists.

## Phase 9 — Workspace interaction actions

### Goal

Allow mouse interaction with workspace widgets after read-only state is stable.

### Native bridge method

Add one exact action method:

```json
{
  "method": "niriFocusWorkspace",
  "params": { "workspaceId": 3 }
}
```

Rules:

- `workspaceId` must be a positive integer.
- The id must exist in the most recent workspace snapshot.
- If niri is unavailable, return a structured error.
- Only call:
  ```bash
  niri msg action focus-workspace <id>
  ```
- Do not add generic `niriAction`, `runCommand`, or command-string bridge methods.

### Web changes

- Workspace labels become buttons.
- Keyboard focus and ARIA labels are correct.
- Failed action returns a visible, short error state without breaking polling.

### Tests

- Rust tests for valid/invalid params.
- Rust tests that unknown/missing workspace id is rejected.
- Web mocked bridge test for click -> exact method/params.

### Manual verification

- Click each visible workspace and confirm niri switches focus.
- Click invalid/unavailable state is not possible from UI.
- Current niri/tty2/KVM smoke still pass.

## Phase 10 — Basic system status widgets

### Goal

Add useful local system widgets without adding DBus/session-control dependencies.

Recommended order:

1. Battery
   - Read `/sys/class/power_supply`.
   - Show percentage and charging/discharging state.
   - Hide when no battery exists.
   - No UPower/DBus in the first implementation.

2. Network
   - Read `/sys/class/net`.
   - Show wired/wireless up/down summary.
   - Do not use NetworkManager/DBus in the first implementation.

3. CPU/memory diagnostic widget
   - Optional compact local system status.
   - Must be cheap and rate-limited.

### Bridge

- Prefer extending `getState` provider snapshots.
- Do not add action methods.

### Tests

- Provider parsing tests using fixture directories under temp paths.
- No tests depend on the host machine actually having a battery or specific NIC name.

### Manual verification

- Laptop with battery: percentage appears.
- Desktop/KVM without battery: battery widget hides.
- Network changes show up without crashing.

## Phase 11 — Widget layout configuration

### Goal

Make visible widgets configurable after widgets exist.

Example config:

```toml
[widgets]
left = ["app-name", "workspaces"]
center = ["clock"]
right = ["focused-window", "battery", "network"]
```

Rules:

- Defaults preserve the current UI.
- Unknown widget names are config errors, not silently ignored.
- Widget config affects only rendering/layout; it does not grant bridge permissions.

### Tests

- Config defaults.
- Valid widget layout.
- Unknown widget rejection.
- Empty section behavior.

## Phase 12 — Niri event-stream provider

### Goal

Replace repeated `niri msg` polling with an efficient event-stream-backed state cache if measurement shows polling overhead or latency is unacceptable.

### Design constraints

- Use `niri msg event-stream` or niri IPC only in a contained provider module.
- Maintain a last-known snapshot consumed by `getState`.
- Reconnect on failure with bounded retry/backoff.
- If the stream is unavailable, fall back to explicit unavailable state.

### Acceptance

- Lower idle CPU than subprocess polling, or a clear decision to keep polling because measured overhead is already below budget.
- No hangs on compositor restart or niri IPC loss.

## Phase 13 — Packaging/install refinement

### Goal

Move from packaging examples to a cleaner install story after feature widgets stabilize.

Potential work:

- Decide repository license and update `PKGBUILD` license from `LicenseRef-NotProvided`.
- Add a local install command documented in README.
- Add installed default config example under `/usr/share/doc/html-desktop-shell/`.
- Add release archive/package verification commands that were actually executed.

Do not auto-enable services during install or tests.

## Deferred large features

These are intentionally not next:

- System tray / StatusNotifierItem: DBus-heavy and high surface area.
- Notifications: requires notification daemon semantics or integration with an existing daemon.
- Audio controls: PipeWire/WirePlumber/pactl integration should come after simpler system widgets.
- Launcher/search: needs process launching policy and UI focus design.
- Full theming system: wait until widget set and layout settle.

## Recommended immediate next task

Implement Phase 8: read-only niri workspace and focused-window status.

This is the smallest step that makes the bar visibly useful while preserving the current safe architecture: one existing `getState` bridge method, no new action permissions, no DBus, no frontend toolchain, and no compositor fallback.
