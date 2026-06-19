import { getState } from "./bridge.js";

const clock = document.getElementById("clock");
const bridgeStatus = document.getElementById("bridge-status");

async function updateState() {
  try {
    const state = await getState();
    clock.textContent = state.clock?.time || "--:--:--";
    bridgeStatus.textContent = bridgeStatusText(state);
  } catch (_error) {
    bridgeStatus.textContent = "bridge: unavailable";
  }
}

function bridgeStatusText(state) {
  const backend = state.host?.backend || "unavailable";
  const monitorCount = state.host?.monitorCount;
  const niri = state.niri;
  const parts = [`bridge: ${backend}`];

  if (Number.isInteger(monitorCount)) {
    parts.push(`monitors: ${monitorCount}`);
  }
  if (niri?.available && niri.focusedOutput) {
    parts.push(`niri: ${niri.focusedOutput}`);
  } else if (niri && !niri.available) {
    parts.push("niri: unavailable");
  }

  return parts.join(" · ");
}

void updateState();
setInterval(updateState, 1000);
