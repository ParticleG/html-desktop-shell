import { describe, expect, test } from "bun:test";
import type { ShellState } from "@html-desktop-shell/shell-api";

import {
  batteryText,
  focusedWindowText,
  networkText,
  visibleWorkspaces,
  workspaceLabel,
} from "./view-model";

import { workspaceIndexForFocus } from "./widgets/types";

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

describe("workspaceIndexForFocus", () => {
  test("selects workspace index instead of the mismatched workspace id", () => {
    expect(workspaceIndexForFocus({ id: 41, index: 2 })).toBe(2);
  });
});

describe("focusedWindowText", () => {
  const focusedWindows = {
    available: true,
    items: [
      { output: "DP-2", window: { appId: "Terminal", title: "DP-2 shell" } },
      { output: "DP-3", window: { appId: "Browser", title: "DP-3 docs" } },
      { output: "HDMI-A-1", window: null },
    ],
  };

  test("formats the focused window for the surface output", () => {
    expect(focusedWindowText(focusedWindows, "DP-2")).toBe("Terminal — DP-2 shell");
    expect(focusedWindowText(focusedWindows, "DP-3")).toBe("Browser — DP-3 docs");
  });

  test("distinguishes no focused window from an unavailable provider", () => {
    expect(focusedWindowText(focusedWindows, "HDMI-A-1")).toBe("no focused window");
    expect(focusedWindowText(focusedWindows, "DP-4")).toBe("no focused window");
    expect(focusedWindowText(focusedWindows, null)).toBe("no focused window");
    expect(focusedWindowText({ available: false, reason: "niri unavailable" }, "DP-2")).toBe(
      "window: unavailable",
    );
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
