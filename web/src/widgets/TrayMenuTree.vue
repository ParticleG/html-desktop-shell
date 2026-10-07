<template>
  <ul ref="menuElement" class="tray-menu" role="menu" :aria-label="label" @keydown="handleKeyDown">
    <li v-for="node in visibleNodes" :key="node.id" class="tray-menu-node" role="none">
      <div v-if="node.type === 'separator'" role="separator" />
      <template v-else>
        <Button
          class="surface-action"
          :data-menu-id="node.id"
          :role="node.toggleType === 'checkmark' ? 'menuitemcheckbox' : node.toggleType === 'radio' ? 'menuitemradio' : 'menuitem'"
          :aria-checked="node.toggleType === 'checkmark' ? node.toggleState === 1 ? true : node.toggleState === 0 ? false : 'mixed' : node.toggleType === 'radio' ? node.toggleState === 1 : undefined"
          :aria-haspopup="isSubmenu(node) ? 'menu' : undefined"
          :aria-expanded="isSubmenu(node) ? expandedIds.has(node.id) : undefined"
          :aria-label="isSubmenu(node) ? t(expandedIds.has(node.id) ? 'desktop.collapseMenu' : 'desktop.expandMenu', { label: node.label || String(node.id) }) : undefined"
          :disabled="busy || !node.enabled || (!isSubmenu(node) && !canActivate)"
          size="small"
          text
          @click="chooseNode(node)"
        >
          <span v-if="node.toggleType" class="tray-toggle" aria-hidden="true">
            {{ node.toggleState === 1 ? node.toggleType === 'radio' ? '●' : '✓' : node.toggleState < 0 ? '−' : '' }}
          </span>
          <ShellIcon v-if="node.icon" :icon="node.icon" :size="16" />
          <span>{{ node.label || String(node.id) }}</span>
          <span v-if="isSubmenu(node)" aria-hidden="true">{{ expandedIds.has(node.id) ? '▾' : '▸' }}</span>
        </Button>
        <TrayMenuTree
          v-if="isSubmenu(node) && node.enabled && expandedIds.has(node.id)"
          :data-submenu-id="node.id"
          :nodes="node.children"
          :label="node.label || String(node.id)"
          :parent-id="node.id"
          :expanded-ids="expandedIds"
          :busy="busy"
          :can-activate="canActivate"
          @toggle="emit('toggle', $event)"
          @activate="emit('activate', $event)"
          @close="emit('close')"
        />
      </template>
    </li>
  </ul>
</template>

<script setup lang="ts">
import Button from "primevue/button";
import { computed, nextTick, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { TrayMenuNode } from "@html-desktop-shell/shell-api";

import ShellIcon from "./ShellIcon.vue";

const props = defineProps<{
  nodes: TrayMenuNode[];
  label: string;
  parentId?: number;
  expandedIds: ReadonlySet<number>;
  busy: boolean;
  canActivate: boolean;
}>();
const emit = defineEmits<{
  toggle: [menuItemId: number];
  activate: [menuItemId: number];
  close: [];
}>();
const { t } = useI18n();
const menuElement = ref<HTMLUListElement | null>(null);
const visibleNodes = computed(() => props.nodes.filter((node) => node.visible));
let focusSubmenuId: number | null = null;

function isSubmenu(node: TrayMenuNode): boolean {
  return node.hasSubmenu || node.children.length > 0;
}

function chooseNode(node: TrayMenuNode): void {
  if (props.busy || !node.visible || !node.enabled || node.type === "separator") {
    return;
  }
  if (isSubmenu(node)) {
    emit("toggle", node.id);
  } else if (props.canActivate) {
    emit("activate", node.id);
  }
}

watch(() => props.expandedIds, async (expanded) => {
  const id = focusSubmenuId;
  if (id === null || !expanded.has(id)) {
    return;
  }
  focusSubmenuId = null;
  await nextTick();
  menuElement.value?.querySelector<HTMLButtonElement>(`[data-submenu-id="${id}"] button:not(:disabled)`)?.focus();
});

function handleKeyDown(event: KeyboardEvent): void {
  const element = menuElement.value;
  const target = event.target;
  if (!element || !(target instanceof HTMLElement) || target.closest('[role="menu"]') !== element) {
    return;
  }
  if (event.key === "Escape" || event.key === "ArrowLeft") {
    if (props.parentId !== undefined) {
      event.preventDefault();
      event.stopPropagation();
      const parentButton = element.closest("li")?.querySelector<HTMLButtonElement>("button");
      emit("toggle", props.parentId);
      parentButton?.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      emit("close");
    }
    return;
  }
  if (event.key === "ArrowRight") {
    const node = props.nodes.find((candidate) => candidate.id === Number(target.dataset.menuId));
    if (node && node.visible && node.enabled && isSubmenu(node) && !props.busy) {
      event.preventDefault();
      if (props.expandedIds.has(node.id)) {
        element.querySelector<HTMLButtonElement>(`[data-submenu-id="${node.id}"] button:not(:disabled)`)?.focus();
      } else {
        focusSubmenuId = node.id;
        emit("toggle", node.id);
      }
    }
    return;
  }
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
    return;
  }
  const buttons = Array.from(element.querySelectorAll<HTMLButtonElement>(":scope > li > button:not(:disabled)"));
  if (buttons.length === 0) {
    return;
  }
  event.preventDefault();
  const current = buttons.indexOf(target as HTMLButtonElement);
  const index = event.key === "Home" ? 0
    : event.key === "End" ? buttons.length - 1
    : event.key === "ArrowDown" ? (current + 1) % buttons.length
    : current < 0 ? buttons.length - 1 : (current - 1 + buttons.length) % buttons.length;
  buttons[index]?.focus();
}
</script>
