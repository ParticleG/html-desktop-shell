import { describe, expect, test } from "bun:test";
import type { ShellState } from "@html-desktop-shell/shell-api";

import {
  batteryText,
  bridgeStatusText,
  focusedWindowText,
  networkText,
  visibleWorkspaces,
  workspaceLabel,
} from "./view-model";

describe("visibleWorkspaces", () => {
  const state: ShellState = {
    niri: {
      workspaces: {
        available: true,
        items: [
          { id: 1, index: 1, output: "eDP-1" },
          { id: 2, index: 2, output: "DP-2" },
        ],
      },
    },
  };

  test("filters by panel output", () => {
    expect(visibleWorkspaces(state, "eDP-1").map((workspace) => workspace.index)).toEqual([1]);
  });

  test("returns all workspaces without panel output", () => {
    expect(visibleWorkspaces(state, null).map((workspace) => workspace.index)).toEqual([1, 2]);
    expect(visibleWorkspaces(state, "").map((workspace) => workspace.index)).toEqual([1, 2]);
  });
});

describe("workspaceLabel", () => {
  test("uses name, index, id, and fallback in order", () => {
    expect(workspaceLabel({ id: 1, index: 1, name: "dev" })).toBe("dev");
    expect(workspaceLabel({ id: 2, index: 2 })).toBe("2");
    expect(workspaceLabel({ id: 3 })).toBe("3");
    expect(workspaceLabel({})).toBe("?");
  });
});

describe("focusedWindowText", () => {
  test("formats focused window states", () => {
    expect(focusedWindowText({ available: true, window: { appId: "Terminal", title: "cargo test" } })).toBe(
      "Terminal — cargo test",
    );
    expect(focusedWindowText({ available: true, window: null })).toBe("no focused window");
    expect(focusedWindowText({ available: false, reason: "niri unavailable" })).toBe("window: unavailable");
  });
});

describe("batteryText", () => {
  test("formats available battery and hides unavailable battery", () => {
    expect(batteryText({ available: true, percentage: 87, status: "discharging" })).toBe("bat: 87% discharging");
    expect(batteryText({ available: false, reason: "no battery" })).toBeNull();
  });
});

describe("networkText", () => {
  test("formats network counts and unavailable state", () => {
    expect(networkText({ available: true, wired: { up: 1, down: 0 }, wireless: { up: 0, down: 1 } })).toBe(
      "net: wired up · wifi down",
    );
    expect(networkText({ available: false, reason: "no network" })).toBe("net: unavailable");
  });
});

describe("bridgeStatusText", () => {
  test("includes backend, monitor count, and focused output", () => {
    expect(
      bridgeStatusText({
        host: { backend: "wayland-layer-shell", monitorCount: 2, bridgeVersion: 2 },
        niri: { focusedOutput: { available: true, name: "eDP-1" } },
      }),
    ).toBe("bridge: wayland-layer-shell · monitors: 2 · niri: eDP-1");
  });
});
