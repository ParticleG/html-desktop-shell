const clock = document.getElementById("clock");
const bridgeStatus = document.getElementById("bridge-status");

function updateClock() {
  clock.textContent = new Date().toLocaleTimeString([], { hour12: false });
}

window.shell = {
  async getHostInfo() {
    const raw = await window.webkit.messageHandlers.shell.postMessage({ method: "getHostInfo" });
    return JSON.parse(raw);
  },
};

async function updateBridgeStatus() {
  try {
    const info = await window.shell.getHostInfo();
    bridgeStatus.textContent = `bridge: ${info.backend}`;
  } catch (_error) {
    bridgeStatus.textContent = "bridge: unavailable";
  }
}

updateClock();
setInterval(updateClock, 1000);
void updateBridgeStatus();
