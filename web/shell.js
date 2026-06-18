const clock = document.getElementById("clock");
const bridgeStatus = document.getElementById("bridge-status");

function updateClock() {
  clock.textContent = new Date().toLocaleTimeString([], { hour12: false });
}

let nextRequestId = 1;

window.shell = {
  async request(method, params = {}) {
    const id = String(nextRequestId++);
    const raw = await window.webkit.messageHandlers.shell.postMessage({ id, method, params });
    const response = JSON.parse(raw);
    if (!response.ok) {
      const message = response.error?.message || "native bridge request failed";
      throw new Error(message);
    }
    return response.result;
  },
  getHostInfo() {
    return this.request("getHostInfo");
  },
  getCapabilities() {
    return this.request("getCapabilities");
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
