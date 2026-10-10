import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";
import type { ShellIcon, ShellMethod, TrayItem } from "@html-desktop-shell/shell-api";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import TrayBarWidget from "./TrayBarWidget.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };
const trayMethods: ShellMethod[] = [
  "activateTrayItem", "secondaryActivateTrayItem", "showTrayItemContextMenu", "scrollTrayItem",
];
let originalWebkit: Window["webkit"];
let originalCanvasContext: PropertyDescriptor | undefined;

function item(id: string, overrides: Partial<TrayItem> = {}): TrayItem {
  return {
    id, title: id, status: "Active", category: "ApplicationStatus", isMenu: false, hasMenu: true,
    tooltip: "", icon: null, attentionIcon: null, overlayIcon: null, ...overrides,
  };
}

function pixel(red: number): ShellIcon {
  return { width: 1, height: 1, rgba: btoa(String.fromCharCode(red, 0, 0, 255)) };
}

function mountWidget(items: TrayItem[], methods = trayMethods) {
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.hostInfo = {
    shell: "html-desktop-shell", backend: "wayland-layer-shell", bridgeVersion: 6,
    surface: { kind: "bar", monitorIndex: 0, output: "DP-1" },
  };
  shell.state = { tray: { available: true, items } };
  shell.capabilities = { methods: [...methods] };
  const wrapper = mount(TrayBarWidget, {
    attachTo: document.body,
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return { wrapper, shell };
}

async function settle(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

function installHandler(requests: RequestEnvelope[]) {
  window.webkit = {
    messageHandlers: {
      shell: {
        postMessage(request) {
          requests.push(request);
          return JSON.stringify({ id: request.id, ok: true, result: request.params });
        },
      },
    },
  };
}

beforeEach(() => {
  originalWebkit = window.webkit;
  originalCanvasContext = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, "getContext");
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
  if (originalCanvasContext) {
    Object.defineProperty(HTMLCanvasElement.prototype, "getContext", originalCanvasContext);
  } else {
    Reflect.deleteProperty(HTMLCanvasElement.prototype, "getContext");
  }
});

describe("TrayBarWidget native-only interactions", () => {
  test("hides Passive items, paints attention and overlay pixels, and keeps untrusted labels plain", async () => {
    const painted: number[][] = [];
    Object.defineProperty(HTMLCanvasElement.prototype, "getContext", {
      configurable: true,
      value() {
        return {
          createImageData(width: number, height: number) {
            return { data: new Uint8ClampedArray(width * height * 4) };
          },
          putImageData(image: { data: Uint8ClampedArray }) {
            painted.push([...image.data]);
          },
        };
      },
    });
    const { wrapper, shell } = mountWidget([
      item("passive", { status: "Passive" }),
      item("attention", {
        status: "NeedsAttention", title: "<script>alert(1)</script>", tooltip: "<b>Untrusted tooltip</b>",
        icon: pixel(11), attentionIcon: pixel(22), overlayIcon: pixel(33),
      }),
    ]);
    await settle();
    expect(wrapper.findAll("button")).toHaveLength(1);
    const button = wrapper.get('[data-tray-id="attention"]');
    expect(button.attributes("title")).toBe("<b>Untrusted tooltip</b>");
    expect(button.attributes("aria-label")).toBe("<script>alert(1)</script>");
    expect(painted).toEqual([[22, 0, 0, 255], [33, 0, 0, 255]]);
    expect(wrapper.find("script, b, img, a, [role='menu']").exists()).toBe(false);

    shell.state = { tray: { available: true, items: [item("attention", { status: "Passive" })] } };
    await settle();
    expect(wrapper.find("button").exists()).toBe(false);
    shell.state = { tray: { available: false, reason: "host disconnected" } };
    await settle();
    expect(wrapper.find("button").exists()).toBe(false);
  });

  test("distinguishes primary, menu-only, native context, middle activation and both scroll axes without DOM popups", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler(requests);
    const { wrapper } = mountWidget([item("player"), item("menu", { isMenu: true })]);
    const player = wrapper.get('[data-tray-id="player"]');
    await player.trigger("click");
    await settle();
    await wrapper.get('[data-tray-id="menu"]').trigger("click");
    await settle();
    await player.trigger("contextmenu");
    await settle();
    await player.trigger("auxclick", { button: 1 });
    await settle();
    await player.trigger("auxclick", { button: 2 });
    await settle();
    await player.trigger("wheel", { deltaX: 0, deltaY: 25 });
    await settle();
    await player.trigger("wheel", { deltaX: -0.75, deltaY: 0 });
    await settle();
    await player.trigger("wheel", { deltaX: 4, deltaY: -9 });
    await settle();
    expect(requests.map((request) => ({ method: request.method, params: request.params }))).toEqual([
      { method: "activateTrayItem", params: { itemId: "player" } },
      { method: "showTrayItemContextMenu", params: { itemId: "menu" } },
      { method: "showTrayItemContextMenu", params: { itemId: "player" } },
      { method: "secondaryActivateTrayItem", params: { itemId: "player" } },
      { method: "scrollTrayItem", params: { itemId: "player", delta: 25, orientation: "vertical" } },
      { method: "scrollTrayItem", params: { itemId: "player", delta: -1, orientation: "horizontal" } },
      { method: "scrollTrayItem", params: { itemId: "player", delta: 4, orientation: "horizontal" } },
      { method: "scrollTrayItem", params: { itemId: "player", delta: -9, orientation: "vertical" } },
    ]);
    expect(wrapper.find("[role='menu']").exists()).toBe(false);
    expect(document.body.querySelector(".p-menu, .p-tieredmenu, .p-contextmenu")).toBeNull();
  });

  test("handles contextual keyboard shortcuts without taking over bare F10", async () => {
    const requests: RequestEnvelope[] = [];
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            return JSON.stringify({ id: request.id, ok: true, result: null });
          },
        },
      },
    };
    const { wrapper } = mountWidget([item("player")]);
    const button = wrapper.get('[data-tray-id="player"]').element;
    const bareF10 = new KeyboardEvent("keydown", { key: "F10", bubbles: true, cancelable: true });
    button.dispatchEvent(bareF10);
    await settle();
    expect(bareF10.defaultPrevented).toBe(false);
    expect(requests).toEqual([]);

    for (const key of [
      { key: "F10", shiftKey: true },
      { key: "ContextMenu" },
    ]) {
      const event = new KeyboardEvent("keydown", { ...key, bubbles: true, cancelable: true });
      button.dispatchEvent(event);
      await settle();
      expect(event.defaultPrevented).toBe(true);
      expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    }
    expect(requests.map((request) => request.method)).toEqual([
      "showTrayItemContextMenu", "showTrayItemContextMenu",
    ]);
  });

  test("does not fall back to Activate for menu-only items and guards unavailable operations", async () => {
    const requests: RequestEnvelope[] = [];
    installHandler(requests);
    const { wrapper, shell } = mountWidget([item("player"), item("menu", { isMenu: true })], ["activateTrayItem"]);
    expect(wrapper.get('[data-tray-id="menu"]').attributes("disabled")).toBeDefined();
    await wrapper.get('[data-tray-id="menu"]').trigger("click");
    await wrapper.get('[data-tray-id="player"]').trigger("contextmenu");
    await wrapper.get('[data-tray-id="player"]').trigger("auxclick", { button: 1 });
    await wrapper.get('[data-tray-id="player"]').trigger("wheel", { deltaX: 10, deltaY: 10 });
    await settle();
    expect(requests).toEqual([]);
    shell.capabilities = { methods: [] };
    await settle();
    expect(wrapper.get('[data-tray-id="player"]').attributes("disabled")).toBeDefined();
    await wrapper.get('[data-tray-id="player"]').trigger("click");
    await settle();
    expect(requests).toEqual([]);
  });

  test("serializes pending actions, exposes structured native errors and clears them on a new explicit action", async () => {
    const requests: RequestEnvelope[] = [];
    let finish!: (value: string) => void;
    const pending = new Promise<string>((resolve) => { finish = resolve; });
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            if (request.method === "activateTrayItem") {
              return pending;
            }
            return JSON.stringify({ id: request.id, ok: true, result: request.params });
          },
        },
      },
    };
    const { wrapper } = mountWidget([item("player")]);
    const button = wrapper.get('[data-tray-id="player"]');
    await button.trigger("click");
    await settle();
    expect(button.attributes("disabled")).toBeDefined();
    await button.trigger("contextmenu");
    await button.trigger("auxclick", { button: 1 });
    await button.trigger("wheel", { deltaY: 15 });
    expect(requests.map((request) => request.method)).toEqual(["activateTrayItem"]);
    finish(JSON.stringify({ id: requests[0]!.id, ok: false, error: { code: "TRAY_ACTION_FAILED", message: "Application rejected activation" } }));
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe("TRAY_ACTION_FAILED: Application rejected activation");
    expect(button.attributes("disabled")).toBeUndefined();
    await button.trigger("contextmenu");
    await settle();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(requests.map((request) => request.method)).toEqual(["activateTrayItem", "showTrayItemContextMenu"]);
  });
});
