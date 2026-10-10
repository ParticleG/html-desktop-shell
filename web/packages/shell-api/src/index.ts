let nextRequestId = 1;

export type SurfaceContext =
  | BarSurfaceContext
  | QuickPanelSurfaceContext
  | WorkspaceHudSurfaceContext
  | DiagnosticsDialogSurfaceContext
  | TrayMenuSurfaceContext;

export interface BarSurfaceContext {
  kind: "bar";
  monitorIndex: number;
  output: string | null;
}

export interface QuickPanelSurfaceContext {
  kind: "quickPanel";
  monitorIndex: number;
  output: string | null;
}

export interface WorkspaceHudSurfaceContext {
  kind: "workspaceHud";
  monitorIndex: number;
  output: string | null;
  workspaceIndex: number;
}

export interface DiagnosticsDialogSurfaceContext {
  kind: "diagnosticsDialog";
  monitorIndex: number;
  output: string | null;
}

export interface TrayMenuSurfaceContext {
  kind: "trayMenu";
  monitorIndex: number;
  output: string | null;
  /** Native-bound tray item identity; never supplied by the browser. */
  trayItemId: string;
}

export interface HostInfo {
  shell: "html-desktop-shell";
  backend: "wayland-layer-shell";
  bridgeVersion: 6;
  surface: SurfaceContext;
}

export type ShellMethod =
  | "getHostInfo"
  | "getCapabilities"
  | "getState"
  | "niriFocusWorkspace"
  | "setQuickPanelOpen"
  | "setDiagnosticsDialogOpen"
  | "setAudioVolume"
  | "setAudioMuted"
  | "getTrayMenu"
  | "activateTrayItem"
  | "secondaryActivateTrayItem"
  | "showTrayItemContextMenu"
  | "scrollTrayItem"
  | "activateTrayMenuItem"
  | "getNotifications"
  | "dismissNotification"
  | "invokeNotificationAction"
  | "markAllNotificationsRead"
  | "searchApplications"
  | "launchApplication"
  | "closeTrayMenu";

export interface Capabilities {
  methods: ShellMethod[];
}

const SHELL_METHODS: Record<ShellMethod, true> = {
  getHostInfo: true,
  getCapabilities: true,
  getState: true,
  niriFocusWorkspace: true,
  setQuickPanelOpen: true,
  setDiagnosticsDialogOpen: true,
  setAudioVolume: true,
  setAudioMuted: true,
  getTrayMenu: true,
  activateTrayItem: true,
  secondaryActivateTrayItem: true,
  showTrayItemContextMenu: true,
  scrollTrayItem: true,
  activateTrayMenuItem: true,
  getNotifications: true,
  dismissNotification: true,
  invokeNotificationAction: true,
  markAllNotificationsRead: true,
  searchApplications: true,
  launchApplication: true,
  closeTrayMenu: true,
};

const U32_MAX = 0xffff_ffff;

export function validateHostInfo(value: unknown): HostInfo {
  const hostInfo = requireExactObject(value, ["shell", "backend", "bridgeVersion", "surface"], "HostInfo");
  if (hostInfo.shell !== "html-desktop-shell") {
    throw invalidResult('HostInfo.shell must equal "html-desktop-shell"');
  }
  if (hostInfo.backend !== "wayland-layer-shell") {
    throw invalidResult('HostInfo.backend must equal "wayland-layer-shell"');
  }
  if (hostInfo.bridgeVersion !== 6) {
    throw invalidResult("HostInfo.bridgeVersion must equal 6");
  }

  return {
    shell: hostInfo.shell,
    backend: hostInfo.backend,
    bridgeVersion: hostInfo.bridgeVersion,
    surface: validateSurfaceContext(hostInfo.surface),
  };
}

export function validateSurfaceContext(value: unknown): SurfaceContext {
  const surface = requireObject(value, "HostInfo.surface");
  switch (surface.kind) {
    case "bar":
    case "quickPanel":
    case "diagnosticsDialog": {
      requireExactKeys(surface, ["kind", "monitorIndex", "output"], `HostInfo.surface (${surface.kind})`);
      const shared = validateSurfaceFields(surface);
      return { kind: surface.kind, ...shared };
    }
    case "workspaceHud": {
      requireExactKeys(
        surface,
        ["kind", "monitorIndex", "output", "workspaceIndex"],
        "HostInfo.surface (workspaceHud)",
      );
      const shared = validateSurfaceFields(surface);
      if (!Number.isSafeInteger(surface.workspaceIndex) || (surface.workspaceIndex as number) <= 0) {
        throw invalidResult("HostInfo.surface.workspaceIndex must be a positive integer");
      }
      return { kind: "workspaceHud", ...shared, workspaceIndex: surface.workspaceIndex as number };
    }
    case "trayMenu": {
      requireExactKeys(
        surface,
        ["kind", "monitorIndex", "output", "trayItemId"],
        "HostInfo.surface (trayMenu)",
      );
      const shared = validateSurfaceFields(surface);
      if (typeof surface.trayItemId !== "string" || surface.trayItemId.length === 0) {
        throw invalidResult("HostInfo.surface.trayItemId must be a non-empty string");
      }
      return { kind: "trayMenu", ...shared, trayItemId: surface.trayItemId };
    }
    default:
      throw invalidResult(`HostInfo.surface.kind is unsupported: ${JSON.stringify(surface.kind)}`);
  }
}

