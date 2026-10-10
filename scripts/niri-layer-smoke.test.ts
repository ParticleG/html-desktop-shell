import { describe, expect, test } from "bun:test";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  formatValidation,
  parseLayersJson,
  parseOutputsJson,
  validateNiriLayers,
  type ValidationExpectation,
} from "./niri-layer-smoke";

const output = (name: string, logical: object | null = { x: 0, y: 0, width: 1920, height: 1080, scale: 1 }) => ({
  name,
  current_mode: logical === null ? null : 0,
  logical,
});

const layer = (
  namespace: string,
  outputName: string,
  surfaceLayer = "Top",
  keyboard_interactivity = "OnDemand",
) => ({ namespace, output: outputName, layer: surfaceLayer, keyboard_interactivity });

function validate(
  outputs: unknown,
  layers: unknown,
  monitorCount: number,
  expectation?: ValidationExpectation,
) {
  return validateNiriLayers(
    parseOutputsJson(JSON.stringify(outputs)),
    parseLayersJson(JSON.stringify(layers)),
    monitorCount,
    expectation,
  );
}

describe("niri layer smoke validator", () => {
  test("accepts reordered output maps, ignores disabled outputs and unrelated layers", () => {
    const result = validate(
      {
        "DP-2": output("DP-2"),
        "HDMI-A-1": output("HDMI-A-1", null),
        "eDP-1": output("eDP-1"),
      },
      [
        layer("notifications", "DP-2", "Overlay", "None"),
        layer("html-desktop-shell-panel-1", "eDP-1"),
        layer("html-desktop-shell-panel-0", "DP-2"),
      ],
      2,
    );

    expect(result.ok).toBe(true);
    expect(result.expected.outputs).toEqual(["DP-2", "eDP-1"]);
    expect(formatValidation(result)).toContain("validation passed");
  });

  test("reports missing shell surfaces with expected and observed tuples", () => {
    const result = validate({ "eDP-1": output("eDP-1") }, [], 1);
    const message = formatValidation(result);
    expect(result.ok).toBe(false);
    expect(message).toContain('missing namespace: "html-desktop-shell-panel-0"');
    expect(message).toContain('wrong output: active output "eDP-1" has no shell surface');
    expect(message).toContain('expected namespaces: "html-desktop-shell-panel-0"');
    expect(message).toContain("observed shell tuples");
  });

  test("reports duplicate namespace and duplicate output", () => {
    const result = validate(
      { "eDP-1": output("eDP-1"), "DP-2": output("DP-2") },
      [
        layer("html-desktop-shell-panel-0", "eDP-1"),
        layer("html-desktop-shell-panel-0", "eDP-1"),
      ],
      2,
    );
    expect(result.diagnostics).toContain(
      'duplicate namespace: "html-desktop-shell-panel-0" observed 2 times',
    );
    expect(result.diagnostics).toContain('wrong output: active output "eDP-1" has 2 shell surfaces');
  });

  test("classifies malformed and out-of-range shell namespace names", () => {
    const result = validate(
      { "eDP-1": output("eDP-1") },
      [
        layer("html-desktop-shell-panel-main", "eDP-1"),
        layer("html-desktop-shell-panel-8", "eDP-1"),
      ],
      1,
    );
    expect(result.diagnostics).toContain(
      'malformed namespace: "html-desktop-shell-panel-main" (panel suffix must be a decimal index)',
    );
    expect(result.diagnostics).toContain(
      'wrong-index/extra namespace: "html-desktop-shell-panel-8" (expected indices 0..0)',
    );
  });

  test("reports wrong output, layer, and keyboard policy", () => {
    const result = validate(
      { "eDP-1": output("eDP-1") },
      [layer("html-desktop-shell-panel-0", "DP-9", "Bottom", "Exclusive")],
      1,
    );
    const message = formatValidation(result);
    expect(message).toContain('wrong output: active output "eDP-1" has no shell surface');
    expect(message).toContain('target non-active output "DP-9"');
    expect(message).toContain('wrong layer: "html-desktop-shell-panel-0" on "DP-9" expected "Top", observed "Bottom"');
    expect(message).toContain('wrong keyboard: "html-desktop-shell-panel-0" on "DP-9" expected "OnDemand", observed "Exclusive"');
    expect(message).toContain(
      '("html-desktop-shell-panel-0", "DP-9", "Bottom", "Exclusive")',
    );
  });

  test("rejects malformed outputs and layers JSON", () => {
    expect(() => parseOutputsJson("{"))
      .toThrow("malformed outputs JSON");
    expect(() => parseLayersJson("["))
      .toThrow("malformed layers JSON");
    expect(() => parseLayersJson('{"namespace":"panel"}'))
      .toThrow("malformed layers JSON: expected an array");
    expect(() => parseLayersJson('[{"namespace":"html-desktop-shell-panel-0"}]'))
      .toThrow("requires string output, layer, and keyboard_interactivity");
  });

  test("reports zero active outputs", () => {
    const result = validate({ "HDMI-A-1": output("HDMI-A-1", null) }, [], 1);
    expect(result.diagnostics).toContain("zero active outputs: every niri output has logical=null");
    expect(result.diagnostics).toContain(
      "monitor-count mismatch: binary reported 1, niri reports 0 active output(s)",
    );
  });

  test("reports binary and niri monitor-count mismatch without relying on map order", () => {
    const result = validate(
      { "DP-2": output("DP-2"), "eDP-1": output("eDP-1") },
      [layer("html-desktop-shell-panel-0", "eDP-1")],
      1,
    );
    expect(result.diagnostics).toContain(
      "monitor-count mismatch: binary reported 1, niri reports 2 active output(s)",
    );
    expect(result.diagnostics).toContain('wrong output: active output "DP-2" has no shell surface');
  });
  test.each([
    ["quick-panel", "html-desktop-shell-quick-panel-1", "Overlay", "OnDemand"],
    ["workspace-hud", "html-desktop-shell-workspace-hud-1", "Overlay", "None"],
    ["diagnostics-dialog", "html-desktop-shell-diagnostics-1", "Overlay", "OnDemand"],
  ] as const)("accepts the closed %s namespace and policy profile", (surface, namespace, expectedLayer, keyboard) => {
    const result = validate(
      { "eDP-1": output("eDP-1"), "DP-2": output("DP-2") },
      [
        layer("html-desktop-shell-panel-0", "DP-2"),
        layer(namespace, "eDP-1", expectedLayer, keyboard),
      ],
      2,
      { surface, monitorIndex: 1, output: "eDP-1" },
    );

    expect(result.ok).toBe(true);
    expect(result.expected).toMatchObject({
      surface,
      namespaces: [namespace],
      outputs: ["eDP-1"],
      layer: expectedLayer,
      keyboard,
      absent: false,
    });
    expect(result.observed).toHaveLength(1);
  });

  test("reports missing transient without treating Bar tuples as extras", () => {
    const result = validate(
      { "eDP-1": output("eDP-1") },
      [layer("html-desktop-shell-panel-0", "eDP-1")],
      1,
      { surface: "quick-panel", monitorIndex: 0, output: "eDP-1" },
    );

    expect(result.diagnostics).toEqual([
      'missing transient: "html-desktop-shell-quick-panel-0" on "eDP-1"',
    ]);
  });

  test("reports transient wrong output and index", () => {
    const result = validate(
      { "eDP-1": output("eDP-1") },
      [
        layer("html-desktop-shell-quick-panel-4", "eDP-1", "Overlay"),
        layer("html-desktop-shell-quick-panel-3", "DP-9", "Overlay"),
      ],
      4,
      { surface: "quick-panel", monitorIndex: 3, output: "eDP-1" },
    );

    expect(result.diagnostics).toContain(
      'wrong-index/extra namespace: "html-desktop-shell-quick-panel-4" (expected indices 0..3)',
    );
    expect(result.diagnostics).toContain(
      'wrong output: "html-desktop-shell-quick-panel-3" expected "eDP-1", observed "DP-9"',
    );
  });

  test("reports duplicate and extra transient tuples", () => {
    const result = validate(
      { "eDP-1": output("eDP-1") },
      [
        layer("html-desktop-shell-diagnostics-2", "eDP-1", "Overlay"),
        layer("html-desktop-shell-diagnostics-2", "eDP-1", "Overlay"),
        layer("html-desktop-shell-diagnostics-8", "eDP-1", "Overlay"),
      ],
      3,
      { surface: "diagnostics-dialog", monitorIndex: 2, output: "eDP-1" },
    );

    expect(result.diagnostics).toContain(
      'duplicate transient: "html-desktop-shell-diagnostics-2" on "eDP-1" observed 2 times',
    );
  });

  test.each([
    ["quick-panel", "html-desktop-shell-quick-panel-1", "Bottom", "Exclusive", "Overlay", "OnDemand"],
    ["workspace-hud", "html-desktop-shell-workspace-hud-1", "Top", "OnDemand", "Overlay", "None"],
    ["diagnostics-dialog", "html-desktop-shell-diagnostics-1", "Bottom", "None", "Overlay", "OnDemand"],
  ] as const)(
    "reports the closed %s layer and keyboard policy",
    (surface, namespace, actualLayer, actualKeyboard, expectedLayer, expectedKeyboard) => {
      const result = validate(
        { "eDP-1": output("eDP-1") },
        [layer(namespace, "eDP-1", actualLayer, actualKeyboard)],
        2,
        { surface, monitorIndex: 1, output: "eDP-1" },
      );

      expect(result.diagnostics).toContain(
        `wrong layer: ${JSON.stringify(namespace)} on "eDP-1" expected ${JSON.stringify(expectedLayer)}, observed ${JSON.stringify(actualLayer)}`,
      );
      expect(result.diagnostics).toContain(
        `wrong keyboard: ${JSON.stringify(namespace)} on "eDP-1" expected ${JSON.stringify(expectedKeyboard)}, observed ${JSON.stringify(actualKeyboard)}`,
      );
    },
  );

  test("accepts selected-key disappearance, ignores other active-monitor instances, and reports selected-key leaks", () => {
    const otherMonitor = layer("html-desktop-shell-workspace-hud-1", "DP-2", "Overlay", "None");
    const closed = validate(
      { "eDP-1": output("eDP-1"), "DP-2": output("DP-2") },
      [layer("html-desktop-shell-panel-0", "eDP-1"), otherMonitor],
      2,
      { surface: "workspace-hud", monitorIndex: 0, output: "eDP-1", absent: true },
    );
    expect(closed.ok).toBe(true);
    expect(formatValidation(closed)).toContain("expected surface: workspace-hud (absent)");

    const leaked = validate(
      { "eDP-1": output("eDP-1"), "DP-2": output("DP-2") },
      [otherMonitor, layer("html-desktop-shell-workspace-hud-0", "eDP-1", "Overlay", "None")],
      2,
      { surface: "workspace-hud", monitorIndex: 0, output: "eDP-1", absent: true },
    );
    expect(leaked.diagnostics).toEqual([
      'leaked-after-close: "html-desktop-shell-workspace-hud-0" on "eDP-1" remained visible',
    ]);
  });

  test.each([false, true])("rejects an out-of-range transient monitor index when absent=%s", (absent) => {
    expect(() =>
      validate(
        { "eDP-1": output("eDP-1"), "DP-2": output("DP-2") },
        [],
        2,
        { surface: "quick-panel", monitorIndex: 2, output: "eDP-1", absent },
      ),
    ).toThrow("expected monitor index must be less than monitor count 2, got 2");
  });

  test("allows valid same-kind instances on other active monitors while checking only the selected policy", () => {
    const result = validate(
      { "eDP-1": output("eDP-1"), "DP-2": output("DP-2") },
      [
        layer("html-desktop-shell-quick-panel-0", "eDP-1", "Overlay", "OnDemand"),
        layer("html-desktop-shell-quick-panel-1", "DP-2", "Bottom", "Exclusive"),
      ],
      2,
      { surface: "quick-panel", monitorIndex: 0, output: "eDP-1" },
    );
    expect(result.ok).toBe(true);
    expect(result.observed).toHaveLength(2);
  });

  test("tray menu rejects a second menu on another output without mistaking dismiss layers for menus", () => {
    const outputs = { "DP-2": output("DP-2"), "DP-3": output("DP-3") };
    const selected = layer("html-desktop-shell-tray-menu-0", "DP-2", "Overlay", "Exclusive");
    const dismiss = layer("html-desktop-shell-tray-dismiss-1", "DP-3", "Overlay", "None");
    const expectation = { surface: "tray-menu" as const, monitorIndex: 0, output: "DP-2" };
    expect(validate(outputs, [selected, dismiss], 2, expectation).ok).toBe(true);
    const duplicate = validate(outputs, [
      selected,
      layer("html-desktop-shell-tray-menu-1", "DP-3", "Overlay", "Exclusive"),
    ], 2, expectation);
    expect(duplicate.ok).toBe(false);
    expect(duplicate.diagnostics).toContain("duplicate tray menu: expected one global menu, observed 2");
    const wrongKeyboard = validate(outputs, [
      layer("html-desktop-shell-tray-menu-0", "DP-2", "Overlay", "OnDemand"),
    ], 2, expectation);
    expect(wrongKeyboard.ok).toBe(false);
    expect(validate(outputs, [dismiss], 2, { ...expectation, absent: true }).ok).toBe(true);
  });

  test("CLI selects a transient profile, supports absence, and preserves status classes", () => {
    const root = mkdtempSync(join(tmpdir(), "niri-layer-validator-cli-"));
    const outputsPath = join(root, "outputs.json");
    const layersPath = join(root, "layers.json");
    try {
      writeFileSync(outputsPath, JSON.stringify({ "eDP-1": output("eDP-1") }));
      writeFileSync(
        layersPath,
        JSON.stringify([layer("html-desktop-shell-quick-panel-0", "eDP-1", "Overlay", "OnDemand")]),
      );
      const base = [
        "bun",
        "scripts/niri-layer-smoke.ts",
        "--outputs",
        outputsPath,
        "--layers",
        layersPath,
        "--monitor-count",
        "1",
        "--surface",
        "quick-panel",
        "--monitor-index",
        "0",
        "--output",
        "eDP-1",
      ];

      const present = Bun.spawnSync(base);
      expect(present.exitCode).toBe(0);
      expect(present.stdout.toString()).toContain("validation passed");

      const leaked = Bun.spawnSync([...base, "--absent"]);
      expect(leaked.exitCode).toBe(1);
      expect(leaked.stderr.toString()).toContain("leaked-after-close");
      const outOfRangeArgs = [...base];
      outOfRangeArgs[outOfRangeArgs.indexOf("--monitor-index") + 1] = "1";
      for (const args of [outOfRangeArgs, [...outOfRangeArgs, "--absent"]]) {
        const outOfRange = Bun.spawnSync(args);
        expect(outOfRange.exitCode).toBe(2);
        expect(outOfRange.stderr.toString()).toContain(
          "expected monitor index must be less than monitor count 1, got 1",
        );
      }


      writeFileSync(layersPath, "{");
      const malformed = Bun.spawnSync(base);
      expect(malformed.exitCode).toBe(2);
      expect(malformed.stderr.toString()).toContain("malformed layers JSON");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

});

interface SmokeFixture {
  root: string;
  env: Record<string, string>;
}

function smokeFixture(layersJson: string, appBody = "sleep 30"): SmokeFixture {
  const root = mkdtempSync(join(tmpdir(), "niri-layer-smoke-test-"));
  const app = join(root, "fake-shell");
  const niri = join(root, "niri");
  const bun = join(root, "bun");
  writeFileSync(
    app,
    `#!/usr/bin/env bash
if [[ "\${*: -1}" == "--check" ]]; then
  echo "monitor_count = 1"
  exit 0
fi
${appBody}
`,
  );
  writeFileSync(
    niri,
    `#!/usr/bin/env bash
case "\${*: -1}" in
  outputs) printf '%s\\n' "$OUTPUTS_JSON" ;;
  layers)
    [[ -z "\${NIRI_LAYER_PID_FILE:-}" ]] || printf '%s\\n' "$$" > "$NIRI_LAYER_PID_FILE"
    sleep "\${NIRI_LAYER_DELAY:-0}" &
    delay_pid=$!
    [[ -z "\${NIRI_LAYER_CHILD_PID_FILE:-}" ]] || printf '%s\\n' "$delay_pid" > "$NIRI_LAYER_CHILD_PID_FILE"
    wait "$delay_pid"
    printf '%s\\n' "$LAYERS_JSON"
    ;;
esac
`,
  );
  writeFileSync(
    bun,
    `#!/usr/bin/env bash
[[ -z "\${VALIDATOR_PID_FILE:-}" ]] || printf '%s\\n' "$$" > "$VALIDATOR_PID_FILE"
sleep "\${VALIDATOR_DELAY:-0}" &
delay_pid=$!
[[ -z "\${VALIDATOR_CHILD_PID_FILE:-}" ]] || printf '%s\\n' "$delay_pid" > "$VALIDATOR_CHILD_PID_FILE"
wait "$delay_pid"
exec "$REAL_BUN" "$@"
`,
  );
  chmodSync(app, 0o755);
  chmodSync(niri, 0o755);
  chmodSync(bun, 0o755);
  return {
    root,
    env: {
      ...Object.fromEntries(
        Object.entries(process.env).filter((entry): entry is [string, string] => entry[1] !== undefined),
      ),
      PATH: `${root}:${process.env.PATH ?? ""}`,
      HTML_DESKTOP_SHELL_BIN: app,
      HTML_DESKTOP_SHELL_CONFIG: join(root, "missing-config"),
      REAL_BUN: process.execPath,
      OUTPUTS_JSON: JSON.stringify({ output: { name: "eDP-1", logical: {} } }),
      LAYERS_JSON: layersJson,
    },
  };
}

function runSmoke(fixture: SmokeFixture, env: Record<string, string> = {}) {
  const started = performance.now();
  const result = Bun.spawnSync({
    cmd: ["bash", join(import.meta.dir, "smoke-current-niri.sh")],
    env: { ...fixture.env, ...env },
  });
  return {
    exitCode: result.exitCode,
    stderr: result.stderr.toString(),
    elapsedMilliseconds: performance.now() - started,
  };
}

function terminateAndWaitForFixtureGroup(processGroup: number, childPid: number): void {
  if (Number.isSafeInteger(processGroup) && processGroup >= 2) {
    try {
      process.kill(-processGroup, "SIGTERM");
    } catch {
      // The application cleanup may already have removed the complete group.
    }
    // This is a Linux process-group integration failsafe, not application timer behavior.
    Bun.spawnSync({
      cmd: [
        "bash",
        "-c",
        'for _ in {1..20}; do kill -0 -- "-$PGID" 2>/dev/null || exit 0; sleep 0.01; done',
      ],
      env: { PGID: String(processGroup) },
    });
    try {
      process.kill(-processGroup, "SIGKILL");
    } catch {
      // The TERM wait already removed the complete group.
    }
  }
  if (Number.isSafeInteger(childPid) && childPid > 1) {
    try {
      process.kill(childPid, "SIGKILL");
    } catch {
      // The process-group kill already removed the collector.
    }
    Bun.spawnSync({
      cmd: [
        "bash",
        "-c",
        'for _ in {1..20}; do kill -0 "$CHILD_PID" 2>/dev/null || exit 0; sleep 0.01; done',
      ],
      env: { CHILD_PID: String(childPid) },
    });
  }
}
function processState(pid: number): string | undefined {
  try {
    return readFileSync(`/proc/${pid}/stat`, "utf8").match(/\) ([A-Z]) /)?.[1];
  } catch {
    return undefined;
  }
}



describe("smoke-current-niri helper", () => {
  test("preserves fatal validator status 2 without polling", () => {
    const fixture = smokeFixture("[");
    try {
      const result = runSmoke(fixture, {
        HTML_DESKTOP_SHELL_SMOKE_DELAY: "30",
        HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "5",
      });
      expect(result.exitCode).toBe(2);
      expect(result.stderr).toContain("malformed layers JSON");
      expect(result.elapsedMilliseconds).toBeLessThan(2_000);
    } finally {
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });

  test("caps a long polling delay at the remaining deadline", () => {
    const fixture = smokeFixture("[]");
    try {
      const result = runSmoke(fixture, {
        HTML_DESKTOP_SHELL_SMOKE_DELAY: "30",
        HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "1",
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("timed out after 1s");
      expect(result.elapsedMilliseconds).toBeLessThan(2_500);
    } finally {
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });

  test.each(["08", "08.5"])(
    "treats leading-zero delay %s as decimal when capping to the deadline",
    (delay) => {
      const fixture = smokeFixture("[]");
      try {
        const result = runSmoke(fixture, {
          HTML_DESKTOP_SHELL_SMOKE_DELAY: delay,
          HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "1",
        });
        expect(result.exitCode).toBe(1);
        expect(result.stderr).toContain("timed out after 1s");
        expect(result.stderr).not.toContain("value too great for base");
        expect(result.elapsedMilliseconds).toBeLessThan(2_500);
      } finally {
        rmSync(fixture.root, { recursive: true, force: true });
      }
    },
  );

  test("reports an application that exits during the final capped sleep", () => {
    const fixture = smokeFixture("[]", "sleep 0.2\nexit 7");
    try {
      const result = runSmoke(fixture, {
        HTML_DESKTOP_SHELL_SMOKE_DELAY: "30",
        HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "1",
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain(
        "application exited before layer assertions passed (status 7)",
      );
      expect(result.stderr).not.toContain("timed out");
    } finally {
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });

  test("bounds a stalled layers query, reaps it, and cleans up the application group", () => {
    const fixture = smokeFixture(
      JSON.stringify([layer("html-desktop-shell-panel-0", "eDP-1")]),
      `bash -c 'trap \"\" TERM; while :; do sleep 1; done' &
child=$!
pgid=$(ps -o pgid= -p "$child")
printf '%s %s\\n' "$child" "$pgid" > "$CHILD_PID_FILE"
wait`,
    );
    const childPidFile = join(fixture.root, "child.pid");
    const layersPidFile = join(fixture.root, "layers.pid");
    const layersChildPidFile = join(fixture.root, "layers-child.pid");
    let childPid = 0;
    let processGroup = 0;
    try {
      const result = runSmoke(fixture, {
        CHILD_PID_FILE: childPidFile,
        NIRI_LAYER_PID_FILE: layersPidFile,
        NIRI_LAYER_DELAY: "30",
        NIRI_LAYER_CHILD_PID_FILE: layersChildPidFile,
        HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "1",
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("niri layers query timed out at the absolute smoke deadline");
      expect(result.elapsedMilliseconds).toBeLessThan(2_500);
      [childPid, processGroup] = readFileSync(childPidFile, "utf8").trim().split(/\s+/).map(Number);
      expect(["Z", undefined]).toContain(processState(childPid));
      const layersPid = Number(readFileSync(layersPidFile, "utf8").trim());
      expect(["Z", undefined]).toContain(processState(layersPid));
      const layersChildPid = Number(readFileSync(layersChildPidFile, "utf8").trim());
      expect(["Z", undefined]).toContain(processState(layersChildPid));
    } finally {
      if (existsSync(childPidFile) && (childPid === 0 || processGroup === 0)) {
        [childPid, processGroup] = readFileSync(childPidFile, "utf8").trim().split(/\s+/).map(Number);
      }
      terminateAndWaitForFixtureGroup(processGroup, childPid);
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });

  test("bounds and reaps a stalled validator", () => {
    const fixture = smokeFixture(JSON.stringify([layer("html-desktop-shell-panel-0", "eDP-1")]));
    const validatorPidFile = join(fixture.root, "validator.pid");
    const validatorChildPidFile = join(fixture.root, "validator-child.pid");
    try {
      const result = runSmoke(fixture, {
        VALIDATOR_PID_FILE: validatorPidFile,
        VALIDATOR_CHILD_PID_FILE: validatorChildPidFile,
        VALIDATOR_DELAY: "30",
        HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "1",
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("validator timed out at the absolute smoke deadline");
      expect(result.elapsedMilliseconds).toBeLessThan(2_500);
      const validatorPid = Number(readFileSync(validatorPidFile, "utf8").trim());
      expect(["Z", undefined]).toContain(processState(validatorPid));
      const validatorChildPid = Number(readFileSync(validatorChildPidFile, "utf8").trim());
      expect(["Z", undefined]).toContain(processState(validatorChildPid));
    } finally {
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });

  test("rejects a validator pass returned after the absolute deadline", () => {
    const fixture = smokeFixture(JSON.stringify([layer("html-desktop-shell-panel-0", "eDP-1")]));
    const fakeTimeout = join(fixture.root, "timeout");
    writeFileSync(
      fakeTimeout,
      `#!/usr/bin/env bash
shift 3
exec "$@"
`,
    );
    chmodSync(fakeTimeout, 0o755);
    try {
      const result = runSmoke(fixture, {
        VALIDATOR_DELAY: "1.1",
        HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "1",
      });
      expect(result.exitCode).toBe(1);
      expect(result.stderr).toContain("validator passed after the absolute smoke deadline");
      expect(result.elapsedMilliseconds).toBeLessThan(2_500);
    } finally {
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });

  test("kills the application process group including a collector child", () => {
    const fixture = smokeFixture(
      JSON.stringify([layer("html-desktop-shell-panel-0", "eDP-1")]),
      `bash -c 'trap \"\" TERM; while :; do sleep 1; done' &
child=$!
pgid=$(ps -o pgid= -p "$child")
printf '%s %s\\n' "$child" "$pgid" > "$CHILD_PID_FILE"
wait`,
    );
    const childPidFile = join(fixture.root, "child.pid");
    let childPid = 0;
    let processGroup = 0;
    try {
      const result = runSmoke(fixture, {
        CHILD_PID_FILE: childPidFile,
        NIRI_LAYER_DELAY: "0.1",
        HTML_DESKTOP_SHELL_SMOKE_DELAY: "0.01",
        HTML_DESKTOP_SHELL_SMOKE_TIMEOUT: "2",
      });
      expect(result.exitCode).toBe(0);
      [childPid, processGroup] = readFileSync(childPidFile, "utf8")
        .trim()
        .split(/\s+/)
        .map(Number);
      let childState: string | undefined;
      try {
        const stat = readFileSync(`/proc/${childPid}/stat`, "utf8");
        childState = stat.match(/\) ([A-Z]) /)?.[1];
      } catch {
        childState = undefined;
      }
      // This integration assertion checks OS process-group cleanup directly; a reaping
      // zombie is terminated, while any runnable/sleeping state is a leaked collector.
      expect(childState === undefined || childState === "Z").toBe(true);
    } finally {
      if (existsSync(childPidFile) && (childPid === 0 || processGroup === 0)) {
        [childPid, processGroup] = readFileSync(childPidFile, "utf8")
          .trim()
          .split(/\s+/)
          .map(Number);
      }
      terminateAndWaitForFixtureGroup(processGroup, childPid);
      rmSync(fixture.root, { recursive: true, force: true });
    }
  });
});
