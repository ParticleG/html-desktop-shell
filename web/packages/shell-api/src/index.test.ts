import { afterEach, describe, expect, test } from "bun:test";

import {
  getCapabilities,
  getHostInfo,
  NativeBridgeError,
  setDiagnosticsDialogOpen,
} from "./index";
import type { HostInfo } from "./index";

type PostedRequest = {
  id: string;
  method: string;
  params: unknown;
};
let priorWebkitDescriptor: PropertyDescriptor | undefined;
let handlerInstalled = false;

function installHandler(postMessage: (request: PostedRequest) => string): void {
  if (!handlerInstalled) {
    priorWebkitDescriptor = Object.getOwnPropertyDescriptor(window, "webkit");
  }
  handlerInstalled = true;
  Object.defineProperty(window, "webkit", {
    configurable: true,
    value: {
      messageHandlers: {
        shell: { postMessage },
      },
    },
  });
}

afterEach(() => {
  if (!handlerInstalled) {
    return;
  }
  if (priorWebkitDescriptor) {
    Object.defineProperty(window, "webkit", priorWebkitDescriptor);
  } else {
    delete window.webkit;
  }
  priorWebkitDescriptor = undefined;
  handlerInstalled = false;
});

describe("shell API contract", () => {
  test("validates every exact v6 surface shape", async () => {
    const surfaces: HostInfo["surface"][] = [
      { kind: "bar", monitorIndex: 0, output: null },
      { kind: "quickPanel", monitorIndex: 1, output: "DP-1" },
      { kind: "workspaceHud", monitorIndex: 2, output: "HDMI-A-1", workspaceIndex: 1 },
      { kind: "diagnosticsDialog", monitorIndex: 0xffff_ffff, output: null },
      { kind: "trayMenu", monitorIndex: 3, output: "DP-3", trayItemId: "item-7" },
    ];
    let index = 0;
    installHandler((request) => JSON.stringify({
      id: request.id,
      ok: true,
      result: {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: surfaces[index++],
      },
    }));

    for (const surface of surfaces) {
      await expect(getHostInfo()).resolves.toEqual({
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface,
      });
    }
  });

  test("rejects legacy, unknown, and malformed HostInfo results", async () => {
    const invalidResults = [
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 5,
        surface: { kind: "bar", monitorIndex: 0, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "notification", monitorIndex: 0, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "bar", monitorIndex: -1, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "quickPanel", monitorIndex: 0.5, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "workspaceHud", monitorIndex: 0, output: null, workspaceIndex: 0 },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "diagnosticsDialog", monitorIndex: 0, output: 7 },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "bar", monitorIndex: 0, output: null, workspaceIndex: 1 },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "trayMenu", monitorIndex: 0, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "trayMenu", monitorIndex: 0, output: null, trayItemId: "" },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "trayMenu", monitorIndex: 0, output: null, trayItemId: 4 },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 6,
        surface: { kind: "trayMenu", monitorIndex: 0, output: "DP-1", trayItemId: "item-1", workspaceIndex: 2 },
      },
    ];
    let index = 0;
    installHandler((request) => JSON.stringify({
      id: request.id,
      ok: true,
      result: invalidResults[index++],
    }));

    for (const _invalidResult of invalidResults) {
      await expect(getHostInfo()).rejects.toThrow("invalid native bridge result");
    }
  });

  test("rejects an unsupported capability but accepts closeTrayMenu", async () => {
    installHandler((request) => JSON.stringify({
      id: request.id,
      ok: true,
      result: { methods: ["getHostInfo", "closeTrayMenu"] },
    }));
    await expect(getCapabilities()).resolves.toEqual({ methods: ["getHostInfo", "closeTrayMenu"] });

    installHandler((request) => JSON.stringify({
      id: request.id,
      ok: true,
      result: { methods: ["getHostInfo", "genericTrigger"] },
    }));
    await expect(getCapabilities()).rejects.toThrow("Capabilities.methods contains an unsupported method");
  });


  test("preserves native error code and message", async () => {
    installHandler((request) => JSON.stringify({
      id: request.id,
      ok: false,
      error: { code: "surface_failed", message: "dialog construction failed" },
    }));

    let caught: unknown;
    try {
      await setDiagnosticsDialogOpen(true);
    } catch (error) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(NativeBridgeError);
    expect(caught).toMatchObject({
      name: "NativeBridgeError",
      code: "surface_failed",
      message: "dialog construction failed",
    });
  });
});
