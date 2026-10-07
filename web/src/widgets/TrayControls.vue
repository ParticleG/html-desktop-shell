<template>
  <section class="desktop-section" :aria-label="t('desktop.tray')">
    <header class="desktop-header"><h2>{{ t('desktop.tray') }}</h2></header>
    <p v-if="!trayAvailable" class="desktop-empty">
      {{ t('states.unavailable') }}<template v-if="shell.state?.tray?.available === false">: {{ shell.state.tray.reason }}</template>
    </p>
    <p v-else-if="items.length === 0" class="desktop-empty">{{ t('desktop.emptyTray') }}</p>
    <ul v-else class="desktop-list">
      <li v-for="item in items" :key="item.id" class="desktop-item" :data-tray-id="item.id">
        <div class="desktop-item-main">
          <span class="tray-icon-stack" aria-hidden="true">
            <ShellIcon :icon="item.status === 'NeedsAttention' ? item.attentionIcon ?? item.icon : item.icon" />
            <ShellIcon v-if="item.overlayIcon" class="tray-icon-overlay" :icon="item.overlayIcon" :size="10" />
          </span>
          <div class="desktop-text">
            <strong>{{ item.title || item.id }}</strong>
            <span v-if="item.tooltip" class="desktop-caption">{{ item.tooltip }}</span>
          </div>
        </div>
        <div class="desktop-controls">
          <Button
            class="surface-action"
            :label="t('desktop.activate')"
            :disabled="busy || !shell.hasCapability('activateTrayItem')"
            size="small"
            text
            @click="activateItem(item.id)"
          />
          <Button
            class="surface-action"
            :label="t('desktop.secondaryActivate')"
            :disabled="busy || !shell.hasCapability('secondaryActivateTrayItem')"
            size="small"
            text
            @click="secondaryActivateItem(item.id)"
          />
          <Button
            class="surface-action"
            :label="t('desktop.nativeMenu')"
            :disabled="busy || !shell.hasCapability('showTrayItemContextMenu')"
            size="small"
            text
            @click="showNativeMenu(item.id)"
          />
          <Button
            v-if="item.hasMenu"
            class="surface-action"
            :label="t('desktop.showMenu')"
            :disabled="busy || !canReadMenu"
            size="small"
            text
            @click="openMenu(item.id)"
          />
        </div>
      </li>
    </ul>
    <div v-if="selectedItem" class="tray-menu-panel" :aria-busy="busy">
      <header class="desktop-header">
        <h3>{{ t('desktop.menuFor', { title: selectedItem.title || selectedItem.id }) }}</h3>
        <div class="desktop-controls">
          <Button
            class="surface-action"
            :label="t('desktop.refresh')"
            :disabled="busy || !canReadMenu"
            size="small"
            text
            @click="loadMenu()"
          />
          <Button class="surface-action" :label="t('desktop.closeMenu')" size="small" text @click="closeMenu" />
        </div>
      </header>
      <p v-if="busy && !menu" class="desktop-caption" role="status">{{ t('desktop.loading') }}</p>
      <TrayMenuTree
        v-if="menu && menu.root.visible"
        :nodes="menu.root.children"
        :label="t('desktop.menuFor', { title: selectedItem.title || selectedItem.id })"
        :expanded-ids="expandedIds"
        :busy="busy || !menu.root.enabled"
        :can-activate="canActivateMenu"
        @toggle="toggleSubmenu"
        @activate="activateMenuNode"
        @close="closeMenu"
      />
    </div>
    <p v-if="error" class="desktop-error" role="alert">{{ error }}</p>
  </section>
</template>

<script setup lang="ts">
import Button from "primevue/button";
import { computed, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  activateTrayItem,
  activateTrayMenuItem,
  getTrayMenu,
  secondaryActivateTrayItem,
  showTrayItemContextMenu,
} from "@html-desktop-shell/shell-api";
import type { TrayMenu, TrayMenuNode } from "@html-desktop-shell/shell-api";

import { useNativeAction } from "../composables/useNativeAction";
import { useShellStore } from "../stores/shell";
import ShellIcon from "./ShellIcon.vue";
import TrayMenuTree from "./TrayMenuTree.vue";

const shell = useShellStore();
const { t } = useI18n();
const { busy, error, run } = useNativeAction();
const trayAvailable = computed(() => shell.state?.tray?.available === true);
const items = computed(() => shell.state?.tray?.available === true ? shell.state.tray.items ?? [] : []);
const canReadMenu = computed(() => shell.hasCapability("getTrayMenu"));
const canActivateMenu = computed(() => shell.hasCapability("activateTrayMenuItem"));
const selectedId = ref<string | null>(null);
const selectedItem = computed(() => items.value.find((item) => item.id === selectedId.value));
const menu = ref<TrayMenu | null>(null);
const expandedIds = ref<ReadonlySet<number>>(new Set());
let menuGeneration = 0;

