import { afterEach, beforeEach, describe, expect, jest, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";

import type { DesktopApplication } from "@html-desktop-shell/shell-api";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import ApplicationLauncher from "./ApplicationLauncher.vue";

interface PendingRequest {
  id: string;
  method: string;
  params: unknown;
  respond(result: unknown): void;
  fail(code: string, message: string): void;
}

const mounted: VueWrapper[] = [];
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };
let previousWebkit: typeof window.webkit;

const applications: DesktopApplication[] = [
  {
    id: "org.example.Zebra.desktop",
    name: "Zebra Editor",
    description: "Edit native documents",
    icon: { width: 1, height: 1, rgba: "/wAA/w==" },
  },
  { id: "org.example.Alpha.desktop", name: "Alpha Terminal", description: null, icon: null },
];

function mountLauncher(available = true) {
  const requests: PendingRequest[] = [];
  window.webkit = {
    messageHandlers: {
      shell: {
        postMessage(request) {
          return new Promise<string>((resolve) => {
            requests.push({
              ...request,
              respond: (result) => resolve(JSON.stringify({ id: request.id, ok: true, result })),
              fail: (code, message) => resolve(JSON.stringify({
                id: request.id, ok: false, error: { code, message },
              })),
            });
          });
        },
      },
    },
  };
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.capabilities = { methods: ["searchApplications", "launchApplication"] };
  shell.state = {
    applications: available
      ? { available: true, count: 2 }
      : { available: false, reason: "Catalog is refreshing" },
  };
  const wrapper = mount(ApplicationLauncher, {
    attachTo: document.body,
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.push(wrapper);
  return { wrapper, shell, requests };
}

async function settle(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

beforeEach(() => {
  previousWebkit = window.webkit;
  jest.useFakeTimers();
});

afterEach(() => {
  for (const wrapper of mounted.splice(0)) {
    wrapper.unmount();
  }
  jest.clearAllTimers();
  jest.useRealTimers();
  document.body.replaceChildren();
  if (previousWebkit === undefined) {
    delete window.webkit;
  } else {
    window.webkit = previousWebkit;
  }
});

describe("ApplicationLauncher mounted native interactions", () => {
  test("presents native order, names, optional descriptions and pixel icons without client ranking", async () => {
    const { wrapper, requests } = mountLauncher();
    expect(requests[0]!.params).toEqual({ query: "", limit: 25 });
    expect(wrapper.get("label").attributes("for")).toBe(wrapper.get("input").attributes("id"));
    requests[0]!.respond({ items: applications });
    await settle();

    expect(wrapper.findAll("button").map((button) => button.attributes("aria-label"))).toEqual([
      "Launch Zebra Editor", "Launch Alpha Terminal",
    ]);
    expect(wrapper.findAll(".desktop-text > span:first-child").map((name) => name.text())).toEqual([
      "Zebra Editor", "Alpha Terminal",
    ]);
    expect(wrapper.findAll(".desktop-text .desktop-caption").map((description) => description.text())).toEqual([
      "Edit native documents",
    ]);
    expect(wrapper.findAll("canvas")).toHaveLength(1);
    expect(wrapper.find("img").exists()).toBe(false);
    expect(wrapper.findAll(".shell-icon")).toHaveLength(2);

    await wrapper.get("input").setValue("no local name match");
    jest.advanceTimersByTime(180);
    requests[1]!.respond({ items: applications });
    await settle();
    expect(wrapper.findAll("button").map((button) => button.attributes("aria-label"))).toEqual([
      "Launch Zebra Editor", "Launch Alpha Terminal",
    ]);
  });

  test("suppresses old selectable rows immediately, debounces input and keeps only the latest native reply", async () => {
    const { wrapper, requests } = mountLauncher();
    requests[0]!.respond({ items: applications });
    await settle();
    const oldButton = wrapper.get("button");

    await wrapper.get("input").setValue("a");
    await oldButton.trigger("click");
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    expect(wrapper.findAll("button")).toHaveLength(0);
    expect(requests.map((request) => request.method)).toEqual(["searchApplications"]);
    await wrapper.get("input").setValue("alpha");
    jest.advanceTimersByTime(180);
    expect(requests[1]!.params).toEqual({ query: "alpha", limit: 25 });

    await wrapper.get("input").setValue("zebra");
    jest.advanceTimersByTime(180);
    requests[2]!.respond({ items: [applications[0]] });
    await settle();
    requests[1]!.respond({ items: [applications[1]] });
    await settle();
    expect(wrapper.findAll("button").map((button) => button.attributes("aria-label"))).toEqual([
      "Launch Zebra Editor",
    ]);
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    expect(requests[3]!.params).toEqual({ applicationId: applications[0]!.id });
    requests[3]!.respond({ applicationId: applications[0]!.id });
    await settle();
  });

  test("Enter launches the keyboard-selected native ID, serializes pending launch and never executes typed paths", async () => {
    const { wrapper, requests } = mountLauncher();
    requests[0]!.respond({ items: applications });
    await settle();
    await wrapper.get("input").trigger("keydown", { key: "ArrowDown" });
    expect(wrapper.findAll("button")[1]!.attributes("aria-current")).toBe("true");
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    expect(requests[1]!.params).toEqual({ applicationId: "org.example.Alpha.desktop" });
    expect(wrapper.findAll("button").every((button) => button.attributes("disabled") !== undefined)).toBe(true);
    expect(wrapper.get("ul").attributes("aria-busy")).toBe("true");
    expect(wrapper.get('[role="status"]').text()).toBe("Loading…");
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    expect(requests.filter((request) => request.method === "launchApplication")).toHaveLength(1);
    requests[1]!.respond({ applicationId: "org.example.Alpha.desktop" });
    await settle();
    expect(wrapper.get("button").attributes("disabled")).toBeUndefined();
    expect(wrapper.find('[role="status"]').exists()).toBe(false);

    await wrapper.get("input").setValue("/usr/bin/terminal --execute https://example.invalid");
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    jest.advanceTimersByTime(180);
    requests[2]!.respond({ items: [] });
    await settle();
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    expect(wrapper.text()).toContain("No matching applications");
    expect(requests.filter((request) => request.method === "launchApplication")).toHaveLength(1);
  });

  test("shows structured unknown/stale native launch errors without a fallback launch", async () => {
    const { wrapper, requests } = mountLauncher();
    requests[0]!.respond({ items: applications });
    await settle();
    await wrapper.get("button").trigger("click");
    requests[1]!.fail("unknown_application", "Application is no longer installed");
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe("unknown_application: Application is no longer installed");
    expect(wrapper.get("button").attributes("disabled")).toBeUndefined();

    await wrapper.get("button").trigger("click");
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    requests[2]!.fail("unknown_application", "Application is not in the current native catalog");
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toContain("unknown_application: Application is not in the current native catalog");
    expect(requests.filter((request) => request.method === "launchApplication")).toHaveLength(2);
    expect(requests[2]!.params).toEqual({ applicationId: "org.example.Zebra.desktop" });
  });

  test("clears results on availability/capability loss, drops old replies and refreshes on catalog readiness/count", async () => {
    const { wrapper, shell, requests } = mountLauncher(false);
    expect(wrapper.get("input").attributes("disabled")).toBeDefined();
    expect(wrapper.text()).toContain("Catalog is refreshing");
    expect(requests).toHaveLength(0);
    shell.state!.applications = { available: true, count: 2 };
    await settle();
    requests[0]!.respond({ items: applications });
    await settle();
    const staleButton = wrapper.get("button");
    shell.state!.applications = { available: false, reason: "Catalog stopped" };
    await staleButton.trigger("click");
    expect(wrapper.findAll("button")).toHaveLength(0);
    expect(wrapper.text()).toContain("Catalog stopped");
    expect(requests.filter((request) => request.method === "launchApplication")).toHaveLength(0);

    shell.state!.applications = { available: true, count: 2 };
    await settle();
    shell.capabilities = { methods: ["launchApplication"] };
    await settle();
    requests[1]!.respond({ items: applications });
    await settle();
    expect(wrapper.findAll("button")).toHaveLength(0);
    expect(wrapper.get("input").attributes("disabled")).toBeDefined();
    shell.capabilities = { methods: ["searchApplications", "launchApplication"] };
    await settle();
    requests[2]!.respond({ items: applications });
    await settle();
    shell.state!.applications = { available: true, count: 3 };
    await settle();
    expect(wrapper.findAll("button").map((button) => button.attributes("aria-label"))).toEqual([
      "Launch Zebra Editor", "Launch Alpha Terminal",
    ]);
    requests[3]!.respond({ items: [applications[1]] });
    await settle();
    expect(wrapper.get("button").text()).toBe("Alpha Terminal");
    shell.capabilities = { methods: ["searchApplications"] };
    await settle();
    expect(wrapper.findAll("button")).toHaveLength(0);
    expect(wrapper.get("input").attributes("disabled")).toBeDefined();
    expect(requests).toHaveLength(4);
  });

  test("shows structured native search errors and preserves dispatched launch errors across query changes", async () => {
    const { wrapper, requests } = mountLauncher();
    requests[0]!.fail("provider_unavailable", "Catalog is not ready");
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe("provider_unavailable: Catalog is not ready");
    await wrapper.get("input").setValue("editor");
    jest.advanceTimersByTime(180);
    requests[1]!.respond({ items: [applications[0]] });
    await settle();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    await wrapper.get("button").trigger("click");
    await wrapper.get("input").setValue("terminal");
    requests[2]!.fail("unknown_application", "Old query application was removed");
    jest.advanceTimersByTime(180);
    requests[3]!.respond({ items: [applications[1]] });
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe("unknown_application: Old query application was removed");
    expect(wrapper.get("button").text()).toBe("Alpha Terminal");
  });

  test("refreshes same-count catalog snapshots without flicker, preserves selection and coalesces slow refreshes", async () => {
    const { wrapper, shell, requests } = mountLauncher();
    requests[0]!.respond({ items: applications });
    await settle();
    await wrapper.get("input").trigger("keydown", { key: "ArrowDown" });

    shell.state!.applications = { available: true, count: 2 };
    await settle();
    expect(wrapper.findAll("button").map((button) => button.attributes("aria-label"))).toEqual([
      "Launch Zebra Editor", "Launch Alpha Terminal",
    ]);
    expect(wrapper.findAll("button")[1]!.attributes("aria-current")).toBe("true");
    expect(wrapper.find('[role="status"]').exists()).toBe(false);
    expect(wrapper.get("button").attributes("disabled")).toBeUndefined();
    for (let index = 0; index < 4; index++) {
      shell.state!.applications = { available: true, count: 2 };
      await settle();
    }
    expect(requests).toHaveLength(2);
    const replacement: DesktopApplication = {
      id: "org.example.New.desktop", name: "New Application", description: null, icon: null,
    };
    requests[1]!.respond({ items: [applications[1], replacement] });
    await settle();
    expect(requests).toHaveLength(3);
    expect(wrapper.findAll("button").map((button) => button.attributes("aria-label"))).toEqual([
      "Launch Alpha Terminal", "Launch New Application",
    ]);
    expect(wrapper.findAll("button")[0]!.attributes("aria-current")).toBe("true");
    requests[2]!.respond({ items: [applications[1], replacement] });
    await settle();
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    expect(requests[3]!.params).toEqual({ applicationId: "org.example.Alpha.desktop" });
    requests[3]!.respond({ applicationId: "org.example.Alpha.desktop" });
    await settle();
  });

  test("preserves pending launch errors through background catalog replacement but drops them on authorization loss", async () => {
    const { wrapper, shell, requests } = mountLauncher();
    requests[0]!.respond({ items: applications });
    await settle();
    await wrapper.get("button").trigger("click");
    shell.state!.applications = { available: true, count: 2 };
    await settle();
    requests[2]!.respond({ items: [applications[1]] });
    requests[1]!.fail("unknown_application", "Launched application left the native catalog");
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toBe("unknown_application: Launched application left the native catalog");

    await wrapper.get("button").trigger("click");
    shell.capabilities = { methods: ["searchApplications"] };
    await settle();
    shell.capabilities = { methods: ["searchApplications", "launchApplication"] };
    await settle();
    requests[4]!.respond({ items: [applications[1]] });
    requests[3]!.fail("unknown_application", "Reply belongs to invalidated authorization");
    await settle();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get("button").attributes("disabled")).toBeUndefined();
  });

  test("cancels a queued debounce and ignores in-flight replies after disposal", async () => {
    const { wrapper, requests } = mountLauncher();
    await wrapper.get("input").setValue("queued query");
    wrapper.unmount();
    mounted.splice(mounted.indexOf(wrapper), 1);
    jest.advanceTimersByTime(1000);
    requests[0]!.respond({ items: applications });
    await settle();
    expect(requests).toHaveLength(1);
    expect(document.body.querySelector("input")).toBeNull();
    expect(document.body.querySelector("button")).toBeNull();
    expect(document.body.textContent).not.toContain("Zebra Editor");
  });

  test("does not restore errors or launch controls when a pending launch completes after disposal", async () => {
    const { wrapper, requests } = mountLauncher();
    requests[0]!.respond({ items: applications });
    await settle();
    await wrapper.get("button").trigger("click");
    wrapper.unmount();
    mounted.splice(mounted.indexOf(wrapper), 1);
    requests[1]!.fail("unknown_application", "Disposed application");
    await settle();
    expect(document.body.querySelector('[role="alert"]')).toBeNull();
    expect(document.body.querySelector("button")).toBeNull();
    expect(requests.filter((request) => request.method === "launchApplication")).toHaveLength(1);
  });
});
