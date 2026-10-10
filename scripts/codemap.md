# `scripts/`

## Responsibility

This directory contains the Niri layer-shell smoke validator, its focused tests, and a live-session launcher for a locally built `html-desktop-shell`:

- `niri-layer-smoke.ts` parses compositor snapshots and validates one typed surface profile.
- `smoke-current-niri.sh` launches the host and repeatedly invokes the validator's default Bar profile under one bounded deadline.
- `niri-layer-smoke.test.ts` covers validator and launcher behavior.

These files describe per-invocation checks; this map records no standing result.

## Typed Surface Profiles

`SURFACE_PROFILES` defines five accepted CLI values and tuple policies:

| CLI profile | Namespace prefix | Layer | Keyboard |
| --- | --- | --- | --- |
| `bar` | `html-desktop-shell-panel-` | `Top` | `OnDemand` |
| `quick-panel` | `html-desktop-shell-quick-panel-` | `Overlay` | `OnDemand` |
| `workspace-hud` | `html-desktop-shell-workspace-hud-` | `Overlay` | `None` |
| `diagnostics-dialog` | `html-desktop-shell-diagnostics-` | `Overlay` | `OnDemand` |
| `tray-menu` | `html-desktop-shell-tray-menu-` | `Overlay` | `Exclusive` |

Only typed content prefixes enter tuple validation. The `tray-menu` profile
detects only the Quick Panel menu's `html-desktop-shell-tray-menu-` layer
namespace: a Bar tray menu is a native parent-relative `GtkPopover`/`xdg_popup`
with no layer namespace, so it never appears in a layer inventory and tuple
inspection can neither confirm nor deny it. A missing namespace, and any
`--absent` result, is not a Bar popup presence pass. Native-only
`html-desktop-shell-tray-dismiss-` layers are distinct and require separate
runtime input/cleanup observation. Menu validation rejects multiple global menus;
bare namespace tuples do not prove an actual popup, grab, focus, geometry, click
capture, or consumed input.

### Bar semantics

Without `--surface`, or with `--surface bar`, the validator expects:

- a positive `--monitor-count`;
- active-output count equal to that monitor count;
- exactly the indexed namespaces `html-desktop-shell-panel-0..N-1`;
- exactly one Bar tuple on each active output and no Bar tuple on an inactive output;
- `Top`/`OnDemand` policy on every observed Bar tuple.

It diagnoses zero active outputs, monitor-count mismatches, missing or duplicate namespaces, malformed or out-of-range suffixes, missing/duplicate/wrong outputs, and policy mismatches.

### Selected transient and absence semantics

`quick-panel`, `workspace-hud`, and `diagnostics-dialog` require both `--monitor-index <N>` and `--output <NAME>`. The monitor index must be non-negative and less than the reported monitor count; output must be non-empty.

For a present transient, validation selects `${profile.namespacePrefix}${monitorIndex}` on the requested output and requires exactly one matching tuple with the profile's layer/keyboard policy. It also diagnoses an inactive requested output, the selected namespace appearing on another output, malformed or out-of-range same-profile namespaces, and same-profile tuples on inactive outputs. Other valid-index transient surfaces may coexist; the check is intentionally about the selected transient rather than a global one-per-output matrix.

With `--absent`, the validator checks only that the selected namespace/output key has no tuple. A matching tuple is reported as `leaked-after-close`. Same-profile tuples with another index or output do not violate this selected absence check, and tuple policies are irrelevant when the selected tuple is absent. `--absent` is invalid for `bar`.

## Validator CLI and Status Contract

```text
bun scripts/niri-layer-smoke.ts \
  --outputs <outputs.json> \
  --layers <layers.json> \
  --monitor-count <N> \
  [--surface <bar|quick-panel|workspace-hud|diagnostics-dialog> \
   [--monitor-index <N> --output <NAME> [--absent]]]
```

