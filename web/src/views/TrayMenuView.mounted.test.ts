import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type DOMWrapper, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";
import type { ShellMethod, TrayMenu, TrayMenuNode } from "@html-desktop-shell/shell-api";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import TrayMenuView from "./TrayMenuView.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };
const trayItemId = "item-7";
const menuMethods: ShellMethod[] = ["getTrayMenu", "activateTrayMenuItem", "closeTrayMenu"];
let originalWebkit: Window["webkit"];

function node(id: number, label: string, overrides: Partial<TrayMenuNode> = {}): TrayMenuNode {
  return {
    id, label, enabled: true, visible: true, type: "", toggleType: null, toggleState: -1,
    icon: null, children: [], hasSubmenu: false, ...overrides,
  };
}

function menuFor(revision: number, children: TrayMenuNode[]): TrayMenu {
  return { itemId: trayItemId, revision, root: node(0, "", { children }) };
}

function mountMenu(methods: ShellMethod[] = menuMethods): VueWrapper {
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.hostInfo = {
    shell: "html-desktop-shell",
    backend: "wayland-layer-shell",
    bridgeVersion: 6,
    surface: { kind: "trayMenu", monitorIndex: 0, output: "DP-1", trayItemId },
  };
  shell.capabilities = { methods: [...methods] };
  const wrapper = mount(TrayMenuView, {
    attachTo: document.body,
    props: { surface: { kind: "trayMenu", monitorIndex: 0, output: "DP-1", trayItemId } },
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return wrapper;
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
          try {
            return JSON.stringify({ id: request.id, ok: true, result: await handle(request) });
          } catch (cause) {
            const failure = cause as { code?: string; message?: string };
            return JSON.stringify({
              id: request.id,
              ok: false,
              error: { code: failure.code ?? "native_failed", message: failure.message ?? "failed" },
            });
          }
        },
      },
    },
  };
}

