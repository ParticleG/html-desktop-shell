import { afterEach, describe, expect, test } from "bun:test";
import { mount, type VueWrapper } from "@vue/test-utils";
import { focusWorkspace, type ShellState } from "@html-desktop-shell/shell-api";

import FocusedWindowWidget from "./FocusedWindowWidget.vue";

const mounted: VueWrapper[] = [];

function mountWidget(state: ShellState | null, output: string | null): VueWrapper {
  const wrapper = mount(FocusedWindowWidget, {
    props: {
      state,
      hostInfo: {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 4,
        surface: { kind: "bar", monitorIndex: 0, output },
      },
      bridgeError: "",
      actionError: "",
      actionErrorDetail: "",
      focusWorkspaceFromButton: async (workspaceIndex) => {
        await focusWorkspace(workspaceIndex);
      },
    },
  });
  mounted.push(wrapper);
  return wrapper;
}

afterEach(() => {
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
  document.body.replaceChildren();
});

describe("FocusedWindowWidget output selection", () => {
  const state: ShellState = {
    niri: {
      focusedWindows: {
        available: true,
        items: [
          { output: "DP-2", window: { appId: "Terminal", title: "DP-2 shell" } },
          { output: "DP-3", window: { appId: "Browser", title: "DP-3 docs" } },
          { output: "HDMI-A-1", window: null },
        ],
      },
    },
  };

  test("renders a distinct focused window for each surface output", () => {
    expect(mountWidget(state, "DP-2").text()).toContain("Terminal — DP-2 shell");
    expect(mountWidget(state, "DP-3").text()).toContain("Browser — DP-3 docs");
  });

  test("renders no-focused state for missing, null-window, and null-output items", () => {
    expect(mountWidget(state, "DP-4").text()).toContain("no focused window");
    expect(mountWidget(state, "HDMI-A-1").text()).toContain("no focused window");
    expect(mountWidget(state, null).text()).toContain("no focused window");
  });

  test("keeps provider unavailability explicit", () => {
    const unavailable: ShellState = {
      niri: { focusedWindows: { available: false, reason: "niri unavailable" } },
    };
    expect(mountWidget(unavailable, "DP-2").text()).toContain("window: unavailable");
  });
});
