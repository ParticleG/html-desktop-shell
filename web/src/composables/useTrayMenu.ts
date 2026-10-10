import { onUnmounted, ref, type Ref } from "vue";

import { activateTrayMenuItem, getTrayMenu } from "@html-desktop-shell/shell-api";
import type { TrayMenu, TrayMenuNode } from "@html-desktop-shell/shell-api";

export interface TrayMenuSessionOptions {
  /** Native-bound item identity, or null while no item is selected. */
  itemId: () => string | null;
  /** Whether the bound item still exists and exports a menu. */
  isSupported: () => boolean;
  /** Whether this surface may read the native menu. */
  canRead: () => boolean;
  /** Whether this surface may activate native menu leaves. */
  canActivate: () => boolean;
  /** Shared native-action busy/error state owned by the caller. */
  busy: Ref<boolean>;
  error: Ref<string>;
  run: <T>(operation: () => Promise<T>) => Promise<T | undefined>;
}

/**
 * Owns the native tray-menu session shared by the Quick Panel tray controls and
 * the independent TrayMenu surface: root fetch, lazy submenu preparation with
 * native revision replacement, leaf activation and asynchronous retirement.
 *
 * `activateNode` reports whether the native activation actually succeeded so a
 * caller that must dismiss its own surface can do so only after the successful
 * native response has been handled.
 */
export function useTrayMenu(options: TrayMenuSessionOptions) {
  const menu = ref<TrayMenu | null>(null);
  const expandedIds = ref<ReadonlySet<number>>(new Set());
  let generation = 0;

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

  function reset(): void {
    generation++;
    menu.value = null;
    expandedIds.value = new Set();
  }

  async function load(menuItemId = 0): Promise<void> {
    const itemId = options.itemId();
    const requestGeneration = generation;
    if (!itemId || !options.isSupported() || !options.canRead() || options.busy.value) {
      return;
    }
    const result = await options.run(async () => {
      try {
        return await getTrayMenu(itemId, menuItemId);
      } catch (cause) {
        if (requestGeneration !== generation || options.itemId() !== itemId) {
          return undefined;
        }
        throw cause;
      }
    });
    if (result === undefined || requestGeneration !== generation || options.itemId() !== itemId
      || !options.isSupported() || !options.canRead()) {
      return;
    }
    if (result.itemId !== itemId) {
      options.error.value = "Native tray menu returned a different item";
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
    if (!current || options.busy.value || !options.canRead()) {
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
      await load(menuItemId);
    }
  }

  async function activateNode(menuItemId: number): Promise<boolean> {
    const current = menu.value;
    const itemId = options.itemId();
    if (!current || !itemId || current.itemId !== itemId || !options.isSupported()
      || !options.canActivate()) {
      return false;
    }
    const node = findEnabledNode([current.root], menuItemId);
    if (!node || node.hasSubmenu || node.children.length > 0) {
      return false;
    }
    const result = await options.run(() => activateTrayMenuItem(itemId, menuItemId, current.revision));
    return result !== undefined;
  }

  onUnmounted(() => {
    generation++;
  });

  return { menu, expandedIds, load, toggleSubmenu, activateNode, reset };
}
