import { afterEach, describe, expect, test } from "bun:test";

import {
  focusWorkspace,
  getCapabilities,
  getHostInfo,
  NativeBridgeError,
  setDiagnosticsDialogOpen,
  setQuickPanelOpen,
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
  priorWebkitDescriptor = Object.getOwnPropertyDescriptor(window, "webkit");
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

describe("shell API Bridge v4 contract", () => {
  test("posts exact workspace and surface-control payloads", async () => {
    const requests: PostedRequest[] = [];
    installHandler((request) => {
      requests.push(request);
      if (request.method === "niriFocusWorkspace") {
        return JSON.stringify({ id: request.id, ok: true, result: { workspaceIndex: 2 } });
      }
      return JSON.stringify({ id: request.id, ok: true, result: request.params });
    });

    await focusWorkspace(2);
    await setQuickPanelOpen(true);
    await setQuickPanelOpen(false);
    await setDiagnosticsDialogOpen(true);
    await setDiagnosticsDialogOpen(false);

    expect(requests).toEqual([
      { id: expect.any(String), method: "niriFocusWorkspace", params: { workspaceIndex: 2 } },
      { id: expect.any(String), method: "setQuickPanelOpen", params: { open: true } },
      { id: expect.any(String), method: "setQuickPanelOpen", params: { open: false } },
      { id: expect.any(String), method: "setDiagnosticsDialogOpen", params: { open: true } },
      { id: expect.any(String), method: "setDiagnosticsDialogOpen", params: { open: false } },
    ]);
    for (const request of requests) {
      expect(request.id).toMatch(/^\d+$/);
    }
  });

  test("exposes the Bridge v4 discriminated surface context", async () => {
    const hostInfo: HostInfo = {
      shell: "html-desktop-shell",
      backend: "wayland-layer-shell",
      bridgeVersion: 4,
      surface: {
        kind: "workspaceHud",
        monitorIndex: 0,
        output: "DP-1",
        workspaceIndex: 7,
      },
    };
    installHandler((request) => JSON.stringify({ id: request.id, ok: true, result: hostInfo }));

    const result = await getHostInfo();
    expect(result.bridgeVersion).toBe(4);
    expect(result.surface).toEqual({
      kind: "workspaceHud",
      monitorIndex: 0,
      output: "DP-1",
      workspaceIndex: 7,
    });
  });

  test("validates every exact Bridge v4 surface shape", async () => {
    const surfaces: HostInfo["surface"][] = [
      { kind: "bar", monitorIndex: 0, output: null },
      { kind: "quickPanel", monitorIndex: 1, output: "DP-1" },
      { kind: "workspaceHud", monitorIndex: 2, output: "HDMI-A-1", workspaceIndex: 1 },
      { kind: "diagnosticsDialog", monitorIndex: 0xffff_ffff, output: null },
    ];
    let index = 0;
    installHandler((request) => JSON.stringify({
      id: request.id,
      ok: true,
      result: {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: surfaces[index++],
      },
    }));

    for (const surface of surfaces) {
      await expect(getHostInfo()).resolves.toEqual({
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface,
      });
    }
  });

  test("rejects legacy, unknown, and malformed HostInfo results", async () => {
    const invalidResults = [
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 3,
        surface: { kind: "bar", monitorIndex: 0, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: { kind: "notification", monitorIndex: 0, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: { kind: "bar", monitorIndex: -1, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: { kind: "quickPanel", monitorIndex: 0.5, output: null },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: { kind: "workspaceHud", monitorIndex: 0, output: null, workspaceIndex: 0 },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: { kind: "diagnosticsDialog", monitorIndex: 0, output: 7 },
      },
      {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: { kind: "bar", monitorIndex: 0, output: null, workspaceIndex: 1 },
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

  test("rejects malformed capabilities", async () => {
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
