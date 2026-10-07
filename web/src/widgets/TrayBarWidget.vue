<template>
  <div class="tray-icons" :aria-label="t('desktop.tray')">
    <Button
      v-for="item in items"
      :key="item.id"
      class="surface-action tray-icon-button"
      :data-tray-id="item.id"
      :aria-label="item.title || item.tooltip || item.id"
      :title="item.tooltip || item.title || item.id"
      :disabled="busy || !canInteract(item)"
      size="small"
      rounded
      text
      @click="activatePrimary(item.id)"
      @contextmenu.prevent="showNativeMenu(item.id)"
      @auxclick.prevent="activateSecondary($event, item.id)"
      @wheel.prevent="scrollItem($event, item.id)"
    >
      <span class="tray-icon-stack" aria-hidden="true">
        <ShellIcon :icon="item.status === 'NeedsAttention' ? item.attentionIcon ?? item.icon : item.icon" :size="18" />
        <ShellIcon v-if="item.overlayIcon" class="tray-icon-overlay" :icon="item.overlayIcon" :size="9" />
      </span>
    </Button>
    <span v-if="error" class="desktop-error tray-bar-error" role="alert" :title="error">{{ error }}</span>
  </div>
</template>

<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import {
  activateTrayItem,
  secondaryActivateTrayItem,
  showTrayItemContextMenu,
  scrollTrayItem,
} from "@html-desktop-shell/shell-api";
import type { TrayItem } from "@html-desktop-shell/shell-api";

import { useNativeAction } from "../composables/useNativeAction";
import { useShellStore } from "../stores/shell";
import ShellIcon from "./ShellIcon.vue";

const shell = useShellStore();
const { t } = useI18n();
const { busy, error, run } = useNativeAction();
const items = computed(() => shell.state?.tray?.available === true
  ? (shell.state.tray.items ?? []).filter((item) => item.status !== "Passive")
  : []);

function canInteract(item: TrayItem): boolean {
  return shell.hasCapability(item.isMenu ? "showTrayItemContextMenu" : "activateTrayItem")
    || shell.hasCapability("secondaryActivateTrayItem")
    || shell.hasCapability("showTrayItemContextMenu")
    || shell.hasCapability("scrollTrayItem");
}

async function activatePrimary(itemId: string): Promise<void> {
  const item = items.value.find((candidate) => candidate.id === itemId);
  if (!item) {
    return;
  }
  if (item.isMenu && shell.hasCapability("showTrayItemContextMenu")) {
    await run(() => showTrayItemContextMenu(itemId));
  } else if (!item.isMenu && shell.hasCapability("activateTrayItem")) {
    await run(() => activateTrayItem(itemId));
  }
}

async function showNativeMenu(itemId: string): Promise<void> {
  if (items.value.some((item) => item.id === itemId) && shell.hasCapability("showTrayItemContextMenu")) {
    await run(() => showTrayItemContextMenu(itemId));
  }
}

async function activateSecondary(event: MouseEvent, itemId: string): Promise<void> {
  if (event.button === 1 && items.value.some((item) => item.id === itemId)
    && shell.hasCapability("secondaryActivateTrayItem")) {
    await run(() => secondaryActivateTrayItem(itemId));
  }
}

async function scrollItem(event: WheelEvent, itemId: string): Promise<void> {
  if (busy.value || !items.value.some((item) => item.id === itemId) || !shell.hasCapability("scrollTrayItem")) {
    return;
  }
  await run(async () => {
    for (const [orientation, amount] of [["horizontal", event.deltaX], ["vertical", event.deltaY]] as const) {
      if (!Number.isFinite(amount) || amount === 0) {
        continue;
      }
      const delta = Math.max(-2_147_483_648, Math.min(2_147_483_647, Math.trunc(amount) || Math.sign(amount)));
      await scrollTrayItem(itemId, delta, orientation);
    }
  });
}
</script>
