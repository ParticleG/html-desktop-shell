import { afterEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import QuickPanelView from "./QuickPanelView.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };

async function settleLifecycle(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

afterEach(() => {
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
  document.body.replaceChildren();
  delete window.webkit;
});

describe("QuickPanelView mounted interactions", () => {
  test("shows provider workspaces, focuses by workspace index, and exposes explicit close actions", async () => {
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
                    available: true,
                    workspaces: {
                      available: true,
                      items: [
                        { id: 41, index: 2, output: "DP-3", isActive: true, isFocused: true },
                        { id: 99, index: 8, output: "DP-2", isActive: false, isFocused: false },
                      ],
                    },
                    focusedWindows: {
                      available: true,
                      items: [
                        { output: "DP-2", window: { appId: "Terminal", title: "DP-2 shell" } },
                        { output: "DP-3", window: { appId: "Browser", title: "DP-3 docs" } },
                      ],
                    },
                  },
                  battery: { available: false },
                  network: { available: false },
                },
              });
            }
            if (request.method === "niriFocusWorkspace") {
              return JSON.stringify({ id: request.id, ok: true, result: { workspaceIndex: 2 } });
            }
            if (request.method === "setQuickPanelOpen") {
              return JSON.stringify({ id: request.id, ok: true, result: { open: false } });
            }
            if (request.method === "setDiagnosticsDialogOpen") {
              return JSON.stringify({ id: request.id, ok: true, result: { open: true } });
            }
            throw new Error(`unexpected method ${request.method}`);
          },
        },
      },
    };

    const pinia = createPinia();
    const shell = useShellStore(pinia);
    shell.hostInfo = {
      shell: "html-desktop-shell",
      backend: "wayland-layer-shell",
      bridgeVersion: 4,
      surface: { kind: "quickPanel", monitorIndex: 0, output: "DP-3" },
    };
    shell.capabilities = {
      methods: [
        "getState",
        "niriFocusWorkspace",
        "setQuickPanelOpen",
        "setDiagnosticsDialogOpen",
      ],
    };
    const wrapper = mount(QuickPanelView, {
      attachTo: document.body,
      global: { plugins: [pinia, i18n, primeVuePlugin] },
    });
    mounted.push(wrapper);
    await settleLifecycle();

    expect(wrapper.findAll(".workspace-item")).toHaveLength(1);
    expect(wrapper.get("#focused-window").text()).toContain("Browser — DP-3 docs");
    await wrapper.get(".workspace-item").trigger("pointerdown", { button: 0 });
    await wrapper.findAll("button").find((button) => button.text() === "Close Quick Panel")!.trigger("click");
    await wrapper.findAll("button").find((button) => button.text() === "Open Diagnostics")!.trigger("click");
    await settleLifecycle();

    expect(requests.filter((request) => request.method !== "getState")).toEqual([
      { id: expect.any(String), method: "niriFocusWorkspace", params: { workspaceIndex: 2 } },
      { id: expect.any(String), method: "setQuickPanelOpen", params: { open: false } },
      { id: expect.any(String), method: "setDiagnosticsDialogOpen", params: { open: true } },
    ]);
  });
});
