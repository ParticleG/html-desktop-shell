let nextRequestId = 1;

export interface HostInfo {
  shell: "html-desktop-shell";
  backend: "wayland-layer-shell";
  bridgeVersion: 2;
  panel: PanelContext;
}

export interface PanelContext {
  index: number;
  output: string | null;
}

export interface Capabilities {
  methods: string[];
}

export interface ShellState {
  clock?: { time?: string };
  host?: { backend?: string; monitorCount?: number; bridgeVersion?: number };
  niri?: NiriState;
  battery?: BatteryState;
  network?: NetworkState;
}

export interface NiriState {
  available?: boolean;
  reason?: string;
  focusedOutput?: NiriFocusedOutputState;
  workspaces?: NiriWorkspacesState;
  focusedWindow?: NiriFocusedWindowState;
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

export interface NiriFocusedWindowState {
  available?: boolean;
  window?: FocusedWindow | null;
  reason?: string;
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
    throw new Error(response.error?.message || "native bridge request failed");
  }
  return response.result as T;
}

export function getHostInfo(): Promise<HostInfo> {
  return request("getHostInfo");
}

export function getCapabilities(): Promise<Capabilities> {
  return request("getCapabilities");
}

export function getState(): Promise<ShellState> {
  return request("getState");
}

export function focusWorkspace(workspaceId: number): Promise<{ workspaceId: number }> {
  return request("niriFocusWorkspace", { workspaceId });
}
