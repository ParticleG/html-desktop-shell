import { onUnmounted, ref } from "vue";

import { NativeBridgeError } from "@html-desktop-shell/shell-api";

export function useNativeAction() {
  const busy = ref(false);
  const error = ref("");
  let disposed = false;

  async function run<T>(operation: () => Promise<T>): Promise<T | undefined> {
    if (disposed || busy.value) {
      return undefined;
    }
    busy.value = true;
    error.value = "";
    try {
      const result = await operation();
      return disposed ? undefined : result;
    } catch (cause) {
      if (!disposed) {
        error.value = nativeErrorMessage(cause);
      }
      return undefined;
    } finally {
      if (!disposed) {
        busy.value = false;
      }
    }
  }

  onUnmounted(() => {
    disposed = true;
  });

  return { busy, error, run };
}

export function nativeErrorMessage(cause: unknown): string {
  if (cause instanceof NativeBridgeError && cause.code) {
    return `${cause.code}: ${cause.message}`;
  }
  return cause instanceof Error ? cause.message : "Native operation failed";
}
