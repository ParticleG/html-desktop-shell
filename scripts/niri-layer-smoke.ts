// Composable layer-tuple profiles for the typed content surfaces. The `tray-menu`
// profile matches only the Quick Panel menu's layer namespace: a Bar tray menu is a
// native parent-relative popup with no layer namespace, so it never appears in a
// layer inventory and cannot be confirmed or denied here.
export const SURFACE_PROFILES = {
  bar: {
    namespacePrefix: "html-desktop-shell-panel-",
    layer: "Top",
    keyboard: "OnDemand",
  },
  "quick-panel": {
    namespacePrefix: "html-desktop-shell-quick-panel-",
    layer: "Overlay",
    keyboard: "OnDemand",
  },
  "workspace-hud": {
    namespacePrefix: "html-desktop-shell-workspace-hud-",
    layer: "Overlay",
    keyboard: "None",
  },
  "diagnostics-dialog": {
    namespacePrefix: "html-desktop-shell-diagnostics-",
    layer: "Overlay",
    keyboard: "OnDemand",
  },
  "tray-menu": {
    namespacePrefix: "html-desktop-shell-tray-menu-",
    layer: "Overlay",
    keyboard: "Exclusive",
  },
} as const;

export type SurfaceProfile = keyof typeof SURFACE_PROFILES;

export type ValidationExpectation =
  | { surface?: "bar" }
  | {
      surface: Exclude<SurfaceProfile, "bar">;
      monitorIndex: number;
      output: string;
      absent?: boolean;
    };

const SURFACE_PROFILE_NAMES = Object.keys(SURFACE_PROFILES) as SurfaceProfile[];

type JsonObject = Record<string, unknown>;

export interface ActiveOutputs {
  names: string[];
}

export interface LayerTuple {
  namespace: string;
  output: string;
  layer: string;
  keyboard: string;
}

export interface ValidationResult {
  ok: boolean;
  diagnostics: string[];
  expected: {
    monitorCount: number;
    surface: SurfaceProfile;
    namespaces: string[];
    outputs: string[];
    layer: string;
    keyboard: string;
    absent: boolean;
  };
  observed: LayerTuple[];
}

