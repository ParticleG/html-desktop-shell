import { defineStore } from "pinia";
import { ref } from "vue";

import { focusWorkspace, getHostInfo, getState } from "@html-desktop-shell/shell-api";
import type { HostInfo, ShellState } from "@html-desktop-shell/shell-api";

export const useShellStore = defineStore("shell", () => {
  const hostInfo = ref<HostInfo | null>(null);
  const state = ref<ShellState | null>(null);
  const bridgeError = ref("");
  const actionError = ref("");
  const actionErrorDetail = ref("");
  const isRefreshing = ref(false);

  async function loadHostInfo(): Promise<void> {
    try {
      hostInfo.value = await getHostInfo();
      bridgeError.value = "";
    } catch {
      bridgeError.value = "bridge: unavailable";
    }
  }

  async function refreshState(): Promise<void> {
    if (isRefreshing.value) {
      return;
    }

    isRefreshing.value = true;
    try {
      state.value = await getState();
      bridgeError.value = "";
    } catch {
      bridgeError.value = "bridge: unavailable";
    } finally {
      isRefreshing.value = false;
    }
  }

  async function focusWorkspaceFromButton(workspaceId: number): Promise<void> {
    clearActionError();
    try {
      await focusWorkspace(workspaceId);
      await refreshState();
    } catch (error) {
      actionError.value = "workspace switch failed";
      actionErrorDetail.value = error instanceof Error && error.message.length > 0
        ? error.message
        : "unknown native bridge error";
    }
  }

  function clearActionError(): void {
    actionError.value = "";
    actionErrorDetail.value = "";
  }

  return {
    hostInfo,
    state,
    bridgeError,
    actionError,
    actionErrorDetail,
    isRefreshing,
    loadHostInfo,
    refreshState,
    focusWorkspaceFromButton,
    clearActionError,
  };
});
