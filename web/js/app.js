import { focusWorkspace, getState } from "./bridge.js";

const STATE_POLL_INTERVAL_MS = 1000;
const ACTION_ERROR_VISIBLE_MS = 4000;

const clock = document.getElementById("clock");
const workspaceStatus = document.getElementById("workspace-status");
const focusedWindow = document.getElementById("focused-window");
const batteryStatus = document.getElementById("battery-status");
const networkStatus = document.getElementById("network-status");
const actionStatus = document.getElementById("action-status");
const bridgeStatus = document.getElementById("bridge-status");
const panelOutput = new URLSearchParams(window.location.search).get("panelOutput") || "";

let actionStatusTimer = 0;
let workspaceRenderKey = "";

async function updateState() {
  try {
    const state = await getState();
    clock.textContent = state.clock?.time || "--:--:--";
    renderNiriState(state.niri);
    renderSystemWidgets(state);
    bridgeStatus.textContent = bridgeStatusText(state);
  } catch (_error) {
    clock.textContent = "--:--:--";
    renderUnavailableNiri("bridge unavailable");
    renderSystemUnavailable("bridge unavailable");
    bridgeStatus.textContent = "bridge: unavailable";
  }
}

function renderNiriState(niri) {
  if (!niri?.available) {
    renderUnavailableNiri(niri?.reason || "niri unavailable");
    return;
  }

  renderWorkspaces(niri.workspaces);
  renderFocusedWindow(niri.focusedWindow);
}

function renderUnavailableNiri(reason) {
  workspaceRenderKey = "";
  workspaceStatus.textContent = "workspaces: unavailable";
  workspaceStatus.title = reason;
  focusedWindow.textContent = "window: unavailable";
  focusedWindow.title = reason;
}

function renderWorkspaces(workspaces) {
  if (!workspaces?.available) {
    const reason = workspaces?.reason || "workspaces unavailable";
    workspaceRenderKey = "";
    workspaceStatus.textContent = "workspaces: unavailable";
    workspaceStatus.title = reason;
    return;
  }

  const items = Array.isArray(workspaces.items) ? workspaces.items : [];
  const visibleItems = panelOutput
    ? items.filter((workspace) => workspace.output === panelOutput)
    : items;
  if (visibleItems.length === 0) {
    workspaceRenderKey = "";
    workspaceStatus.textContent = "workspaces: none";
    workspaceStatus.title = panelOutput
      ? `niri reported no workspaces for ${panelOutput}`
      : "niri reported no workspaces";
    return;
  }

  const renderKey = workspaceRenderSignature(visibleItems);
  if (renderKey === workspaceRenderKey) {
    return;
  }
  workspaceRenderKey = renderKey;

  const prefix = document.createElement("span");
  prefix.className = "workspace-prefix";
  prefix.textContent = "workspaces:";
  workspaceStatus.replaceChildren(prefix);
  workspaceStatus.title = panelOutput ? `workspaces on ${panelOutput}` : "";

  for (const workspace of visibleItems) {
    const item = document.createElement("button");
    const workspaceId = workspaceActionId(workspace);
    item.type = "button";
    item.className = "workspace-item";
    item.textContent = workspaceLabel(workspace);
    item.title = workspaceTitle(workspace);
    item.setAttribute("aria-label", `Focus ${item.title}`);
    if (Number.isInteger(workspaceId)) {
      item.dataset.workspaceId = String(workspaceId);
      item.addEventListener("pointerdown", handleWorkspacePointerDown);
      item.addEventListener("keydown", handleWorkspaceKeyDown);
    } else {
      item.disabled = true;
    }
    if (workspace.isActive) {
      item.classList.add("is-active");
    }
    if (workspace.isFocused) {
      item.classList.add("is-focused");
      item.setAttribute("aria-current", "true");
    }
    workspaceStatus.append(item);
  }
}

function workspaceRenderSignature(items) {
  return JSON.stringify(
    items.map((workspace) => [
      workspace.id,
      workspace.index,
      workspace.name || "",
      workspace.output || "",
      Boolean(workspace.isActive),
      Boolean(workspace.isFocused),
    ]),
  );
}

function workspaceActionId(workspace) {
  if (Number.isInteger(workspace?.index) && workspace.index > 0) {
    return workspace.index;
  }
  return null;
}

function handleWorkspacePointerDown(event) {
  if (event.button !== 0) {
    return;
  }
  event.preventDefault();
  void focusWorkspaceFromButton(event.currentTarget);
}

function handleWorkspaceKeyDown(event) {
  if (event.key !== "Enter" && event.key !== " ") {
    return;
  }
  event.preventDefault();
  void focusWorkspaceFromButton(event.currentTarget);
}

async function focusWorkspaceFromButton(button) {
  const workspaceId = Number(button.dataset.workspaceId);
  if (!Number.isInteger(workspaceId) || workspaceId <= 0) {
    return;
  }

  clearActionStatus();
  button.disabled = true;
  try {
    await focusWorkspace(workspaceId);
  } catch (error) {
    showActionError(error);
  } finally {
    button.disabled = false;
  }
}