- Status `0`: the supplied snapshots satisfy the selected expectation.
- Status `1`: parsed inputs produced assertion diagnostics; callers may retry with a newer layer snapshot.
- Status `2`: usage, file reading, JSON shape, or other fatal processing failed.

Formatted output records the selected profile, expected namespaces and outputs, tuple policy or selected absence rule, and observed tuples. It does not inspect rendered pixels, placement geometry, exclusive zones, or input-region shape.

## Current Launcher Flow

`smoke-current-niri.sh` accepts no positional arguments and currently exercises the validator's default Bar profile only:

1. Resolve the repository root, binary, optional config, validator, poll interval, and deadline values.
2. Require the binary and validator plus `niri`, `bun`, `setsid`, and `timeout`; validate timeout as a positive integer and delay as a non-negative decimal.
3. Run the binary with optional `--config` and `--check`, then parse a positive `monitor_count`.
4. Capture `niri msg -j outputs` once before launch.
5. Create a private temporary directory and launch the same binary/config through `setsid --wait`, making its PID the process-group ID and capturing application output.
6. Establish the absolute microsecond deadline after launch.
7. While time remains, reject early application exit, run `niri msg -j layers` within the remaining deadline, then run the Bun validator within the same remaining deadline using no transient profile arguments.
8. Return status `0` only after a current validator status `0`, an unexpired absolute deadline, and a final application-liveness check. Retry validator status `1`; propagate fatal status `2`; classify timeout/kill statuses `124` and `137` as deadline expiration.
9. Cap the polling sleep to the remaining coarse deadline. On expiration, recheck process liveness and print the final diagnostics and captured application output.
10. Invoke cleanup on every exit path.

The typed transient CLI is available for separately captured snapshots, but this launcher does not open/close transients or pass `--surface`, `--monitor-index`, `--output`, or `--absent`.

## Inputs

| Input | Default | Effect |
| --- | --- | --- |
| `HTML_DESKTOP_SHELL_BIN` | `$project_root/target/debug/html-desktop-shell` | Selects the executable used for `--check` and launch. |
| `HTML_DESKTOP_SHELL_CONFIG` | `$project_root/test/panel-default.toml` | Adds `--config` to both invocations only when this path is a regular file. |
| `HTML_DESKTOP_SHELL_SMOKE_DELAY` | `0.25` | Sets the non-negative interval between retryable snapshots, capped by remaining time. |
| `HTML_DESKTOP_SHELL_SMOKE_TIMEOUT` | `15` | Sets the positive-integer total post-launch assertion deadline in seconds. |
| Current compositor session | none | Supplies `niri msg -j outputs` and `niri msg -j layers`. |

## Deadline and Process Cleanup

- `remaining_timeout()` derives subcommand duration from one absolute `deadline_us`; `timeout --signal=TERM --kill-after=0.1s` bounds each post-launch layer query and validator invocation.
- A separate coarse `SECONDS` deadline bounds the loop and sleep, while `deadline_expired()` prevents accepting a validator result completed after the microsecond deadline.
- `fail_if_app_exited()` reports captured output and the reaped status when the host exits before validation completes.
- The EXIT trap sends `TERM` to the entire process group, waits up to twenty 50 ms intervals, sends `KILL` if descendants remain, reaps the leader, and removes the temporary directory.
- INT and TERM traps select statuses `130` and `143`; the EXIT trap still performs group cleanup.

## Integration Points and Scope

- **Local host:** the launcher uses an existing executable and never builds or installs it.
- **Native check mode:** `--check` provides the monitor count used by Bar validation.
- **Niri:** JSON output and layer inventories provide active-output and surface tuples.
- **Bun:** runs the TypeScript validator and its tests.
- **Typed native surfaces:** profile names and tuple policies correspond to Bar, Quick Panel, Workspace HUD, and Diagnostics Dialog layer-shell surfaces.
- **Residual manual scope:** appearance, geometry, exclusive reservation, compositor-confirmed HUD timing, and empty GDK input-region behavior are outside this structural snapshot validator.
