import { onMounted, onUnmounted } from "vue";

import { useShellStore } from "../stores/shell";

const POLL_INTERVAL_MS = 1000;

export function useShellPolling(): void {
  const shell = useShellStore();
  let disposed = false;
  let timer: number | undefined;

  async function poll(): Promise<void> {
    await shell.refreshState();
    if (!disposed) {
      timer = window.setTimeout(() => {
        void poll();
      }, POLL_INTERVAL_MS);
    }
  }

  onMounted(() => {
    void poll();
  });

  onUnmounted(() => {
    disposed = true;
    clearTimeout(timer);
  });
}