export function validateCapabilities(value: unknown): Capabilities {
  const capabilities = requireExactObject(value, ["methods"], "Capabilities");
  if (!Array.isArray(capabilities.methods)) {
    throw invalidResult("Capabilities.methods must be an array");
  }
  if (
    !capabilities.methods.every(
      (method) => typeof method === "string" && Object.hasOwn(SHELL_METHODS, method),
    )
  ) {
    throw invalidResult("Capabilities.methods contains an unsupported method");
  }
  return { methods: capabilities.methods as ShellMethod[] };
}

function validateSurfaceFields(surface: Record<string, unknown>): {
  monitorIndex: number;
  output: string | null;
} {
  if (
    !Number.isInteger(surface.monitorIndex)
    || (surface.monitorIndex as number) < 0
    || (surface.monitorIndex as number) > U32_MAX
  ) {
    throw invalidResult("HostInfo.surface.monitorIndex must be a nonnegative 32-bit integer");
  }
  if (surface.output !== null && typeof surface.output !== "string") {
    throw invalidResult("HostInfo.surface.output must be a string or null");
  }
  return { monitorIndex: surface.monitorIndex as number, output: surface.output as string | null };
}

function requireExactObject(
  value: unknown,
  keys: readonly string[],
  label: string,
): Record<string, unknown> {
  const object = requireObject(value, label);
  requireExactKeys(object, keys, label);
  return object;
}

