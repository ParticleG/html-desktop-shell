import { afterEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";
import type { ShellMethod } from "@html-desktop-shell/shell-api";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import DiagnosticsDialogView from "./DiagnosticsDialogView.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };

function mountDiagnostics(methods: ShellMethod[]) {
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.hostInfo = {
    shell: "html-desktop-shell",
    backend: "wayland-layer-shell",
    bridgeVersion: 6,
    surface: { kind: "diagnosticsDialog", monitorIndex: 1, output: "DP-1" },
  };
  shell.capabilities = { methods };
  const wrapper = mount(DiagnosticsDialogView, {
    attachTo: document.body,
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return { wrapper, shell };
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

describe("DiagnosticsDialogView mounted behavior", () => {
  test("renders only host, capabilities, and unavailable provider status when closure is unavailable", async () => {
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            return JSON.stringify({
              id: request.id,
              ok: true,
              result: {
                niri: { available: false },
                battery: { available: false },
                network: { available: false },
              },
            });
          },
        },
      },
    };

    const { wrapper } = mountDiagnostics(["getHostInfo", "getCapabilities", "getState"]);
    await settleLifecycle();

    expect(wrapper.text()).toContain("diagnostics close unavailable");
    expect(wrapper.text()).toContain("niriunavailable");
    expect(wrapper.text()).toContain("Batteryunavailable");
    expect(wrapper.text()).toContain("Networkunavailable");
    expect(wrapper.find("button").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("niriFocusWorkspace");
  });

  test("closes with the exact authorized payload", async () => {
    const requests: RequestEnvelope[] = [];
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            requests.push(request);
            if (request.method === "getState") {
              return JSON.stringify({ id: request.id, ok: true, result: {} });
            }
            if (request.method === "setDiagnosticsDialogOpen") {
              return JSON.stringify({ id: request.id, ok: true, result: { open: false } });
            }
            throw new Error(`unexpected method ${request.method}`);
          },
        },
      },
    };

    const { wrapper } = mountDiagnostics([
      "getHostInfo",
      "getCapabilities",
      "getState",
      "setDiagnosticsDialogOpen",
    ]);
    await settleLifecycle();
    await wrapper.get("button").trigger("click");
    await settleLifecycle();

    expect(requests.filter((request) => request.method === "setDiagnosticsDialogOpen")).toEqual([
      { id: expect.any(String), method: "setDiagnosticsDialogOpen", params: { open: false } },
    ]);
  });

  test("renders exact provider reasons and recovers when providers become available", async () => {
    let available = false;
    window.webkit = {
      messageHandlers: {
        shell: {
          postMessage(request) {
            return JSON.stringify({
              id: request.id,
              ok: true,
              result: available
                ? {
                    niri: { available: true, reason: "stale niri reason" },
                    battery: { available: true, reason: "stale battery reason" },
                    network: { available: true, reason: "stale network reason" },
                  }
                : {
                    niri: { available: false, reason: "niri socket missing" },
                    battery: { available: false, reason: "no power supply" },
                    network: { available: false, reason: "netlink offline" },
                  },
            });
          },
        },
      },
    };

    const { wrapper, shell } = mountDiagnostics(["getHostInfo", "getCapabilities", "getState"]);
    await settleLifecycle();

    expect(wrapper.text()).toContain("niriniri socket missing");
    expect(wrapper.text()).toContain("Batteryno power supply");
    expect(wrapper.text()).toContain("Networknetlink offline");

    available = true;
    await shell.refreshState();
    await settleLifecycle();

    expect(wrapper.text()).toContain("niriavailable");
    expect(wrapper.text()).toContain("Batteryavailable");
    expect(wrapper.text()).toContain("Networkavailable");
    expect(wrapper.text()).not.toContain("stale niri reason");
    expect(wrapper.text()).not.toContain("niri socket missing");
  });
});