function closeMenu(): void {
  menuGeneration++;
  selectedId.value = null;
  menu.value = null;
  expandedIds.value = new Set();
  error.value = "";
}

watch(
  [() => selectedItem.value?.id, () => selectedItem.value?.hasMenu, canReadMenu, canActivateMenu],
  ([id, hasMenu, canRead, canActivate], previous) => {
    if (selectedId.value && (!id || !hasMenu || !canRead || (previous[3] && !canActivate))) {
      closeMenu();
    }
  },
  { flush: "sync" },
);

onUnmounted(() => {
  menuGeneration++;
});

async function activateItem(itemId: string): Promise<void> {
  if (items.value.some((item) => item.id === itemId) && shell.hasCapability("activateTrayItem")) {
    await run(() => activateTrayItem(itemId));
  }
}

async function secondaryActivateItem(itemId: string): Promise<void> {
  if (items.value.some((item) => item.id === itemId) && shell.hasCapability("secondaryActivateTrayItem")) {
    await run(() => secondaryActivateTrayItem(itemId));
  }
}

async function showNativeMenu(itemId: string): Promise<void> {
  if (items.value.some((item) => item.id === itemId) && shell.hasCapability("showTrayItemContextMenu")) {
    await run(() => showTrayItemContextMenu(itemId));
  }
}

async function openMenu(itemId: string): Promise<void> {
  if (busy.value || !canReadMenu.value || !items.value.some((item) => item.id === itemId && item.hasMenu)) {
    return;
  }
  closeMenu();
  selectedId.value = itemId;
  await loadMenu();
}

function findEnabledNode(nodes: readonly TrayMenuNode[], id: number): TrayMenuNode | undefined {
  for (const node of nodes) {
    if (!node.visible || !node.enabled || node.type === "separator") {
      continue;
    }
    if (node.id === id) {
      return node;
    }
    const child = findEnabledNode(node.children, id);
    if (child) {
      return child;
    }
  }
  return undefined;
}

async function loadMenu(menuItemId = 0): Promise<void> {
  const itemId = selectedId.value;
  const generation = menuGeneration;
  if (!itemId || !selectedItem.value?.hasMenu || !canReadMenu.value || busy.value) {
    return;
  }
  const result = await run(async () => {
    try {
      return await getTrayMenu(itemId, menuItemId);
    } catch (cause) {
      if (generation !== menuGeneration || selectedId.value !== itemId) {
        return undefined;
      }
      throw cause;
    }
  });
  if (result === undefined || generation !== menuGeneration || selectedId.value !== itemId
    || !selectedItem.value?.hasMenu || !canReadMenu.value) {
    return;
  }
  if (result.itemId !== itemId) {
    error.value = "Native tray menu returned a different item";
    return;
  }
  menu.value = result;
  const nextExpanded = new Set(expandedIds.value);
  if (menuItemId !== 0) {
    nextExpanded.add(menuItemId);
  }
  for (const id of nextExpanded) {
    const node = findEnabledNode([result.root], id);
    if (!node || (!node.hasSubmenu && node.children.length === 0)) {
      nextExpanded.delete(id);
    }
  }
  expandedIds.value = nextExpanded;
}

async function toggleSubmenu(menuItemId: number): Promise<void> {
  const current = menu.value;
  if (!current || busy.value || !canReadMenu.value) {
    return;
  }
  const node = findEnabledNode([current.root], menuItemId);
  if (!node || (!node.hasSubmenu && node.children.length === 0)) {
    return;
  }
  if (expandedIds.value.has(menuItemId)) {
    const nextExpanded = new Set(expandedIds.value);
    nextExpanded.delete(menuItemId);
    expandedIds.value = nextExpanded;
  } else {
    await loadMenu(menuItemId);
  }
}

async function activateMenuNode(menuItemId: number): Promise<void> {
  const current = menu.value;
  const itemId = selectedId.value;
  if (!current || !itemId || current.itemId !== itemId || !selectedItem.value?.hasMenu || !canActivateMenu.value) {
    return;
  }
  const node = findEnabledNode([current.root], menuItemId);
  if (!node || node.hasSubmenu || node.children.length > 0) {
    return;
  }
  await run(() => activateTrayMenuItem(itemId, menuItemId, current.revision));
}
</script>