function clearActionStatus() {
  window.clearTimeout(actionStatusTimer);
  actionStatusTimer = 0;
  actionStatus.textContent = "";
  actionStatus.title = "";
}

function showActionError(error) {
  const detail = error instanceof Error ? error.message : "workspace switch failed";
  actionStatus.textContent = "workspace switch failed";
  actionStatus.title = detail;
  window.clearTimeout(actionStatusTimer);
  actionStatusTimer = window.setTimeout(clearActionStatus, ACTION_ERROR_VISIBLE_MS);
}

function workspaceLabel(workspace) {
  if (typeof workspace?.name === "string" && workspace.name.length > 0) {
    return workspace.name;
  }
  if (Number.isInteger(workspace?.index)) {
    return String(workspace.index);
  }
  if (Number.isInteger(workspace?.id)) {
    return String(workspace.id);
  }
  return "?";
}

function workspaceTitle(workspace) {
  const label = workspaceLabel(workspace);
  const output = typeof workspace?.output === "string" ? workspace.output : "unknown output";
  return `workspace ${label} on ${output}`;
}

function renderFocusedWindow(focusedWindowState) {
  if (!focusedWindowState?.available) {
    const reason = focusedWindowState?.reason || "focused window unavailable";
    focusedWindow.textContent = "window: unavailable";
    focusedWindow.title = reason;
    return;
  }

  const windowInfo = focusedWindowState.window;
  if (!windowInfo) {
    focusedWindow.textContent = "no focused window";
    focusedWindow.title = "niri reported no focused window";
    return;
  }

  const appId = typeof windowInfo.appId === "string" ? windowInfo.appId : "";
  const title = typeof windowInfo.title === "string" ? windowInfo.title : "";
  const text = focusedWindowText(appId, title);
  focusedWindow.textContent = text;
  focusedWindow.title = text;
}

function focusedWindowText(appId, title) {
  if (appId && title && appId !== title) {
    return `${appId} — ${title}`;
  }
  return title || appId || "focused window";
}

function renderSystemWidgets(state) {
  renderBattery(state.battery);
  renderNetwork(state.network);
}

function renderSystemUnavailable(reason) {
  batteryStatus.hidden = true;
  batteryStatus.textContent = "";
  batteryStatus.title = reason;
  networkStatus.textContent = "net: unavailable";
  networkStatus.title = reason;
}

function renderBattery(battery) {
  if (!battery?.available) {
    batteryStatus.hidden = true;
    batteryStatus.textContent = "";
    batteryStatus.title = battery?.reason || "battery unavailable";
    return;
  }

  const percentage = Number.isInteger(battery.percentage) ? `${battery.percentage}%` : "?";
  const status = typeof battery.status === "string" ? ` ${battery.status}` : "";
  batteryStatus.hidden = false;
  batteryStatus.textContent = `bat: ${percentage}${status}`;
  batteryStatus.title = batteryTitle(battery);
}

function batteryTitle(battery) {
  const batteries = Array.isArray(battery.batteries) ? battery.batteries : [];
  if (batteries.length === 0) {
    return batteryStatus.textContent;
  }
  return batteries
    .map((item) => `${item.name}: ${item.percentage}% ${item.status}`)
    .join(" · ");
}

function renderNetwork(network) {
  if (!network?.available) {
    networkStatus.textContent = "net: unavailable";
    networkStatus.title = network?.reason || "network unavailable";
    return;
  }

  const parts = [];
  appendNetworkPart(parts, "wired", network.wired);
  appendNetworkPart(parts, "wifi", network.wireless);
  networkStatus.textContent = `net: ${parts.length > 0 ? parts.join(" · ") : "unknown"}`;
  networkStatus.title = networkTitle(network);
}

function appendNetworkPart(parts, label, counts) {
  const up = integerOrZero(counts?.up);
  const down = integerOrZero(counts?.down);
  if (up + down === 0) {
    return;
  }
  parts.push(`${label} ${up > 0 ? "up" : "down"}`);
}

function networkTitle(network) {
  const interfaces = Array.isArray(network.interfaces) ? network.interfaces : [];
  if (interfaces.length === 0) {
    return networkStatus.textContent;
  }
  return interfaces
    .map((item) => `${item.name}: ${item.kind} ${item.state}`)
    .join(" · ");
}

function integerOrZero(value) {
  return Number.isInteger(value) ? value : 0;
}

function bridgeStatusText(state) {
  const backend = state.host?.backend || "unavailable";
  const monitorCount = state.host?.monitorCount;
  const focusedOutput = state.niri?.focusedOutput;
  const parts = [`bridge: ${backend}`];

  if (Number.isInteger(monitorCount)) {
    parts.push(`monitors: ${monitorCount}`);
  }
  if (focusedOutput?.available && focusedOutput.name) {
    parts.push(`niri: ${focusedOutput.name}`);
  } else if (state.niri && !state.niri.available) {
    parts.push("niri: unavailable");
  }

  return parts.join(" · ");
}

void updateState();
setInterval(updateState, STATE_POLL_INTERVAL_MS);
