import { afterEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";
import type { SurfaceContext } from "@html-desktop-shell/shell-api";

import { i18n } from "./i18n";
import { installPrimeVue } from "./plugins/primevue";
import SurfaceRoot from "./SurfaceRoot.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };

function mountSurface(surface: SurfaceContext, requests: RequestEnvelope[]): VueWrapper {
  const capabilitiesBySurface = {
    bar: [
      "getHostInfo",
      "getCapabilities",
      "getState",
      "niriFocusWorkspace",
      "setQuickPanelOpen",
      "setDiagnosticsDialogOpen",
    ],
    quickPanel: [
      "getHostInfo",
      "getCapabilities",
      "getState",
      "niriFocusWorkspace",
      "setQuickPanelOpen",
      "setDiagnosticsDialogOpen",
    ],
    workspaceHud: ["getHostInfo", "getCapabilities"],
    diagnosticsDialog: ["getHostInfo", "getCapabilities", "getState", "setDiagnosticsDialogOpen"],
  } as const;
  window.webkit = {
    messageHandlers: {
      shell: {
        postMessage(request) {
          requests.push(request);
          if (request.method === "getHostInfo") {
            return JSON.stringify({
              id: request.id,
              ok: true,
              result: {
                shell: "html-desktop-shell",
                backend: "wayland-layer-shell",
                bridgeVersion: 5,
                surface,
              },
            });
          }
          if (request.method === "getCapabilities") {
            return JSON.stringify({
              id: request.id,
              ok: true,
              result: { methods: capabilitiesBySurface[surface.kind] },
            });
          }
          if (request.method === "getState") {
            return JSON.stringify({
              id: request.id,
              ok: true,
              result: {
                clock: { time: "12:34:56" },
                niri: { available: false, workspaces: { available: false, items: [] } },
                battery: { available: false },
                network: { available: false },
              },
            });
          }
          throw new Error(`unexpected method ${request.method}`);
        },
      },
    },
  };

  const wrapper = mount(SurfaceRoot, {
    attachTo: document.body,
    global: { plugins: [createPinia(), i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return wrapper;
}

function mountHostInfoResult(
  hostInfo: unknown,
  capabilities: unknown = { methods: ["getHostInfo", "getCapabilities"] },
): VueWrapper {
  window.webkit = {
    messageHandlers: {
      shell: {
        postMessage(request) {
          const result = request.method === "getHostInfo" ? hostInfo : capabilities;
          return JSON.stringify({ id: request.id, ok: true, result });
        },
      },
    },
  };

  const wrapper = mount(SurfaceRoot, {
    attachTo: document.body,
    global: { plugins: [createPinia(), i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return wrapper;
}

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

describe("SurfaceRoot mounted routing", () => {
  test("renders every surface variant exhaustively and loads HostInfo once", async () => {
    const cases: Array<{ surface: SurfaceContext; selector: string; text: string }> = [
      {
        surface: { kind: "bar", monitorIndex: 0, output: "DP-1" },
        selector: "#shell-bar",
        text: "HTML Shell",
      },
      {
        surface: { kind: "quickPanel", monitorIndex: 0, output: "DP-1" },
        selector: "#quick-panel",
        text: "Quick Panel",
      },
      {
        surface: {
          kind: "workspaceHud",
          monitorIndex: 0,
          output: "DP-1",
          workspaceIndex: 7,
        },
        selector: "#workspace-hud",
        text: "7",
      },
      {
        surface: { kind: "diagnosticsDialog", monitorIndex: 0, output: "DP-1" },
        selector: "#diagnostics-dialog",
        text: "Diagnostics",
      },
    ];

    for (const item of cases) {
      const requests: RequestEnvelope[] = [];
      const wrapper = mountSurface(item.surface, requests);
      await settleLifecycle();

      expect(wrapper.get(item.selector).text()).toContain(item.text);
      expect(requests.filter((request) => request.method === "getHostInfo")).toHaveLength(1);
      expect(requests.filter((request) => request.method === "getCapabilities")).toHaveLength(1);
      expect(requests.filter((request) => request.method === "getState")).toHaveLength(
        item.surface.kind === "workspaceHud" ? 0 : 1,
      );

      wrapper.unmount();
    }
  });

  test("shows bridge failure without rendering a view for invalid HostInfo", async () => {
    const invalidHostInfo = [
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 3,
        surface: { kind: "bar", monitorIndex: 0, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 5,
        surface: { kind: "unknown", monitorIndex: 0, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 5,
        surface: { kind: "workspaceHud", monitorIndex: -1, output: null, workspaceIndex: 0 },
      },
    ];

    for (const hostInfo of invalidHostInfo) {
      const wrapper = mountHostInfoResult(hostInfo);
      await settleLifecycle();

      expect(wrapper.get("#surface-loading").text()).toContain("invalid native bridge result");
      expect(wrapper.find("#shell-bar").exists()).toBe(false);
      expect(wrapper.find("#quick-panel").exists()).toBe(false);
      expect(wrapper.find("#workspace-hud").exists()).toBe(false);
      expect(wrapper.find("#diagnostics-dialog").exists()).toBe(false);

      wrapper.unmount();
    }
  });

  test("shows bridge failure when capabilities are malformed", async () => {
    const wrapper = mountHostInfoResult(
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 5,
        surface: { kind: "bar", monitorIndex: 0, output: null },
      },
      { methods: ["getHostInfo", "genericTrigger"] },
    );
    await settleLifecycle();

    expect(wrapper.get("#surface-loading").text()).toContain(
      "Capabilities.methods contains an unsupported method",
    );
    expect(wrapper.find("#shell-bar").exists()).toBe(false);
  });
});