function requireObject(value: unknown, label: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw invalidResult(`${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

function requireExactKeys(object: Record<string, unknown>, keys: readonly string[], label: string): void {
  const actualKeys = Object.keys(object);
  if (actualKeys.length !== keys.length || !keys.every((key) => Object.hasOwn(object, key))) {
    throw invalidResult(`${label} has an invalid shape`);
  }
}

function invalidResult(message: string): TypeError {
  return new TypeError(`invalid native bridge result: ${message}`);
}

export interface ShellState {
  clock?: { time?: string };
  host?: { backend?: string; monitorCount?: number; bridgeVersion?: number };
  niri?: NiriState;
  battery?: BatteryState;
  network?: NetworkState;
  audio?: AudioState | { available: true };
  tray?: TrayState;
  notifications?: NotificationsState;
  applications?: ApplicationsState;
}

export interface NiriState {
  available?: boolean;
  reason?: string;
  focusedOutput?: NiriFocusedOutputState;
  workspaces?: NiriWorkspacesState;
  focusedWindows?: NiriFocusedWindowsState;
}

export interface NiriFocusedOutputState {
  available?: boolean;
  name?: string;
  reason?: string;
}

export interface NiriWorkspacesState {
  available?: boolean;
  items?: NiriWorkspace[];
  reason?: string;
}

export interface NiriWorkspace {
  id?: number;
  index?: number;
  name?: string;
  output?: string;
  isActive?: boolean;
  isFocused?: boolean;
}

export interface NiriFocusedWindowsState {
  available: boolean;
  items?: NiriFocusedWindowItem[];
  reason?: string;
}

export interface NiriFocusedWindowItem {
  output: string;
  window: FocusedWindow | null;
}

export interface FocusedWindow {
  appId?: string;
  title?: string;
}

export interface BatteryState {
  available?: boolean;
  reason?: string;
  percentage?: number;
  status?: string;
  batteries?: BatteryItem[];
}

export interface BatteryItem {
  name?: string;
  percentage?: number;
  status?: string;
}

export interface NetworkState {
  available?: boolean;
  reason?: string;
  wired?: NetworkCounts;
  wireless?: NetworkCounts;
  interfaces?: NetworkInterfaceState[];
}

export interface NetworkCounts {
  up?: number;
  down?: number;
}

export interface NetworkInterfaceState {
  name?: string;
  kind?: string;
  state?: string;
  isUp?: boolean;
}

export interface ShellIcon {
  width: number;
  height: number;
  /** Base64-encoded, tightly packed, unpremultiplied RGBA8 pixels. */
  rgba: string;
}

export interface UnavailableProvider {
  available: false;
  reason: string;
}

export type AudioState = UnavailableProvider | {
  available: true;
  volume: number;
  muted: boolean;
};

export interface TrayItem {
  id: string;
  title: string;
  status: string;
  category: string;
  isMenu: boolean;
  hasMenu: boolean;
  tooltip: string;
  icon: ShellIcon | null;
  attentionIcon: ShellIcon | null;
  overlayIcon: ShellIcon | null;
}

export type TrayState = UnavailableProvider | {
  available: true;
  items?: TrayItem[];
  /** Diagnostics receives count-only tray state. */
  count?: number;
};

export interface TrayMenuNode {
  id: number;
  label: string;
  enabled: boolean;
  visible: boolean;
  type: string;
  toggleType: string | null;
  toggleState: number;
  icon: ShellIcon | null;
  children: TrayMenuNode[];
  hasSubmenu: boolean;
}

export interface TrayMenu {
  itemId: string;
  revision: number;
  root: TrayMenuNode;
}

export type NotificationsState = UnavailableProvider | {
  available: true;
  unreadCount: number;
  activeCount: number;
  historyCount: number;
};

export interface NotificationItem {
  id: number;
  appName: string;
  summary: string;
  body: string;
  createdAt: number;
  active: boolean;
  read: boolean;
  urgency: "low" | "normal" | "critical";
  icon: ShellIcon | null;
  actions: Array<{ key: string; label: string }>;
}

export interface DesktopApplication {
  id: string;
  name: string;
  description: string | null;
  icon: ShellIcon | null;
}

export type ApplicationsState = UnavailableProvider | {
  available: true;
  count: number;
};

export interface NativeResponse<T> {
  id: string;
  ok: boolean;
  result?: T;
  error?: NativeError;
}

export interface NativeError {
  code?: string;
  message?: string;
}

export class NativeBridgeError extends Error {
  readonly code: string | undefined;

  constructor(error: NativeError | undefined) {
    super(error?.message || "native bridge request failed");
    this.name = "NativeBridgeError";
    this.code = error?.code;
  }
}

interface NativeRequestPayload {
  id: string;
  method: string;
  params: unknown;
}

interface WebKitShellHandler {
  postMessage(request: NativeRequestPayload): Promise<string> | string;
}

declare global {
  interface Window {
    webkit?: {
      messageHandlers?: {
        shell?: WebKitShellHandler;
      };
    };
  }
}

export async function request<T = unknown>(method: string, params?: unknown): Promise<T> {
  const handler = window.webkit?.messageHandlers?.shell;
  if (!handler) {
    throw new Error("native bridge unavailable");
  }

  const id = String(nextRequestId++);
  const raw = await handler.postMessage({ id, method, params: params ?? {} });
  const response = JSON.parse(raw) as NativeResponse<T>;
  if (response.ok === false) {
    throw new NativeBridgeError(response.error);
  }
  return response.result as T;
}

export async function getHostInfo(): Promise<HostInfo> {
  return validateHostInfo(await request<unknown>("getHostInfo"));
}

export async function getCapabilities(): Promise<Capabilities> {
  return validateCapabilities(await request<unknown>("getCapabilities"));
}

export function getState(): Promise<ShellState> {
  return request("getState");
}

export function focusWorkspace(workspaceIndex: number): Promise<{ workspaceIndex: number }> {
  return request("niriFocusWorkspace", { workspaceIndex });
}

export function setQuickPanelOpen(open: boolean): Promise<{ open: boolean }> {
  return request("setQuickPanelOpen", { open });
}

export function setDiagnosticsDialogOpen(open: boolean): Promise<{ open: boolean }> {
  return request("setDiagnosticsDialogOpen", { open });
}

export function setAudioVolume(volume: number): Promise<AudioState> {
  return request("setAudioVolume", { volume });
}

export function setAudioMuted(muted: boolean): Promise<AudioState> {
  return request("setAudioMuted", { muted });
}

export function getTrayMenu(itemId: string, menuItemId = 0): Promise<TrayMenu> {
  return request("getTrayMenu", { itemId, menuItemId });
}

export function activateTrayItem(itemId: string): Promise<{ itemId: string }> {
  return request("activateTrayItem", { itemId });
}

export function secondaryActivateTrayItem(itemId: string): Promise<{ itemId: string }> {
  return request("secondaryActivateTrayItem", { itemId });
}

export function showTrayItemContextMenu(itemId: string): Promise<null> {
  return request("showTrayItemContextMenu", { itemId });
}

export function scrollTrayItem(
  itemId: string,
  delta: number,
  orientation: "horizontal" | "vertical",
): Promise<{ itemId: string }> {
  return request("scrollTrayItem", { itemId, delta, orientation });
}

export function activateTrayMenuItem(
  itemId: string,
  menuItemId: number,
  revision: number,
): Promise<{ itemId: string; menuItemId: number; revision: number }> {
  return request("activateTrayMenuItem", { itemId, menuItemId, revision });
}

export function closeTrayMenu(): Promise<null> {
  return request("closeTrayMenu", {});
}

export function getNotifications(): Promise<{ items: NotificationItem[] }> {
  return request("getNotifications", {});
}

export function dismissNotification(notificationId: number): Promise<NotificationsState> {
  return request("dismissNotification", { notificationId });
}

export function invokeNotificationAction(
  notificationId: number,
  actionKey: string,
): Promise<NotificationsState> {
  return request("invokeNotificationAction", { notificationId, actionKey });
}

export function markAllNotificationsRead(): Promise<NotificationsState> {
  return request("markAllNotificationsRead", {});
}

export function searchApplications(
  query: string,
  limit = 25,
): Promise<{ items: DesktopApplication[] }> {
  return request("searchApplications", { query, limit });
}

export function launchApplication(applicationId: string): Promise<{ applicationId: string }> {
  return request("launchApplication", { applicationId });
}