function fail(code: string, message: string): never {
  throw Object.assign(new Error(message), { code });
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

describe("TrayMenuView native menu surface", () => {
  test("fetches only the context-bound item menu, never polls state, and escapes literal labels", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler((request) => {
      if (request.method === "getTrayMenu") {
        return menuFor(5, [node(1, "<b>Open</b>")]);
      }
      return fail("unexpected_method", request.method);
    }, requests);
    const wrapper = mountMenu();
    await settle();

    expect(requests.map((request) => ({ method: request.method, params: request.params }))).toEqual([
      { method: "getTrayMenu", params: { itemId: trayItemId, menuItemId: 0 } },
    ]);
    expect(wrapper.find("#tray-menu-content").exists()).toBe(true);
    expect(wrapper.get('[role="menu"]').attributes("aria-label")).toBe(`Menu for ${trayItemId}`);
    expect(menuAction(wrapper, 1).text()).toBe("<b>Open</b>");
    expect(menuAction(wrapper, 1).find("b").exists()).toBe(false);
    expect(wrapper.find("img, [role='menu'] [role='separator']").exists()).toBe(false);
    expect(document.body.querySelector(".p-menu, .p-tieredmenu, .p-contextmenu")).toBeNull();
    expect(requests.filter((request) => request.method === "getState")).toEqual([]);
  });

  test("prepares lazy submenus, replaces the whole root, activates leaves with the response revision", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler((request) => {
      const params = request.params as { menuItemId: number };
      if (request.method !== "getTrayMenu") {
        return request.params;
      }
      if (params.menuItemId === 0) {
        return menuFor(40, [node(1, "Initial sibling"), node(10, "Tools", { hasSubmenu: true })]);
      }
      if (params.menuItemId === 10) {
        return menuFor(41, [node(1, "Updated sibling"), node(10, "Tools", { hasSubmenu: true, children: [node(20, "Advanced", { hasSubmenu: true })] })]);
      }
      if (params.menuItemId === 20) {
        return menuFor(42, [node(10, "Tools", { hasSubmenu: true, children: [node(20, "Advanced", { hasSubmenu: true, children: [node(30, "Nested action")] })] })]);
      }
      return fail("unexpected_submenu", `unexpected submenu ${params.menuItemId}`);
    }, requests);
    const wrapper = mountMenu();
    await settle();
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("1");

    (menuAction(wrapper, 10).element as HTMLButtonElement).focus();
    await menuAction(wrapper, 10).trigger("keydown", { key: "ArrowRight" });
    await settle();
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("20");
    expect(menuAction(wrapper, 1).text()).toBe("Updated sibling");

    await menuAction(wrapper, 20).trigger("keydown", { key: "ArrowRight" });
    await settle();
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("30");
    expect(wrapper.findAll('[role="menu"]')).toHaveLength(3);

    await menuAction(wrapper, 30).trigger("click");
    await settle();
    expect(requests.map((request) => ({ method: request.method, params: request.params }))).toEqual([
      { method: "getTrayMenu", params: { itemId: trayItemId, menuItemId: 0 } },
      { method: "getTrayMenu", params: { itemId: trayItemId, menuItemId: 10 } },
      { method: "getTrayMenu", params: { itemId: trayItemId, menuItemId: 20 } },
      { method: "activateTrayMenuItem", params: { itemId: trayItemId, menuItemId: 30, revision: 42 } },
      { method: "closeTrayMenu", params: {} },
    ]);

    await menuAction(wrapper, 30).trigger("keydown", { key: "ArrowLeft" });
    await settle();
    expect(wrapper.find('[data-menu-id="30"]').exists()).toBe(false);
    expect((document.activeElement as HTMLElement).dataset.menuId).toBe("20");
  });

  test("closes only through the close-only capability and consumes one close per Escape", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler((request) => request.method === "getTrayMenu"
      ? menuFor(9, [node(1, "Action")])
      : null, requests);
    const wrapper = mountMenu();
    await settle();

    await wrapper.findAll("button").find((button) => button.text() === "Close menu")!.trigger("click");
    await settle();
    expect(requests.filter((request) => request.method === "closeTrayMenu").map((request) => request.params)).toEqual([{}]);

    await menuAction(wrapper, 1).trigger("keydown", { key: "Escape" });
    await settle();
    expect(requests.filter((request) => request.method === "closeTrayMenu").map((request) => request.params)).toEqual([{}, {}]);
    expect(requests.filter((request) => request.method === "getState")).toEqual([]);
  });

  test("shows wrapped load and stale action errors, retries only on explicit refresh", async () => {
    const requests: RequestEnvelope[] = [];
    let reads = 0;
    let activations = 0;
    installHandler((request) => {
      if (request.method === "getTrayMenu") {
        reads++;
        if (reads === 1) {
          return fail("TRAY_MENU_UNAVAILABLE", "This tray item does not export a DBusMenu");
        }
        return menuFor(19, [node(1, "Current action")]);
      }
      if (request.method === "activateTrayMenuItem" && ++activations === 1) {
        return fail("TRAY_MENU_STALE", "Menu changed; fetch its current layout");
      }
      return request.params;
    }, requests);
    const wrapper = mountMenu();
    await settle();
    expect(reads).toBe(1);
    expect(wrapper.get('[role="alert"]').text()).toBe("TRAY_MENU_UNAVAILABLE: This tray item does not export a DBusMenu");
    expect(wrapper.find('[role="menu"]').exists()).toBe(false);

    await wrapper.findAll("button").find((button) => button.text() === "Refresh")!.trigger("click");
    await settle();
    expect(reads).toBe(2);
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(menuAction(wrapper, 1).text()).toBe("Current action");

    await menuAction(wrapper, 1).trigger("click");
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe("TRAY_MENU_STALE: Menu changed; fetch its current layout");
    expect(activations).toBe(1);
    expect(requests.filter((request) => request.method === "activateTrayMenuItem").map((request) => request.params)).toEqual([
      { itemId: trayItemId, menuItemId: 1, revision: 19 },
    ]);
    expect(requests.filter((request) => request.method === "closeTrayMenu")).toEqual([]);

    await menuAction(wrapper, 1).trigger("click");
    await settle();
    expect(activations).toBe(2);
    expect(requests.filter((request) => request.method === "closeTrayMenu").map((request) => request.params)).toEqual([{}]);
  });

  test("retires pending work on unmount so a late native result never resurfaces", async () => {
    const requests: RequestEnvelope[] = [];
    let finish!: (value: unknown) => void;
    const pending = new Promise<unknown>((resolve) => { finish = resolve; });
    installHandler(() => pending, requests);
    const wrapper = mountMenu();
    await settle();
    expect(wrapper.get('[role="status"]').text()).toBe("Loading…");

    wrapper.unmount();
    expect(document.body.querySelector("#tray-menu")).toBeNull();
    finish(menuFor(77, [node(7, "Late action")]));
    await settle();

    expect(document.body.querySelector("#tray-menu")).toBeNull();
    expect(document.body.textContent).not.toContain("Late action");
  });

  test("gates refresh, activation and close on the exact scoped capabilities", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler((request) => request.method === "getTrayMenu"
      ? menuFor(3, [node(1, "Read-only action")])
      : request.params, requests);
    const wrapper = mountMenu(["getTrayMenu"]);
    await settle();

    expect(menuAction(wrapper, 1).attributes("disabled")).toBeDefined();
    await menuAction(wrapper, 1).trigger("click");
    expect(wrapper.findAll("button").find((button) => button.text() === "Close menu")!.attributes("disabled")).toBeDefined();
    await wrapper.get("#tray-menu").trigger("keydown", { key: "Escape" });
    await settle();
    expect(requests.map((request) => request.method)).toEqual(["getTrayMenu"]);

    const closeOnly = mountMenu(["closeTrayMenu"]);
    await settle();
    expect(closeOnly.get("#tray-menu-content").text()).toContain("unavailable");
    expect(closeOnly.findAll("button").find((button) => button.text() === "Refresh")!.attributes("disabled")).toBeDefined();
    expect(requests.map((request) => request.method)).toEqual(["getTrayMenu"]);
  });
});
