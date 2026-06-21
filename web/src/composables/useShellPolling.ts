import { useIntervalFn } from "@vueuse/core";
import { onMounted, onUnmounted } from "vue";

import { useShellStore } from "../stores/shell";

export function useShellPolling(): void {
  const shell = useShellStore();
  const { pause, resume } = useIntervalFn(
    () => {
      void shell.refreshState();
    },
    1000,
    { immediate: false },
  );

  onMounted(async () => {
    pause();
    await shell.loadHostInfo();
    await shell.refreshState();
    resume();
  });

  onUnmounted(() => {
    pause();
  });
}
