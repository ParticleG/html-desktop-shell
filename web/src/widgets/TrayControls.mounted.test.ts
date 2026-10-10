import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type DOMWrapper, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";
import type { ShellMethod, TrayItem, TrayMenu, TrayMenuNode } from "@html-desktop-shell/shell-api";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import TrayControls from "./TrayControls.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };
const trayMethods: ShellMethod[] = [
  "activateTrayItem", "secondaryActivateTrayItem", "showTrayItemContextMenu",
  "getTrayMenu", "activateTrayMenuItem",
];
let originalWebkit: Window["webkit"];

function item(id: string, overrides: Partial<TrayItem> = {}): TrayItem {
  return {
    id, title: id, status: "Active", category: "ApplicationStatus", isMenu: false, hasMenu: true,
    tooltip: "", icon: null, attentionIcon: null, overlayIcon: null, ...overrides,
  };
}

function node(id: number, label: string, overrides: Partial<TrayMenuNode> = {}): TrayMenuNode {
  return {
    id, label, enabled: true, visible: true, type: "", toggleType: null, toggleState: -1,
    icon: null, children: [], hasSubmenu: false, ...overrides,
  };
}

function menuFor(itemId: string, revision: number, children: TrayMenuNode[]): TrayMenu {
  return { itemId, revision, root: node(0, "", { children }) };
}

