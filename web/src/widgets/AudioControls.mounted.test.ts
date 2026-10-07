import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import type { ShellMethod, ShellState } from "@html-desktop-shell/shell-api";
import { createPinia } from "pinia";
import type { App, Component, Plugin } from "vue";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import AudioBarWidget from "./AudioBarWidget.vue";
import AudioControls from "./AudioControls.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };
let originalWebkit: typeof window.webkit;

beforeEach(() => {
  originalWebkit = window.webkit;
});
afterEach(() => {
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
  document.body.replaceChildren();
  if (originalWebkit === undefined) {
    delete window.webkit;
  } else {
    window.webkit = originalWebkit;
  }
});

function mountAudio(
  audio: ShellState["audio"],
  respond: (request: RequestEnvelope) => string | Promise<string>,
  methods: ShellMethod[] = ["setAudioVolume", "setAudioMuted"],
  component: Component = AudioControls,
) {
  const requests: RequestEnvelope[] = [];
  window.webkit = {
    messageHandlers: {
      shell: {
        postMessage(request) {
          requests.push(request);
          return respond(request);
        },
      },
    },
  };
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.state = { audio, battery: { available: false } };
  shell.capabilities = { methods };
  const wrapper = mount(component, {
    attachTo: document.body,
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return { wrapper, shell, requests };
}

function pendingReply() {
  let resolve!: (value: string) => void;
  const promise = new Promise<string>((complete) => {
    resolve = complete;
  });
  return { promise, resolve };
}

async function settle(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

function unexpectedRequest(request: RequestEnvelope): never {
  throw new Error(`Unexpected native operation: ${request.method}`);
}

describe("AudioControls native observation and interaction", () => {
  test("observes volume and mute polling, then applies the entire native mute snapshot", async () => {
    let replyIndex = 0;
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 73, muted: true },
      (request) => JSON.stringify({
        id: request.id,
        ok: true,
        result: replyIndex++ === 0
          ? { available: true, volume: 81, muted: true }
          : { available: true, volume: 79, muted: false },
      }),
    );
    const range = wrapper.get<HTMLInputElement>("input[type=range]");
    expect(wrapper.get("label").attributes("for")).toBe(range.attributes("id"));
    expect(range.element.min).toBe("0");
    expect(range.element.max).toBe("100");
    expect(range.element.step).toBe("1");
    expect(range.element.value).toBe("73");
    expect(wrapper.get("button").text()).toBe("Unmute");
    expect(wrapper.get("button").attributes("aria-pressed")).toBe("true");

    shell.state!.audio = { available: true, volume: 82, muted: false };
    await settle();
    expect(range.element.value).toBe("82");
    expect(wrapper.get("button").text()).toBe("Mute");
    expect(requests).toEqual([]);

    await wrapper.get("button").trigger("click");
    await settle();
    expect(shell.state!.audio).toEqual({ available: true, volume: 81, muted: true });
    expect(shell.state!.battery).toEqual({ available: false });
    expect(wrapper.get("label").text()).toBe("Volume: 81%");
    expect(wrapper.get("button").text()).toBe("Unmute");
    expect(requests).toEqual([
      { id: expect.any(String), method: "setAudioMuted", params: { muted: true } },
    ]);

    await wrapper.get("button").trigger("click");
    await settle();
    expect(requests[1]).toMatchObject({ method: "setAudioMuted", params: { muted: false } });
    expect(shell.state!.audio).toEqual({ available: true, volume: 79, muted: false });
    expect(wrapper.get("label").text()).toBe("Volume: 79%");
    expect(wrapper.get("button").attributes("aria-pressed")).toBe("false");
    expect(wrapper.get("button").text()).toBe("Mute");
  });

  test("preserves amplified readings without writing the bounded slider value", async () => {
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 137.5, muted: false },
      (request) => JSON.stringify({
        id: request.id,
        ok: true,
        result: { available: true, volume: 98, muted: true },
      }),
    );
    const range = wrapper.get<HTMLInputElement>("input[type=range]");
    expect(range.element.value).toBe("100");
    expect(wrapper.text()).toContain("Volume: 137.5%");
    expect(wrapper.text()).toContain("Amplified: 137.5%");
    expect(range.attributes("aria-valuetext")).toBe("137.5%");
    shell.state!.audio = { available: true, volume: 145, muted: false };
    await settle();
    expect(wrapper.text()).toContain("Amplified: 145%");
    expect(requests).toEqual([]);

    range.element.value = "99.7";
    await range.trigger("input");
    expect(requests).toEqual([]);
    await range.trigger("change");
    await settle();
    expect(requests[0]).toMatchObject({ method: "setAudioVolume", params: { volume: 100 } });
    expect(shell.state!.audio).toEqual({ available: true, volume: 98, muted: true });
    expect(range.element.value).toBe("98");
    expect(wrapper.text()).not.toContain("Amplified:");
  });

  test("does not let polling replace an in-progress drag and commits only on change", async () => {
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 40, muted: false },
      (request) => JSON.stringify({
        id: request.id,
        ok: true,
        result: { available: true, volume: 77, muted: true },
      }),
    );
    const range = wrapper.get<HTMLInputElement>("input[type=range]");
    await range.trigger("pointerdown", { button: 0 });
    range.element.value = "78";
    await range.trigger("input");
    shell.state!.audio = { available: true, volume: 20, muted: true };
    await settle();
    expect(range.element.value).toBe("78");
    expect(range.attributes("aria-valuetext")).toBe("78%");
    expect(wrapper.get("label").text()).toBe("Volume: 20%");
    expect(requests).toEqual([]);
    await range.trigger("pointerup");
    await range.trigger("change");
    await settle();
    expect(requests[0]).toMatchObject({ method: "setAudioVolume", params: { volume: 78 } });
    expect(wrapper.get("label").text()).toBe("Volume: 77%");
    expect(range.element.value).toBe("77");
    expect(wrapper.get("button").text()).toBe("Unmute");
  });

  test("gates each control independently and hides unavailable or incomplete observations", async () => {
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 45, muted: false }, unexpectedRequest, [],
    );
    const range = wrapper.get<HTMLInputElement>("input[type=range]");
    expect(range.element.disabled).toBe(true);
    expect(wrapper.get<HTMLButtonElement>("button").element.disabled).toBe(true);
    await range.trigger("change");
    await wrapper.get("button").trigger("click");
    expect(requests).toEqual([]);

    shell.capabilities = { methods: ["setAudioVolume"] };
    await settle();
    expect(range.element.disabled).toBe(false);
    expect(wrapper.get<HTMLButtonElement>("button").element.disabled).toBe(true);
    shell.capabilities = { methods: ["setAudioMuted"] };
    await settle();
    expect(range.element.disabled).toBe(true);
    expect(wrapper.get<HTMLButtonElement>("button").element.disabled).toBe(false);

    shell.state!.audio = { available: false, reason: "No default output" };
    await settle();
    expect(wrapper.text()).toContain("unavailable: No default output");
    expect(wrapper.find("input").exists()).toBe(false);
    expect(wrapper.find("button").exists()).toBe(false);
    expect(wrapper.text()).not.toContain("45%");
    shell.state!.audio = { available: true };
    await settle();
    expect(wrapper.text()).toContain("unavailable");
    expect(wrapper.find("input").exists()).toBe(false);
    expect(requests).toEqual([]);
  });

  test("serializes writes while pending and accepts a fresh snapshot after ordinary polling", async () => {
    const reply = pendingReply();
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 41, muted: false }, () => reply.promise,
    );
    const range = wrapper.get<HTMLInputElement>("input[type=range]");
    range.element.value = "60";
    await range.trigger("input");
    await range.trigger("change");
    expect(range.element.disabled).toBe(true);
    expect(wrapper.get<HTMLButtonElement>("button").element.disabled).toBe(true);
    expect(wrapper.get("section").attributes("aria-busy")).toBe("true");
    range.element.value = "85";
    await range.trigger("change");
    await wrapper.get("button").trigger("click");
    expect(requests).toHaveLength(1);
    shell.state!.audio = { available: true, volume: 54, muted: false };
    await settle();
    reply.resolve(JSON.stringify({
      id: requests[0]!.id, ok: true,
      result: { available: true, volume: 66, muted: true },
    }));
    await settle();
    expect(shell.state!.audio).toEqual({ available: true, volume: 66, muted: true });
    expect(wrapper.get("label").text()).toBe("Volume: 66%");
    expect(range.element.disabled).toBe(false);
    expect(range.element.value).toBe("66");
    expect(wrapper.get("section").attributes("aria-busy")).toBe("false");
  });

  test("exposes structured failures without optimistic state and clears them on successful retry", async () => {
    let fail = true;
    const { wrapper, shell } = mountAudio(
      { available: true, volume: 41, muted: false },
      (request) => JSON.stringify(fail ? {
        id: request.id, ok: false, error: { code: "AUDIO_WRITE_FAILED", message: "Mixer rejected update" },
      } : {
        id: request.id, ok: true, result: { available: true, volume: 39, muted: true },
      }),
    );
    const range = wrapper.get<HTMLInputElement>("input[type=range]");
    range.element.value = "90";
    await range.trigger("input");
    await range.trigger("change");
    await settle();
    expect(wrapper.get("[role=alert]").text()).toBe("AUDIO_WRITE_FAILED: Mixer rejected update");
    expect(shell.state!.audio).toEqual({ available: true, volume: 41, muted: false });
    expect(range.element.value).toBe("41");
    expect(range.element.disabled).toBe(false);
    fail = false;
    await wrapper.get("button").trigger("click");
    await settle();
    expect(wrapper.find("[role=alert]").exists()).toBe(false);
    expect(wrapper.get("button").text()).toBe("Unmute");
    expect(wrapper.get("label").text()).toBe("Volume: 39%");
  });

  test("drops a late snapshot after availability loss even when the provider recovers", async () => {
    const reply = pendingReply();
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 41, muted: false }, () => reply.promise,
    );
    await wrapper.get("button").trigger("click");
    shell.state!.audio = { available: false, reason: "Audio disconnected" };
    await settle();
    expect(wrapper.text()).toContain("Audio disconnected");
    expect(wrapper.find("button").exists()).toBe(false);
    shell.state!.audio = { available: true, volume: 22, muted: false };
    await settle();
    reply.resolve(JSON.stringify({
      id: requests[0]!.id, ok: true,
      result: { available: true, volume: 45, muted: true },
    }));
    await settle();
    expect(shell.state!.audio).toEqual({ available: true, volume: 22, muted: false });
    expect(wrapper.get("label").text()).toBe("Volume: 22%");
    expect(wrapper.get("button").text()).toBe("Mute");
  });

  test("does not apply late native results after unmount or capability revocation", async () => {
    const replies = [pendingReply(), pendingReply()];
    let replyIndex = 0;
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 41, muted: false }, () => replies[replyIndex++]!.promise,
    );
    await wrapper.get("button").trigger("click");
    shell.capabilities = { methods: ["setAudioVolume"] };
    await settle();
    replies[0]!.resolve(JSON.stringify({
      id: requests[0]!.id, ok: true,
      result: { available: true, volume: 90, muted: true },
    }));
    await settle();
    expect(shell.state!.audio).toEqual({ available: true, volume: 41, muted: false });
    shell.capabilities = { methods: ["setAudioVolume", "setAudioMuted"] };
    await settle();
    await wrapper.get("button").trigger("click");
    wrapper.unmount();
    mounted.splice(mounted.indexOf(wrapper), 1);
    replies[1]!.resolve(JSON.stringify({
      id: requests[1]!.id, ok: true,
      result: { available: true, volume: 99, muted: true },
    }));
    await settle();
    expect(shell.state!.audio).toEqual({ available: true, volume: 41, muted: false });
  });

  test("applies native unavailable snapshots rather than retaining stale controls", async () => {
    const { wrapper, shell } = mountAudio(
      { available: true, volume: 41, muted: false },
      (request) => JSON.stringify({
        id: request.id, ok: true, result: { available: false, reason: "Default output removed" },
      }),
    );
    await wrapper.get("button").trigger("click");
    await settle();
    expect(shell.state!.audio).toEqual({ available: false, reason: "Default output removed" });
    expect(wrapper.text()).toContain("Default output removed");
    expect(wrapper.find("input").exists()).toBe(false);
    expect(wrapper.find("button").exists()).toBe(false);
  });
});

