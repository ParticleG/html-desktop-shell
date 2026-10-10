import type {
  BatteryState,
  NetworkState,
  NiriFocusedWindowsState,
  NiriWorkspace,
  ShellState,
} from "@html-desktop-shell/shell-api";

export function visibleWorkspaces(
  state: ShellState | null | undefined,
  surfaceOutput: string | null | undefined,
): NiriWorkspace[] {
  const items = state?.niri?.workspaces?.items;
  if (!Array.isArray(items) || typeof surfaceOutput !== "string" || surfaceOutput.length === 0) {
    return [];
  }

  return items.filter((workspace) => workspace.output === surfaceOutput);
}

export function workspaceLabel(workspace: NiriWorkspace): string {
  const name = typeof workspace.name === "string" ? workspace.name.trim() : "";
  if (name.length > 0) {
    return name;
  }
  if (typeof workspace.index === "number" && Number.isInteger(workspace.index)) {
    return String(workspace.index);
  }
  if (typeof workspace.id === "number" && Number.isInteger(workspace.id)) {
    return String(workspace.id);
  }
  return "?";
}

export function workspaceTitle(workspace: NiriWorkspace): string {
  const output = typeof workspace.output === "string" && workspace.output.length > 0
    ? workspace.output
    : "unknown output";
  return `workspace ${workspaceLabel(workspace)} on ${output}`;
}

export function focusedWindowText(
  windowsState: NiriFocusedWindowsState | undefined,
  surfaceOutput: string | null | undefined,
): string {
  if (!windowsState?.available) {
    return "window: unavailable";
  }

  const item = Array.isArray(windowsState.items)
    ? windowsState.items.find(({ output }) => output === surfaceOutput)
    : undefined;
  const windowInfo = item?.window;
  if (!windowInfo) {
    return "no focused window";
  }

  const appId = typeof windowInfo.appId === "string" ? windowInfo.appId : "";
  const title = typeof windowInfo.title === "string" ? windowInfo.title : "";
  if (appId.length > 0 && title.length > 0 && appId !== title) {
    return `${appId} — ${title}`;
  }
  return title || appId || "focused window";
}

export function batteryText(battery: BatteryState | undefined): string | null {
  if (!battery?.available) {
    return null;
  }

  const percentage = typeof battery.percentage === "number" && Number.isInteger(battery.percentage)
    ? String(battery.percentage)
    : "?";
  const status = typeof battery.status === "string" && battery.status.length > 0
    ? ` ${battery.status}`
    : "";
  return `bat: ${percentage}%${status}`;
}

export function networkText(network: NetworkState | undefined): string {
  if (!network?.available) {
    return "net: unavailable";
  }

  const wired = network.wired;
  const wireless = network.wireless;
  const wiredUp = typeof wired?.up === "number" && Number.isInteger(wired.up) ? wired.up : 0;
  const wiredDown = typeof wired?.down === "number" && Number.isInteger(wired.down) ? wired.down : 0;
  const wirelessUp = typeof wireless?.up === "number" && Number.isInteger(wireless.up) ? wireless.up : 0;
  const wirelessDown = typeof wireless?.down === "number" && Number.isInteger(wireless.down) ? wireless.down : 0;
  const parts: string[] = [];

  if (wiredUp + wiredDown > 0) {
    parts.push(`wired ${wiredUp > 0 ? "up" : "down"}`);
  }
  if (wirelessUp + wirelessDown > 0) {
    parts.push(`wifi ${wirelessUp > 0 ? "up" : "down"}`);
  }

  return `net: ${parts.length > 0 ? parts.join(" · ") : "unknown"}`;
}

export function bridgeStatusText(
  state: ShellState | null | undefined,
  surfaceOutput: string | null | undefined,
): string {
  const backend = state?.host?.backend || "unavailable";
  const monitorCount = state?.host?.monitorCount;
  const parts = [`bridge: ${backend}`];

  if (typeof monitorCount === "number" && Number.isInteger(monitorCount)) {
    parts.push(`monitors: ${monitorCount}`);
  }
  if (state?.niri?.available && surfaceOutput) {
    parts.push(`niri: ${surfaceOutput}`);
  } else if (state?.niri && !state.niri.available) {
    parts.push("niri: unavailable");
  }

  return parts.join(" · ");
}