function mountControls(items: TrayItem[], methods = trayMethods) {
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.hostInfo = {
    shell: "html-desktop-shell", backend: "wayland-layer-shell", bridgeVersion: 6,
    surface: { kind: "quickPanel", monitorIndex: 0, output: "DP-1" },
  };
  shell.state = { tray: { available: true, items } };
  shell.capabilities = { methods: [...methods] };
  const wrapper = mount(TrayControls, {
    attachTo: document.body,
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return { wrapper, shell };
}

function itemAction(wrapper: VueWrapper, itemId: string, label: string): DOMWrapper<Element> {
  const button = wrapper.get(`[data-tray-id="${itemId}"]`).findAll("button").find((candidate) => candidate.text() === label);
  if (!button) {
    throw new Error(`missing ${label} action for ${itemId}`);
  }
  return button;
}

function menuAction(wrapper: VueWrapper, id: number): Omit<DOMWrapper<Element>, "exists"> {
  return wrapper.get(`[data-menu-id="${id}"]`);
}

async function settle(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

function installHandler(handle: (request: RequestEnvelope) => unknown | Promise<unknown>, requests: RequestEnvelope[]) {
  window.webkit = {
    messageHandlers: {
      shell: {
        async postMessage(request) {
          requests.push(request);
          return JSON.stringify({ id: request.id, ok: true, result: await handle(request) });
        },
      },
    },
  };
}

beforeEach(() => {
  originalWebkit = window.webkit;
});

afterEach(() => {
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
  document.body.replaceChildren();
  if (originalWebkit === undefined) {
    delete window.webkit;
  } else {
    window.webkit = originalWebkit;
  }
});

describe("TrayControls mounted menu behavior", () => {
  test("keeps Passive items in the panel and preserves hidden, disabled, separator and toggle semantics", async () => {
    const requests: RequestEnvelope[] = [];
    const root = menuFor("player", 7, [
      node(1, "<b>Open</b>"),
      node(2, "Hidden action", { visible: false }),
      node(3, "Disabled action", { enabled: false }),
      node(4, "", { type: "separator" }),
      node(5, "Mixed check", { toggleType: "checkmark", toggleState: -1 }),
      node(6, "Selected radio", { toggleType: "radio", toggleState: 1 }),
      node(7, "Disabled submenu", { enabled: false, hasSubmenu: true, children: [node(8, "Disabled descendant")] }),
    ]);
    installHandler((request) => request.method === "getTrayMenu" ? root : request.params, requests);
    const { wrapper } = mountControls([item("player", { status: "Passive", tooltip: "<img src='https://invalid.test/x'>" })]);
    expect(wrapper.get('[data-tray-id="player"]').text()).toContain("<img src='https://invalid.test/x'>");
    expect(wrapper.find("img").exists()).toBe(false);
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();

    expect(wrapper.get('[role="menu"]').attributes("aria-label")).toBe("Menu for player");
    expect(wrapper.text()).not.toContain("Hidden action");
    expect(wrapper.text()).not.toContain("Disabled descendant");
    expect(wrapper.findAll('[role="separator"]')).toHaveLength(1);
    expect(wrapper.find('[data-menu-id="4"]').exists()).toBe(false);
    expect(menuAction(wrapper, 3).attributes("disabled")).toBeDefined();
    expect(menuAction(wrapper, 7).attributes("disabled")).toBeDefined();
    expect(menuAction(wrapper, 5).attributes("role")).toBe("menuitemcheckbox");
    expect(menuAction(wrapper, 5).attributes("aria-checked")).toBe("mixed");
    expect(menuAction(wrapper, 6).attributes("role")).toBe("menuitemradio");
    expect(menuAction(wrapper, 6).attributes("aria-checked")).toBe("true");
    expect(menuAction(wrapper, 1).text()).toBe("<b>Open</b>");
    expect(menuAction(wrapper, 1).find("b").exists()).toBe(false);
    await menuAction(wrapper, 3).trigger("click");
    await menuAction(wrapper, 7).trigger("click");
    await settle();
    expect(requests.filter((request) => request.method === "activateTrayMenuItem")).toEqual([]);

    (menuAction(wrapper, 1).element as HTMLButtonElement).focus();
    await menuAction(wrapper, 1).trigger("keydown", { key: "ArrowDown" });
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("5");
    await menuAction(wrapper, 5).trigger("keydown", { key: "ArrowUp" });
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("1");
    await menuAction(wrapper, 5).trigger("click");
    await settle();
    expect(requests.find((request) => request.method === "activateTrayMenuItem")?.params).toEqual({ itemId: "player", menuItemId: 5, revision: 7 });
    expect(menuAction(wrapper, 5).attributes("aria-checked")).toBe("mixed");
  });

  test("prepares known lazy submenu IDs, replaces the complete root and uses its real revision for nested leaves", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler((request) => {
      const params = request.params as { itemId: string; menuItemId: number };
      if (request.method !== "getTrayMenu") {
        return request.params;
      }
      if (params.menuItemId === 0) {
        return menuFor("player", 40, [node(1, "Initial sibling"), node(10, "Tools", { hasSubmenu: true })]);
      }
      if (params.menuItemId === 10) {
        return menuFor("player", 41, [
          node(1, "Updated sibling"),
          node(10, "Tools", { hasSubmenu: true, children: [node(20, "Advanced", { hasSubmenu: true })] }),
        ]);
      }
      if (params.menuItemId === 20) {
        return menuFor("player", 42, [
          node(1, "Updated sibling"),
          node(10, "Tools", { hasSubmenu: true, children: [
            node(20, "Advanced", { hasSubmenu: true, children: [node(30, "Nested action")] }),
          ] }),
        ]);
      }
      throw new Error(`unexpected submenu ${params.menuItemId}`);
    }, requests);
    const { wrapper } = mountControls([item("player")]);
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();
    expect(menuAction(wrapper, 10).attributes("aria-expanded")).toBe("false");
    expect(wrapper.find('[data-menu-id="20"]').exists()).toBe(false);

    (menuAction(wrapper, 10).element as HTMLButtonElement).focus();
    await menuAction(wrapper, 10).trigger("keydown", { key: "ArrowRight" });
    await settle();
    expect(menuAction(wrapper, 1).text()).toBe("Updated sibling");
    expect(menuAction(wrapper, 10).attributes("aria-expanded")).toBe("true");
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("20");
    await menuAction(wrapper, 20).trigger("keydown", { key: "ArrowRight" });
    await settle();
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("30");
    expect(wrapper.findAll('[role="menu"]')).toHaveLength(3);
    await menuAction(wrapper, 30).trigger("click");
    await settle();
    expect(requests.map((request) => ({ method: request.method, params: request.params }))).toEqual([
      { method: "getTrayMenu", params: { itemId: "player", menuItemId: 0 } },
      { method: "getTrayMenu", params: { itemId: "player", menuItemId: 10 } },
      { method: "getTrayMenu", params: { itemId: "player", menuItemId: 20 } },
      { method: "activateTrayMenuItem", params: { itemId: "player", menuItemId: 30, revision: 42 } },
    ]);

    await menuAction(wrapper, 30).trigger("keydown", { key: "ArrowLeft" });
    await settle();
    expect(wrapper.find('[data-menu-id="30"]').exists()).toBe(false);
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("20");
    expect(requests.filter((request) => request.method === "getTrayMenu")).toHaveLength(3);
  });

  test("shows native stale-menu errors without retries and permits an explicit root refetch", async () => {
    const requests: RequestEnvelope[] = [];
    let reads = 0;
    let events = 0;
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            if (request.method === "getTrayMenu") {
              reads++;
              return JSON.stringify({ id: request.id, ok: true, result: menuFor("player", reads === 1 ? 7 : 19, [node(1, reads === 1 ? "Old action" : "Current action")]) });
            }
            if (request.method === "activateTrayMenuItem" && ++events === 1) {
              return JSON.stringify({ id: request.id, ok: false, error: { code: "TRAY_MENU_STALE", message: "Menu changed; fetch its current layout" } });
            }
            return JSON.stringify({ id: request.id, ok: true, result: request.params });
          },
        },
      },
    };
    const { wrapper } = mountControls([item("player")]);
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();
    await menuAction(wrapper, 1).trigger("click");
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe("TRAY_MENU_STALE: Menu changed; fetch its current layout");
    expect(reads).toBe(1);
    expect(events).toBe(1);

    await wrapper.findAll("button").find((button) => button.text() === "Refresh")!.trigger("click");
    await settle();
    expect(reads).toBe(2);
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(menuAction(wrapper, 1).text()).toBe("Current action");
    await menuAction(wrapper, 1).trigger("click");
    await settle();
    expect(requests.filter((request) => request.method === "activateTrayMenuItem").map((request) => request.params)).toEqual([
      { itemId: "player", menuItemId: 1, revision: 7 },
      { itemId: "player", menuItemId: 1, revision: 19 },
    ]);
  });

  test("item removal invalidates a pending layout and a late result cannot replace another item's menu", async () => {
    const requests: RequestEnvelope[] = [];
    let finish!: (value: unknown) => void;
    const pending = new Promise<unknown>((resolve) => { finish = resolve; });
    installHandler((request) => {
      const params = request.params as { itemId: string };
      return params.itemId === "old" ? pending : menuFor("new", 2, [node(2, "New action")]);
    }, requests);
    const { wrapper, shell } = mountControls([item("old"), item("new")]);
    await itemAction(wrapper, "old", "Show menu").trigger("click");
    await settle();
    expect(wrapper.get('[role="status"]').text()).toBe("Loading…");
    shell.state = { tray: { available: true, items: [item("new")] } };
    await settle();
    expect(wrapper.find(".tray-menu-panel").exists()).toBe(false);
    finish(menuFor("old", 99, [node(99, "Retired action")]));
    await settle();
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Retired action");
    await itemAction(wrapper, "new", "Show menu").trigger("click");
    await settle();
    expect(menuAction(wrapper, 2).text()).toBe("New action");
    expect(wrapper.get('[role="menu"]').attributes("aria-label")).toBe("Menu for new");
    expect(wrapper.text()).not.toContain("Retired action");
  });

  test("never displays a different-item native menu response", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler(() => menuFor("other", 8, [node(8, "Other item's action")]), requests);
    const { wrapper } = mountControls([item("player")]);
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);
    expect(wrapper.text()).not.toContain("Other item's action");
    expect(wrapper.get('[role="alert"]').text()).toContain("different item");
  });

  test("gates each native operation and clears an open menu when menu capability or item support disappears", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler((request) => request.method === "getTrayMenu" ? menuFor("player", 3, [node(1, "Read-only action")]) : request.params, requests);
    const { wrapper, shell } = mountControls([item("player")], ["getTrayMenu", "activateTrayItem"]);
    expect(itemAction(wrapper, "player", "Activate").attributes("disabled")).toBeUndefined();
    expect(itemAction(wrapper, "player", "Secondary activation").attributes("disabled")).toBeDefined();
    expect(itemAction(wrapper, "player", "Native menu").attributes("disabled")).toBeDefined();
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();
    expect(menuAction(wrapper, 1).attributes("disabled")).toBeDefined();
    await menuAction(wrapper, 1).trigger("click");
    await itemAction(wrapper, "player", "Secondary activation").trigger("click");
    await itemAction(wrapper, "player", "Native menu").trigger("click");
    await settle();
    expect(requests.map((request) => request.method)).toEqual(["getTrayMenu"]);

    shell.capabilities = { methods: [...trayMethods] };
    await settle();
    expect(menuAction(wrapper, 1).attributes("disabled")).toBeUndefined();
    shell.capabilities = { methods: ["getTrayMenu", "activateTrayItem"] };
    await settle();
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();
    shell.state = { tray: { available: true, items: [item("player", { hasMenu: false })] } };
    await settle();
    expect(wrapper.find(".tray-menu-panel").exists()).toBe(false);
    expect(wrapper.findAll("button").some((button) => button.text() === "Show menu")).toBe(false);

    shell.state = { tray: { available: true, items: [item("player")] } };
    await settle();
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();
    shell.capabilities = { methods: ["activateTrayItem"] };
    await settle();
    expect(wrapper.find(".tray-menu-panel").exists()).toBe(false);
    expect(itemAction(wrapper, "player", "Show menu").attributes("disabled")).toBeDefined();
    shell.state = { tray: { available: false, reason: "watcher disconnected" } };
    await settle();
    expect(wrapper.find("[data-tray-id]").exists()).toBe(false);
    expect(wrapper.text()).toContain("unavailable");
  });

  test("closing a pending menu discards a late native error instead of exposing it for a subsequent selection", async () => {
    const requests: RequestEnvelope[] = [];
    let finish!: (value: string) => void;
    const pending = new Promise<string>((resolve) => { finish = resolve; });
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            return pending;
          },
        },
      },
    };
    const { wrapper } = mountControls([item("player")]);
    await itemAction(wrapper, "player", "Show menu").trigger("click");
    await settle();
    await wrapper.findAll("button").find((button) => button.text() === "Close menu")!.trigger("click");
    finish(JSON.stringify({ id: requests[0]!.id, ok: false, error: { code: "RETIRED_ITEM", message: "No longer registered" } }));
    await settle();
    expect(wrapper.find(".tray-menu-panel").exists()).toBe(false);
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(itemAction(wrapper, "player", "Show menu").attributes("disabled")).toBeUndefined();
  });

  test("keeps one fixed native-menu anchor per row resolvable while the row controls are busy", async () => {
    const requests: RequestEnvelope[] = [];
    let finish!: (value: string) => void;
    const pending = new Promise<string>((resolve) => { finish = resolve; });
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            return pending;
          },
        },
      },
    };
    const { wrapper } = mountControls([item("player"), item("menu", { isMenu: true })]);
    expect(wrapper.get('[data-tray-id="player"] [data-tray-menu-trigger]').text()).toBe("Native menu");
    expect(wrapper.findAll("[data-tray-menu-trigger]")).toHaveLength(2);

    await wrapper.get('[data-tray-id="player"] [data-tray-menu-trigger]').trigger("click");
    await settle();
    const anchor = wrapper.get('[data-tray-id="player"] [data-tray-menu-trigger]');
    expect(anchor.text()).toBe("Native menu");
    expect(anchor.attributes("disabled")).toBeDefined();

    finish(JSON.stringify({ id: requests[0]!.id, ok: true, result: null }));
    await settle();
    expect(anchor.text()).toBe("Native menu");
    expect(anchor.attributes("disabled")).toBeUndefined();
  });
});
