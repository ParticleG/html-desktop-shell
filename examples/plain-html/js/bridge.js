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

export function setAudioVolume(volume) {
  return request("setAudioVolume", { volume });
}

export function setAudioMuted(muted) {
  return request("setAudioMuted", { muted });
}

export function getTrayMenu(itemId, menuItemId = 0) {
  return request("getTrayMenu", { itemId, menuItemId });
}

export function activateTrayItem(itemId) {
  return request("activateTrayItem", { itemId });
}

export function secondaryActivateTrayItem(itemId) {
  return request("secondaryActivateTrayItem", { itemId });
}

export function showTrayItemContextMenu(itemId) {
  return request("showTrayItemContextMenu", { itemId });
}

export function scrollTrayItem(itemId, delta, orientation) {
  return request("scrollTrayItem", { itemId, delta, orientation });
}

export function activateTrayMenuItem(itemId, menuItemId, revision) {
  return request("activateTrayMenuItem", { itemId, menuItemId, revision });
}

export function closeTrayMenu() {
  return request("closeTrayMenu", {});
}

export function getNotifications() {
  return request("getNotifications", {});
}

export function dismissNotification(notificationId) {
  return request("dismissNotification", { notificationId });
}

export function invokeNotificationAction(notificationId, actionKey) {
  return request("invokeNotificationAction", { notificationId, actionKey });
}

export function markAllNotificationsRead() {
  return request("markAllNotificationsRead", {});
}

export function searchApplications(query, limit = 25) {
  return request("searchApplications", { query, limit });
}

export function launchApplication(applicationId) {
  return request("launchApplication", { applicationId });
}
