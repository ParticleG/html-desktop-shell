<template>
  <main
    id="tray-menu"
    class="surface-root surface-tray-menu"
    :aria-label="t('surfaces.trayMenu')"
    @keydown.esc="requestClose"
  >
    <section id="tray-menu-content" ref="contentElement" class="tray-menu-content" :aria-busy="busy">
      <div class="desktop-controls tray-menu-controls">
        <Button
          class="surface-action"
          :label="t('desktop.refresh')"
          :disabled="busy || !canRead"
          size="small"
          text
          @click="refresh"
        />
        <Button
          class="surface-action"
          :label="t('desktop.closeMenu')"
          :disabled="closing || !canClose"
          size="small"
          text
          @click="requestClose"
        />
      </div>
      <p v-if="!canRead" class="desktop-empty">{{ t("states.unavailable") }}</p>
      <p v-else-if="busy && !menu" class="desktop-caption" role="status">{{ t("desktop.loading") }}</p>
      <TrayMenuTree
        v-if="menu && menu.root.visible"
        :nodes="menu.root.children"
        :label="menuLabel"
        :expanded-ids="expandedIds"
        :busy="busy || !menu.root.enabled"
        :can-activate="canActivate"
        @toggle="toggleSubmenu"
        @activate="activateLeaf"
        @close="requestClose"
      />
      <p v-if="error" class="desktop-error" role="alert">{{ error }}</p>
    </section>
  </main>
</template>

<script setup lang="ts">
import Button from "primevue/button";
import { computed, nextTick, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { closeTrayMenu } from "@html-desktop-shell/shell-api";
import type { TrayMenuSurfaceContext } from "@html-desktop-shell/shell-api";

import { nativeErrorMessage, useNativeAction } from "../composables/useNativeAction";
import { useTrayMenu } from "../composables/useTrayMenu";
import { useShellStore } from "../stores/shell";
import TrayMenuTree from "../widgets/TrayMenuTree.vue";

const props = defineProps<{ surface: TrayMenuSurfaceContext }>();
const { t } = useI18n();
const shell = useShellStore();
const { busy, error, run } = useNativeAction();
const canRead = computed(() => shell.hasCapability("getTrayMenu"));
const canActivate = computed(() => shell.hasCapability("activateTrayMenuItem"));
const canClose = computed(() => shell.hasCapability("closeTrayMenu"));
const menuLabel = computed(() => t("desktop.menuFor", { title: props.surface.trayItemId }));
const contentElement = ref<HTMLElement | null>(null);
const closing = ref(false);
const { menu, expandedIds, load, toggleSubmenu, activateNode } = useTrayMenu({
  itemId: () => props.surface.trayItemId,
  isSupported: () => true,
  canRead: () => canRead.value,
  canActivate: () => canActivate.value,
  busy,
  error,
  run,
});

async function refresh(): Promise<void> {
  await load();
  await nextTick();
  contentElement.value?.querySelector<HTMLButtonElement>(".tray-menu button:not(:disabled)")?.focus();
}

async function activateLeaf(menuItemId: number): Promise<void> {
  if (await activateNode(menuItemId)) {
    await requestClose();
  }
}

async function requestClose(): Promise<void> {
  if (closing.value || !canClose.value) {
    return;
  }
  closing.value = true;
  error.value = "";
  try {
    await closeTrayMenu();
  } catch (cause) {
    error.value = nativeErrorMessage(cause);
  } finally {
    closing.value = false;
  }
}

onMounted(() => {
  void refresh();
});
</script>
