import { getHostInfo, getState } from "./bridge.js";

const app = document.getElementById("app");

async function render() {
  try {
    const [hostInfo, state] = await Promise.all([getHostInfo(), getState()]);
    const niri = state.niri?.available ? "available" : "unavailable";
    app.innerHTML = `
      <h1>HTML Shell bridge demo</h1>
      <dl>
        <dt>Backend</dt><dd>${hostInfo.backend}</dd>
        <dt>Bridge version</dt><dd>${hostInfo.bridgeVersion}</dd>
        <dt>Panel</dt><dd>${hostInfo.panel?.index ?? "?"} on ${hostInfo.panel?.output ?? "unknown output"}</dd>
        <dt>Clock</dt><dd>${state.clock?.time ?? "--:--:--"}</dd>
        <dt>Monitor count</dt><dd>${state.host?.monitorCount ?? "unknown"}</dd>
        <dt>niri</dt><dd>${niri}</dd>
      </dl>
    `;
  } catch (error) {
    app.textContent = error instanceof Error ? error.message : "native bridge unavailable";
  }
}

void render();
