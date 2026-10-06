let nextRequestId = 1;

export class NativeBridgeError extends Error {
  constructor(error) {
    super(error?.message || "native bridge request failed");
    this.name = "NativeBridgeError";
    this.code = error?.code;
  }
}

export async function request(method, params = {}) {
  const id = String(nextRequestId++);
  const raw = await window.webkit.messageHandlers.shell.postMessage({ id, method, params });
  const response = JSON.parse(raw);
  if (!response.ok) {
    throw new NativeBridgeError(response.error);
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

export function focusWorkspace(workspaceIndex) {
  return request("niriFocusWorkspace", { workspaceIndex });
}

export function setQuickPanelOpen(open) {
  return request("setQuickPanelOpen", { open });
}

export function setDiagnosticsDialogOpen(open) {
  return request("setDiagnosticsDialogOpen", { open });
}
