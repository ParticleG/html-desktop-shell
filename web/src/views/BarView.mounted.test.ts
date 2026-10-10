import { afterEach, beforeEach, describe, expect, jest, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import BarView from "./BarView.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };

function mountBar(output = "DP-2", monitorIndex = 0): VueWrapper {
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.hostInfo = {
    shell: "html-desktop-shell",
    backend: "wayland-layer-shell",
    bridgeVersion: 6,
    surface: { kind: "bar", monitorIndex, output },
  };
  shell.capabilities = {
    methods: [
      "getState",
      "niriFocusWorkspace",
      "setQuickPanelOpen",
      "setDiagnosticsDialogOpen",
    ],
  };
  const wrapper = mount(BarView, {
    attachTo: document.body,
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return wrapper;
}

async function settleLifecycle(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

beforeEach(() => {
  jest.useFakeTimers();
});

afterEach(() => {
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
  jest.clearAllTimers();
  jest.useRealTimers();
  document.body.replaceChildren();
  delete window.webkit;
});

describe("BarView mounted integration", () => {
  test("uses exact surface-control payloads and preserves visible native failures", async () => {
    const requests: RequestEnvelope[] = [];
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            if (request.method === "getState") {
              return JSON.stringify({
                id: request.id,
                ok: true,
                result: {
                  niri: {
                    workspaces: { available: false, items: [] },
                    focusedWindows: {
                      available: true,
                      items: [
                        { output: "DP-2", window: { appId: "Terminal", title: "DP-2 shell" } },
                        { output: "DP-3", window: { appId: "Browser", title: "DP-3 docs" } },
                      ],
                    },
                  },
                },
              });
            }
            if (request.method === "setQuickPanelOpen") {
              return JSON.stringify({ id: request.id, ok: true, result: { open: true } });
            }
            if (request.method === "setDiagnosticsDialogOpen") {
              return JSON.stringify({
                id: request.id,
                ok: false,
                error: { code: "surface_failed", message: "diagnostics construction failed" },
              });
            }
            throw new Error(`unexpected method ${request.method}`);
          },
        },
      },
    };

    const wrapper = mountBar();
    await settleLifecycle();
    const buttons = wrapper.findAll("button");
    await buttons.find((button) => button.text() === "Open Quick Panel")!.trigger("click");
    await buttons.find((button) => button.text() === "Open Diagnostics")!.trigger("click");
    await settleLifecycle();

    expect(wrapper.get("#focused-window").text()).toContain("Terminal — DP-2 shell");
    expect(requests.filter((request) => request.method.startsWith("set"))).toEqual([
      { id: expect.any(String), method: "setQuickPanelOpen", params: { open: true } },
      { id: expect.any(String), method: "setDiagnosticsDialogOpen", params: { open: true } },
    ]);
    expect(wrapper.get("#action-status").text()).toContain("diagnostics open failed");
    expect(wrapper.get("#action-status").attributes("title")).toBe("diagnostics construction failed");
  });

  test("serializes polling and never schedules after cleanup", async () => {
    const requests: RequestEnvelope[] = [];
    let resolveState!: (value: string) => void;
    const pendingState = new Promise<string>((resolve) => {
      resolveState = resolve;
    });
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            return pendingState;
          },
        },
      },
    };

    const wrapper = mountBar();
    expect(requests.filter((request) => request.method === "getState")).toHaveLength(1);
    jest.advanceTimersByTime(5000);
    await settleLifecycle();
    expect(requests.filter((request) => request.method === "getState")).toHaveLength(1);

    wrapper.unmount();
    resolveState(JSON.stringify({ id: requests[0]!.id, ok: true, result: {} }));
    await settleLifecycle();
    jest.advanceTimersByTime(3000);
    await settleLifecycle();

    expect(requests.filter((request) => request.method === "getState")).toHaveLength(1);
  });

  test("continues serialized polling while mounted and clears its own timer", async () => {
    const requests: RequestEnvelope[] = [];
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            return JSON.stringify({ id: request.id, ok: true, result: {} });
          },
        },
      },
    };

    const wrapper = mountBar();
    await settleLifecycle();
    expect(requests).toHaveLength(1);

    jest.advanceTimersByTime(1000);
    await settleLifecycle();
    expect(requests).toHaveLength(2);

    wrapper.unmount();
    jest.advanceTimersByTime(3000);
    await settleLifecycle();
    expect(requests).toHaveLength(2);
  });

  test("keeps each Bar output label stable across global focus changes", async () => {
    let focusedOutput = "DP-2";
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            if (request.method !== "getState") {
              throw new Error(`unexpected method ${request.method}`);
            }
            return JSON.stringify({
              id: request.id,
              ok: true,
              result: {
                niri: {
                  available: true,
                  focusedOutput: { available: true, name: focusedOutput },
                },
              },
            });
          },
        },
      },
    };

    const dp2 = mountBar("DP-2", 0);
    const dp3 = mountBar("DP-3", 1);
    await settleLifecycle();
    expect(dp2.get("#bridge-status").text()).toContain("niri: DP-2");
    expect(dp3.get("#bridge-status").text()).toContain("niri: DP-3");

    focusedOutput = "DP-3";
    jest.advanceTimersByTime(1000);
    await settleLifecycle();
    expect(dp2.get("#bridge-status").text()).toContain("niri: DP-2");
    expect(dp3.get("#bridge-status").text()).toContain("niri: DP-3");
  });
});
