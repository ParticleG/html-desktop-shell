import { afterEach, describe, expect, mock, test } from "bun:test";
import { mount, type VueWrapper } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import type { ShellState } from "@html-desktop-shell/shell-api";

import { i18n } from "../i18n";
import WorkspacesWidget from "./WorkspacesWidget.vue";

const mounted: VueWrapper[] = [];

function mountWidget(
  state: ShellState | null,
  focusWorkspaceFromButton = mock(async (_workspaceIndex: number) => {}),
): { wrapper: VueWrapper; focusWorkspaceFromButton: typeof focusWorkspaceFromButton } {
  const wrapper = mount(WorkspacesWidget, {
    props: {
      state,
      hostInfo: {
        shell: "html-desktop-shell",
        backend: "wayland-layer-shell",
        bridgeVersion: 5,
        surface: { kind: "bar", monitorIndex: 0, output: "DP-1" },
      },
      bridgeError: "",
      actionError: "",
      actionErrorDetail: "",
      focusWorkspaceFromButton,
    },
    global: { plugins: [i18n, PrimeVue] },
  });
  mounted.push(wrapper);
  return { wrapper, focusWorkspaceFromButton };
}

afterEach(() => {
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
  document.body.replaceChildren();
  delete window.webkit;
});

describe("WorkspacesWidget mounted interactions", () => {
  test("uses the workspace index for primary pointer, Enter, and Space only", async () => {
    const { wrapper, focusWorkspaceFromButton } = mountWidget({
      niri: {
        workspaces: {
          available: true,
          items: [{ id: 41, index: 2, output: "DP-1", isActive: true, isFocused: true }],
        },
      },
    });
    const button = wrapper.get("button");

    await button.trigger("pointerdown", { button: 1 });
    await button.trigger("keydown", { key: "Escape" });
    expect(focusWorkspaceFromButton).toHaveBeenCalledTimes(0);

    await button.trigger("pointerdown", { button: 0 });
    await button.trigger("keydown", { key: "Enter" });
    await button.trigger("keydown", { key: " " });

    expect(focusWorkspaceFromButton).toHaveBeenCalledTimes(3);
    expect(focusWorkspaceFromButton.mock.calls).toEqual([[2], [2], [2]]);
    expect(focusWorkspaceFromButton.mock.calls.flat()).not.toContain(41);
  });

  test("renders unavailable and empty workspace states", () => {
    const unavailable = mountWidget({
      niri: { workspaces: { available: false, reason: "provider unavailable" } },
    }).wrapper;
    expect(unavailable.text()).toContain("workspaces: unavailable");
    expect(unavailable.find("button").exists()).toBe(false);

    const empty = mountWidget({
      niri: { workspaces: { available: true, items: [] } },
    }).wrapper;
    expect(empty.text()).toContain("workspaces: none");
    expect(empty.find("button").exists()).toBe(false);
  });
});
