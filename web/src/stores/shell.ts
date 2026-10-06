import { defineStore } from "pinia";
import { ref } from "vue";

import {
  focusWorkspace,
  getCapabilities,
  getHostInfo,
  getState,
  setDiagnosticsDialogOpen,
  setQuickPanelOpen,
} from "@html-desktop-shell/shell-api";
import type { Capabilities, HostInfo, ShellMethod, ShellState } from "@html-desktop-shell/shell-api";

export const useShellStore = defineStore("shell", () => {
  const hostInfo = ref<HostInfo | null>(null);
  const capabilities = ref<Capabilities | null>(null);
  const state = ref<ShellState | null>(null);
  const bridgeError = ref("");
  const actionError = ref("");
  const actionErrorDetail = ref("");
  const isRefreshing = ref(false);
  let initialization: Promise<void> | null = null;

  function loadHostInfo(): Promise<void> {
    if (initialization) {
      return initialization;
    }

    initialization = (async () => {
      try {
        const [loadedHostInfo, loadedCapabilities] = await Promise.all([
          getHostInfo(),
          getCapabilities(),
        ]);
        hostInfo.value = loadedHostInfo;
        capabilities.value = loadedCapabilities;
        bridgeError.value = "";
      } catch (error) {
        hostInfo.value = null;
        capabilities.value = null;
        initialization = null;
        bridgeError.value = errorMessage(error, "bridge: unavailable");
      }
    })();
    return initialization;
  }

  async function refreshState(): Promise<void> {
    if (isRefreshing.value) {
      return;
    }

    isRefreshing.value = true;
    try {
      state.value = await getState();
      bridgeError.value = "";
    } catch (error) {
      bridgeError.value = errorMessage(error, "bridge: unavailable");
    } finally {
      isRefreshing.value = false;
    }
  }

  async function focusWorkspaceFromButton(workspaceIndex: number): Promise<void> {
    clearActionError();
    try {
      await focusWorkspace(workspaceIndex);
      await refreshState();
    } catch (error) {
      setActionError("workspace switch failed", error);
    }
  }

  async function setQuickPanelOpenFromButton(open: boolean): Promise<void> {
    clearActionError();
    try {
      await setQuickPanelOpen(open);
    } catch (error) {
      setActionError(open ? "quick panel open failed" : "quick panel close failed", error);
    }
  }

  async function setDiagnosticsDialogOpenFromButton(open: boolean): Promise<void> {
    clearActionError();
    try {
      await setDiagnosticsDialogOpen(open);
    } catch (error) {
      setActionError(open ? "diagnostics open failed" : "diagnostics close failed", error);
    }
  }

  function hasCapability(method: ShellMethod): boolean {
    return capabilities.value?.methods.includes(method) === true;
  }

  function setActionError(message: string, error: unknown): void {
    actionError.value = message;
    actionErrorDetail.value = errorMessage(error, "unknown native bridge error");
  }

  function clearActionError(): void {
    actionError.value = "";
    actionErrorDetail.value = "";
  }

  return {
    hostInfo,
    capabilities,
    state,
    bridgeError,
    actionError,
    actionErrorDetail,
    isRefreshing,
    loadHostInfo,
    refreshState,
    focusWorkspaceFromButton,
    clearActionError,
    setQuickPanelOpenFromButton,
    setDiagnosticsDialogOpenFromButton,
    hasCapability,
  };
});

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.length > 0 ? error.message : fallback;
}
