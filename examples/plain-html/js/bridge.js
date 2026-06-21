let nextRequestId = 1;

export async function request(method, params = {}) {
  const id = String(nextRequestId++);
  const raw = await window.webkit.messageHandlers.shell.postMessage({ id, method, params });
  const response = JSON.parse(raw);
  if (!response.ok) {
    const message = response.error?.message || "native bridge request failed";
    throw new Error(message);
  }
  return response.result;
}

export function getHostInfo() {
  return request("getHostInfo");
}

export function getCapabilities() {
  return request("getCapabilities");
}

export function getState() {
  return request("getState");
}

export function focusWorkspace(workspaceId) {
  return request("niriFocusWorkspace", { workspaceId });
}