function object(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function parseOutputsJson(source: string): ActiveOutputs {
  let value: unknown;
  try {
    value = JSON.parse(source);
  } catch (error) {
    throw new Error(`malformed outputs JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!object(value)) {
    throw new Error("malformed outputs JSON: expected an object map");
  }

  const names: string[] = [];
  for (const [mapName, rawOutput] of Object.entries(value)) {
    if (!object(rawOutput)) {
      throw new Error(`malformed outputs JSON: output ${JSON.stringify(mapName)} must be an object`);
    }
    const name = rawOutput.name;
    if (typeof name !== "string" || name.length === 0) {
      throw new Error(`malformed outputs JSON: output ${JSON.stringify(mapName)} has no string name`);
    }
    if (rawOutput.logical !== null) {
      if (!object(rawOutput.logical)) {
        throw new Error(`malformed outputs JSON: output ${JSON.stringify(name)} logical must be an object or null`);
      }
      names.push(name);
    }
  }

  const duplicateNames = names.filter((name, index) => names.indexOf(name) !== index);
  if (duplicateNames.length > 0) {
    throw new Error(`malformed outputs JSON: duplicate active output name(s): ${[...new Set(duplicateNames)].join(", ")}`);
  }
  return { names: names.sort() };
}

export function parseLayersJson(source: string): LayerTuple[] {
  let value: unknown;
  try {
    value = JSON.parse(source);
  } catch (error) {
    throw new Error(`malformed layers JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
  if (!Array.isArray(value)) {
    throw new Error("malformed layers JSON: expected an array");
  }

  const shellLayers: LayerTuple[] = [];
  value.forEach((rawLayer, index) => {
    if (!object(rawLayer)) {
      throw new Error(`malformed layers JSON: entry ${index} must be an object`);
    }
    const namespace = rawLayer.namespace;
    if (typeof namespace !== "string") {
      throw new Error(`malformed layers JSON: entry ${index} has no string namespace`);
    }
    if (!SURFACE_PROFILE_NAMES.some((profile) => namespace.startsWith(SURFACE_PROFILES[profile].namespacePrefix))) return;

    const output = rawLayer.output;
    const layer = rawLayer.layer;
    const keyboard = rawLayer.keyboard_interactivity;
    if (typeof output !== "string" || typeof layer !== "string" || typeof keyboard !== "string") {
      throw new Error(
        `malformed layers JSON: shell surface ${JSON.stringify(namespace)} requires string output, layer, and keyboard_interactivity`,
      );
    }
    shellLayers.push({ namespace, output, layer, keyboard });
  });
  return shellLayers;
}

function counts(values: string[]): Map<string, number> {
  const result = new Map<string, number>();
  for (const value of values) result.set(value, (result.get(value) ?? 0) + 1);
  return result;
}

function quoted(values: string[]): string {
  return values.length === 0 ? "(none)" : values.map((value) => JSON.stringify(value)).join(", ");
}

export function validateNiriLayers(
  outputs: ActiveOutputs,
  layers: LayerTuple[],
  monitorCount: number,
  expectation: ValidationExpectation = {},
): ValidationResult {
  if (!Number.isSafeInteger(monitorCount) || monitorCount < 1) {
    throw new Error(`expected monitor count must be a positive integer, got ${JSON.stringify(monitorCount)}`);
  }

  const surface = expectation.surface ?? "bar";
  const profile = SURFACE_PROFILES[surface];
  const profileLayers = layers.filter((candidate) => candidate.namespace.startsWith(profile.namespacePrefix));
  const diagnostics: string[] = [];

  if (surface === "bar") {
    const expectedNamespaces = Array.from(
      { length: monitorCount },
      (_, index) => `${profile.namespacePrefix}${index}`,
    );
    const expectedNamespaceSet = new Set(expectedNamespaces);
    const activeOutputSet = new Set(outputs.names);

    if (outputs.names.length === 0) diagnostics.push("zero active outputs: every niri output has logical=null");
    if (outputs.names.length !== monitorCount) {
      diagnostics.push(
        `monitor-count mismatch: binary reported ${monitorCount}, niri reports ${outputs.names.length} active output(s)`,
      );
    }

    const namespaceCounts = counts(profileLayers.map((candidate) => candidate.namespace));
    for (const namespace of expectedNamespaces) {
      const count = namespaceCounts.get(namespace) ?? 0;
      if (count === 0) diagnostics.push(`missing namespace: ${JSON.stringify(namespace)}`);
      if (count > 1) diagnostics.push(`duplicate namespace: ${JSON.stringify(namespace)} observed ${count} times`);
    }
    for (const [namespace, count] of namespaceCounts) {
      if (expectedNamespaceSet.has(namespace)) continue;
      const suffix = namespace.slice(profile.namespacePrefix.length);
      if (!/^\d+$/.test(suffix)) {
        diagnostics.push(`malformed namespace: ${JSON.stringify(namespace)} (panel suffix must be a decimal index)`);
      } else {
        diagnostics.push(
          `wrong-index/extra namespace: ${JSON.stringify(namespace)} (expected indices 0..${monitorCount - 1})${count > 1 ? `, observed ${count} times` : ""}`,
        );
      }
    }

    const outputCounts = counts(profileLayers.map((candidate) => candidate.output));
    for (const output of outputs.names) {
      const count = outputCounts.get(output) ?? 0;
      if (count === 0) diagnostics.push(`wrong output: active output ${JSON.stringify(output)} has no shell surface`);
      if (count > 1) diagnostics.push(`wrong output: active output ${JSON.stringify(output)} has ${count} shell surfaces`);
    }
    for (const [output, count] of outputCounts) {
      if (!activeOutputSet.has(output)) {
        diagnostics.push(`wrong output: ${count} shell surface(s) target non-active output ${JSON.stringify(output)}`);
      }
    }

    validatePolicies(profileLayers, profile.layer, profile.keyboard, diagnostics);
    return validationResult(
      diagnostics,
      monitorCount,
      surface,
      expectedNamespaces,
      outputs.names,
      profile.layer,
      profile.keyboard,
      false,
      profileLayers,
    );
  }

  const { monitorIndex, output, absent = false } = expectation;
  if (!Number.isSafeInteger(monitorIndex) || monitorIndex < 0) {
    throw new Error(`expected monitor index must be a non-negative integer, got ${JSON.stringify(monitorIndex)}`);
  }
  if (monitorIndex >= monitorCount) {
    throw new Error(
      `expected monitor index must be less than monitor count ${monitorCount}, got ${JSON.stringify(monitorIndex)}`,
    );
  }
  if (output.length === 0) throw new Error("expected output must be a non-empty string");
  const expectedNamespace = `${profile.namespacePrefix}${monitorIndex}`;

  const activeOutputSet = new Set(outputs.names);
  const selectedNamespaceLayers = profileLayers.filter(
    (candidate) => candidate.namespace === expectedNamespace,
  );
  const exact = selectedNamespaceLayers.filter((candidate) => candidate.output === output);

  if (absent) {
    for (const candidate of exact) {
      diagnostics.push(
        `leaked-after-close: ${JSON.stringify(candidate.namespace)} on ${JSON.stringify(candidate.output)} remained visible`,
      );
    }
  } else {
    if (!activeOutputSet.has(output)) {
      diagnostics.push(`wrong output: expected transient output ${JSON.stringify(output)} is not active`);
    }
    if (exact.length === 0) {
      diagnostics.push(`missing transient: ${JSON.stringify(expectedNamespace)} on ${JSON.stringify(output)}`);
    }
    if (exact.length > 1) {
      diagnostics.push(`duplicate transient: ${JSON.stringify(expectedNamespace)} on ${JSON.stringify(output)} observed ${exact.length} times`);
    }
    if (surface === "tray-menu" && profileLayers.length > 1) {
      diagnostics.push(`duplicate tray menu: expected one global menu, observed ${profileLayers.length}`);
    }
    for (const candidate of selectedNamespaceLayers) {
      if (candidate.output !== output) {
        diagnostics.push(
          `wrong output: ${JSON.stringify(candidate.namespace)} expected ${JSON.stringify(output)}, observed ${JSON.stringify(candidate.output)}`,
        );
      }
    }
    for (const candidate of profileLayers) {
      if (candidate.namespace === expectedNamespace) continue;
      const suffix = candidate.namespace.slice(profile.namespacePrefix.length);
      if (!/^\d+$/.test(suffix)) {
        diagnostics.push(
          `wrong-index/extra namespace: ${JSON.stringify(candidate.namespace)} (expected a decimal index in 0..${monitorCount - 1})`,
        );
        continue;
      }
      const candidateIndex = Number(suffix);
      if (!Number.isSafeInteger(candidateIndex) || candidateIndex >= monitorCount) {
        diagnostics.push(
          `wrong-index/extra namespace: ${JSON.stringify(candidate.namespace)} (expected indices 0..${monitorCount - 1})`,
        );
      } else if (!activeOutputSet.has(candidate.output)) {
        diagnostics.push(
          `wrong output: ${JSON.stringify(candidate.namespace)} targets non-active output ${JSON.stringify(candidate.output)}`,
        );
      }
    }
    validatePolicies(exact, profile.layer, profile.keyboard, diagnostics);
  }

  return validationResult(
    diagnostics,
    monitorCount,
    surface,
    [expectedNamespace],
    [output],
    profile.layer,
    profile.keyboard,
    absent,
    profileLayers,
  );
}

function validatePolicies(
  layers: LayerTuple[],
  expectedLayer: string,
  expectedKeyboard: string,
  diagnostics: string[],
): void {
  for (const surface of layers) {
    const identity = `${JSON.stringify(surface.namespace)} on ${JSON.stringify(surface.output)}`;
    if (surface.layer !== expectedLayer) {
      diagnostics.push(`wrong layer: ${identity} expected ${JSON.stringify(expectedLayer)}, observed ${JSON.stringify(surface.layer)}`);
    }
    if (surface.keyboard !== expectedKeyboard) {
      diagnostics.push(
        `wrong keyboard: ${identity} expected ${JSON.stringify(expectedKeyboard)}, observed ${JSON.stringify(surface.keyboard)}`,
      );
    }
  }
}

function validationResult(
  diagnostics: string[],
  monitorCount: number,
  surface: SurfaceProfile,
  namespaces: string[],
  outputs: string[],
  layer: string,
  keyboard: string,
  absent: boolean,
  observed: LayerTuple[],
): ValidationResult {
  return {
    ok: diagnostics.length === 0,
    diagnostics,
    expected: { monitorCount, surface, namespaces, outputs, layer, keyboard, absent },
    observed,
  };
}

export function formatValidation(result: ValidationResult): string {
  const lines = result.ok ? ["niri layer smoke validation passed"] : result.diagnostics.map((item) => `- ${item}`);
  lines.push(`expected surface: ${result.expected.surface}${result.expected.absent ? " (absent)" : ""}`);
  lines.push(`expected namespaces: ${quoted(result.expected.namespaces)}`);
  lines.push(`expected active outputs: ${quoted(result.expected.outputs)}`);
  lines.push(
    result.expected.absent
      ? "expected tuple constraints: no tuple with the selected namespace/output key"
      : `expected tuple constraints: (namespace/output count policy, layer=${JSON.stringify(result.expected.layer)}, keyboard=${JSON.stringify(result.expected.keyboard)})`,
  );
  lines.push("observed shell tuples (namespace, output, layer, keyboard):");
  if (result.observed.length === 0) lines.push("  (none)");
  for (const surface of result.observed) {
    lines.push(
      `  (${JSON.stringify(surface.namespace)}, ${JSON.stringify(surface.output)}, ${JSON.stringify(surface.layer)}, ${JSON.stringify(surface.keyboard)})`,
    );
  }
  return lines.join("\n");
}

function usage(): never {
  throw new Error(
    "usage: bun scripts/niri-layer-smoke.ts --outputs <outputs.json> --layers <layers.json> --monitor-count <N> [--surface <bar|quick-panel|workspace-hud|diagnostics-dialog|tray-menu> [--monitor-index <N> --output <NAME> [--absent]]] (--surface tray-menu inspects only the Quick Panel menu layer namespace; a Bar tray menu is a native parent-relative popup that no layer inventory, and no --absent result, can confirm or deny)",
  );
}

async function main(args: string[]): Promise<number> {
  let outputsPath: string | undefined;
  let layersPath: string | undefined;
  let monitorCountText: string | undefined;
  let surface: SurfaceProfile = "bar";
  let monitorIndexText: string | undefined;
  let expectedOutput: string | undefined;
  let absent = false;
  for (let index = 0; index < args.length;) {
    const flag = args[index];
    if (flag === "--absent") {
      absent = true;
      index += 1;
      continue;
    }
    const value = args[index + 1];
    if (value === undefined) usage();
    if (flag === "--outputs") outputsPath = value;
    else if (flag === "--layers") layersPath = value;
    else if (flag === "--monitor-count") monitorCountText = value;
    else if (flag === "--surface" && SURFACE_PROFILE_NAMES.includes(value as SurfaceProfile)) surface = value as SurfaceProfile;
    else if (flag === "--monitor-index") monitorIndexText = value;
    else if (flag === "--output") expectedOutput = value;
    else usage();
    index += 2;
  }
  if (!outputsPath || !layersPath || !monitorCountText || !/^\d+$/.test(monitorCountText)) usage();

  let expectation: ValidationExpectation;
  if (surface === "bar") {
    if (monitorIndexText !== undefined || expectedOutput !== undefined || absent) usage();
    expectation = { surface };
  } else {
    if (monitorIndexText === undefined || !/^\d+$/.test(monitorIndexText) || !expectedOutput) usage();
    expectation = { surface, monitorIndex: Number(monitorIndexText), output: expectedOutput, absent };
  }

  const monitorCount = Number(monitorCountText);
  const [outputsSource, layersSource] = await Promise.all([
    Bun.file(outputsPath).text(),
    Bun.file(layersPath).text(),
  ]);
  const result = validateNiriLayers(
    parseOutputsJson(outputsSource),
    parseLayersJson(layersSource),
    monitorCount,
    expectation,
  );
  const formatted = formatValidation(result);
  (result.ok ? console.log : console.error)(formatted);
  return result.ok ? 0 : 1;
}

if (import.meta.main) {
  try {
    process.exitCode = await main(Bun.argv.slice(2));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}
