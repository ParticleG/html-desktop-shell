import { getHostInfo } from "./bridge.js";
import { startClock } from "./clock.js";

const clock = document.getElementById("clock");
const bridgeStatus = document.getElementById("bridge-status");

async function updateBridgeStatus() {
  try {
    const info = await getHostInfo();
    bridgeStatus.textContent = `bridge: ${info.backend}`;
  } catch (_error) {
    bridgeStatus.textContent = "bridge: unavailable";
  }
}

startClock(clock);
void updateBridgeStatus();
