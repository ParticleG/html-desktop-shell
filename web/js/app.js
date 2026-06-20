import { getState } from "./bridge.js";

const clock = document.getElementById("clock");
const workspaceStatus = document.getElementById("workspace-status");
const focusedWindow = document.getElementById("focused-window");
const bridgeStatus = document.getElementById("bridge-status");

async function updateState() {
  try {
    const state = await getState();
    clock.textContent = state.clock?.time || "--:--:--";
    renderNiriState(state.niri);
    bridgeStatus.textContent = bridgeStatusText(state);
  } catch (_error) {
    clock.textContent = "--:--:--";
    renderUnavailableNiri("bridge unavailable");
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
  workspaceStatus.textContent = "workspaces: unavailable";
  workspaceStatus.title = reason;
  focusedWindow.textContent = "window: unavailable";
  focusedWindow.title = reason;
}

function renderWorkspaces(workspaces) {
  if (!workspaces?.available) {
    const reason = workspaces?.reason || "workspaces unavailable";
    workspaceStatus.textContent = "workspaces: unavailable";
    workspaceStatus.title = reason;
    return;
  }

  const items = Array.isArray(workspaces.items) ? workspaces.items : [];
  if (items.length === 0) {
    workspaceStatus.textContent = "workspaces: none";
    workspaceStatus.title = "niri reported no workspaces";
    return;
  }

  const prefix = document.createElement("span");
  prefix.className = "workspace-prefix";
  prefix.textContent = "workspaces:";
  workspaceStatus.replaceChildren(prefix);
  workspaceStatus.title = "";

  for (const workspace of items) {
    const item = document.createElement("span");
    item.className = "workspace-item";
    item.textContent = workspaceLabel(workspace);
    item.dataset.workspaceId = String(workspace.id ?? "");
    item.title = workspaceTitle(workspace);
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
  return `${label} on ${output}`;
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
setInterval(updateState, 1000);