describe("AudioBarWidget compact native observation", () => {
  test("shows amplified and mute state without writing audio or creating a popup", async () => {
    const { wrapper, shell, requests } = mountAudio(
      { available: true, volume: 143, muted: true }, unexpectedRequest, [], AudioBarWidget,
    );
    const button = wrapper.get<HTMLButtonElement>("button");
    expect(button.text()).toContain("Volume: 143%");
    expect(button.text()).toContain("Mute");
    expect(button.attributes("aria-label")).toBe("Open Quick Panel");
    expect(button.attributes("aria-describedby")).toBe(wrapper.get("span[id]").attributes("id"));
    expect(button.element.disabled).toBe(true);
    await button.trigger("click");
    expect(requests).toEqual([]);
    shell.state!.audio = { available: true, volume: 63, muted: false };
    await settle();
    expect(button.text()).toContain("Volume: 63%");
    expect(button.text()).not.toContain("Mute");
    expect(wrapper.find("input").exists()).toBe(false);
    expect(wrapper.find("[role=dialog]").exists()).toBe(false);
    shell.state!.audio = { available: false, reason: "Mixer unavailable" };
    await settle();
    expect(button.text()).toContain("Mixer unavailable");
    expect(button.text()).not.toContain("63%");
    expect(requests).toEqual([]);
  });

  test("opens only the Quick Panel, prevents duplicates and displays structured failures", async () => {
    const reply = pendingReply();
    const { wrapper, requests } = mountAudio(
      { available: true, volume: 52, muted: false }, () => reply.promise,
      ["setQuickPanelOpen"], AudioBarWidget,
    );
    const button = wrapper.get<HTMLButtonElement>("button");
    await button.trigger("click");
    await button.trigger("click");
    expect(button.element.disabled).toBe(true);
    expect(requests).toEqual([
      { id: expect.any(String), method: "setQuickPanelOpen", params: { open: true } },
    ]);
    expect(wrapper.find("[role=dialog]").exists()).toBe(false);
    reply.resolve(JSON.stringify({
      id: requests[0]!.id, ok: false,
      error: { code: "SURFACE_UNAVAILABLE", message: "Quick Panel is unavailable" },
    }));
    await settle();
    expect(wrapper.get("[role=alert]").text()).toBe("SURFACE_UNAVAILABLE: Quick Panel is unavailable");
    expect(button.element.disabled).toBe(false);
  });
});
