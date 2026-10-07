import { afterEach, describe, expect, test } from "bun:test";
import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { createPinia } from "pinia";
import type { App, Plugin } from "vue";
import type { NotificationItem, NotificationsState, ShellMethod } from "@html-desktop-shell/shell-api";

import { i18n } from "../i18n";
import { installPrimeVue } from "../plugins/primevue";
import { useShellStore } from "../stores/shell";
import NotificationBarWidget from "./NotificationBarWidget.vue";
import NotificationCenter from "./NotificationCenter.vue";

interface RequestEnvelope {
  id: string;
  method: string;
  params: unknown;
}
type Reply = { result: unknown } | { error: { code: string; message: string } };
type Handler = (params: unknown) => Reply | Promise<Reply>;
const mounted = new Set<VueWrapper>();
const originalWebkit = window.webkit;
const primeVuePlugin: Plugin = { install: (app: App) => installPrimeVue(app) };
const methods: ShellMethod[] = ["getNotifications", "dismissNotification", "invokeNotificationAction", "markAllNotificationsRead"];

function item(overrides: Partial<NotificationItem> = {}): NotificationItem {
  return {
    id: 1,
    appName: "Fixture app",
    summary: "Build complete",
    body: "Ready to review",
    createdAt: 1_700_000_000_000,
    active: true,
    read: false,
    urgency: "normal",
    icon: null,
    actions: [{ key: "review", label: "Review" }],
    ...overrides,
  };
}

function counts(unreadCount = 1, activeCount = 1, historyCount = 1): NotificationsState {
  return { available: true, unreadCount, activeCount, historyCount };
}

function fixture(options: { items?: NotificationItem[]; notifications?: NotificationsState; methods?: ShellMethod[]; bar?: boolean } = {}) {
  const requests: RequestEnvelope[] = [];
  const history = { items: options.items ?? [item()] };
  const handlers: Record<string, Handler> = {
    getNotifications: () => ({ result: { items: history.items } }),
  };
  window.webkit = {
    messageHandlers: {
      shell: {
        async postMessage(request) {
          requests.push(request);
          const handler = handlers[request.method];
          if (!handler) {
            throw new Error(`unexpected native method ${request.method}`);
          }
          const reply = await handler(request.params);
          return JSON.stringify({ id: request.id, ok: !("error" in reply), ...reply });
        },
      },
    },
  };
  const pinia = createPinia();
  const shell = useShellStore(pinia);
  shell.capabilities = { methods: options.methods ?? methods };
  shell.state = { notifications: options.notifications ?? counts() };
  const wrapper = mount(options.bar ? NotificationBarWidget : NotificationCenter, {
    attachTo: document.body,
    global: { plugins: [pinia, i18n, primeVuePlugin] },
  });
  mounted.add(wrapper);
  return { wrapper, shell, history, handlers, requests };
}

async function settle(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

function button(wrapper: VueWrapper, label: string) {
  return wrapper.findAll("button").find((candidate) => candidate.text() === label)!;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((complete) => { resolve = complete; });
  return { promise, resolve };
}

function dispose(wrapper: VueWrapper): void {
  wrapper.unmount();
  mounted.delete(wrapper);
}

afterEach(() => {
  for (const wrapper of mounted) {
    wrapper.unmount();
  }
  mounted.clear();
  document.body.replaceChildren();
  if (originalWebkit === undefined) {
    delete window.webkit;
  } else {
    window.webkit = originalWebkit;
  }
});

describe("NotificationCenter native history", () => {
  test("renders literal bodies, timestamps, urgency and read/active state without HTML or URL authority", async () => {
    const { wrapper } = fixture({ items: [item({
      summary: "<b>Summary</b>",
      body: '<b>Body</b>\n<img src="https://invalid.test/icon">',
      urgency: "critical",
    })] });
    await settle();
    const record = wrapper.get(".notification-item");
    expect(record.text()).toContain("<b>Summary</b>");
    expect(record.get(".notification-body").text()).toContain('<b>Body</b>\n<img src="https://invalid.test/icon">');
    expect(record.find("b").exists()).toBe(false);
    expect(record.find("img").exists()).toBe(false);
    expect(record.find("a").exists()).toBe(false);
    expect(record.get("time").attributes("datetime")).toBe("2023-11-14T22:13:20.000Z");
    expect(record.text()).toContain("Critical");
    expect(record.text()).toContain("Active");
    expect(record.classes()).toContain("is-unread");
    expect(record.classes()).toContain("is-critical");
    expect(button(wrapper, "Dismiss").attributes("aria-label")).toBe("Dismiss: <b>Summary</b>");
  });

  test("uses returned native unread snapshots and refreshes resident and closed action history", async () => {
    const resident = item({ id: 1, actions: [{ key: "stay", label: "Stay" }] });
    const transient = item({ id: 2, actions: [{ key: "close", label: "Finish" }] });
    const { wrapper, shell, history, handlers } = fixture({ items: [resident, transient], notifications: counts(17, 2, 2) });
    handlers.invokeNotificationAction = (params) => {
      const { notificationId } = params as { notificationId: number };
      history.items = history.items.map((entry) => entry.id === notificationId
        ? { ...entry, read: true, active: notificationId === 1 } : entry);
      return { result: counts(notificationId === 1 ? 13 : 0, notificationId === 1 ? 2 : 1, 2) };
    };
    await settle();
    await button(wrapper, "Stay").trigger("click");
    await settle();
    expect(shell.state?.notifications).toEqual(counts(13, 2, 2));
    const stillActive = wrapper.get('[data-notification-id="1"]');
    expect(stillActive.classes()).not.toContain("is-unread");
    expect(stillActive.text()).toContain("Read");
    expect(stillActive.text()).toContain("Active");
    expect(button(wrapper, "Stay").attributes("disabled")).toBeUndefined();
    await button(wrapper, "Finish").trigger("click");
    await settle();
    expect(shell.state?.notifications).toEqual(counts(0, 1, 2));
    expect(wrapper.get('[data-notification-id="2"]').text()).toContain("Closed");
    expect(button(wrapper, "Finish").attributes("disabled")).toBeDefined();
    expect(button(wrapper, "Mark all read").attributes("disabled")).toBeDefined();
  });

  test("allows dismissing closed history but never invokes its inactive actions", async () => {
    const { wrapper, shell, history, handlers, requests } = fixture({
      items: [item({ active: false, read: true })], notifications: counts(0, 0, 1),
    });
    handlers.dismissNotification = () => {
      history.items = [];
      return { result: counts(0, 0, 0) };
    };
    await settle();
    expect(button(wrapper, "Review").attributes("disabled")).toBeDefined();
    await button(wrapper, "Review").trigger("click");
    expect(requests.some((request) => request.method === "invokeNotificationAction")).toBe(false);
    await button(wrapper, "Dismiss").trigger("click");
    await settle();
    expect(wrapper.text()).toContain("No notifications");
    expect(shell.state?.notifications).toEqual(counts(0, 0, 0));
  });

  test("mark-all-read uses native state and displays structured mutation and history failures with recovery", async () => {
    const { wrapper, shell, history, handlers } = fixture();
    handlers.markAllNotificationsRead = () => ({ error: { code: "NOTIFICATION_CONFLICT", message: "Owner is unavailable" } });
    await settle();
    await button(wrapper, "Mark all read").trigger("click");
    await settle();
    expect(wrapper.get('[role="alert"]').text()).toContain("NOTIFICATION_CONFLICT: Owner is unavailable");
    expect(shell.state?.notifications).toEqual(counts());
    expect(wrapper.get(".notification-item").classes()).toContain("is-unread");
    handlers.markAllNotificationsRead = () => {
      history.items = [item({ read: true })];
      return { result: counts(0) };
    };
    handlers.getNotifications = () => ({ error: { code: "HISTORY_UNAVAILABLE", message: "Read failed" } });
    await button(wrapper, "Mark all read").trigger("click");
    await settle();
    expect(shell.state?.notifications).toEqual(counts(0));
    expect(wrapper.get('[role="alert"]').text()).toContain("HISTORY_UNAVAILABLE: Read failed");
    handlers.getNotifications = () => ({ result: { items: history.items } });
    await button(wrapper, "Refresh").trigger("click");
    await settle();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get(".notification-item").classes()).not.toContain("is-unread");
  });

  test("serializes and coalesces snapshot reads, including replacements whose counts do not change", async () => {
    const { wrapper, shell, handlers, requests } = fixture();
    await settle();
    const first = deferred<Reply>();
    const second = deferred<Reply>();
    let reads = 0;
    handlers.getNotifications = () => (++reads === 1 ? first.promise : second.promise);
    shell.state = { notifications: counts() };
    shell.state = { notifications: counts() };
    shell.state = { notifications: counts() };
    await settle();
    expect(reads).toBe(1);
    first.resolve({ result: { items: [item({ body: "First replacement" })] } });
    await settle();
    expect(reads).toBe(2);
    second.resolve({ result: { items: [item({ body: "Latest replacement" })] } });
    await settle();
    expect(wrapper.get(".notification-body").text()).toBe("Latest replacement");
    expect(requests.some((request) => request.method === "getState")).toBe(false);
    expect(reads).toBe(2);
  });

  test("drops stale body replies on unavailable state, recovers, and does not read without capability", async () => {
    const { wrapper, shell, handlers, requests } = fixture();
    await settle();
    const oldRead = deferred<Reply>();
    handlers.getNotifications = () => oldRead.promise;
    shell.state = { notifications: counts() };
    await settle();
    shell.state = { notifications: { available: false, reason: "Notification name owned by another process" } };
    await settle();
    expect(wrapper.text()).toContain("Notification name owned by another process");
    expect(wrapper.find(".notification-body").exists()).toBe(false);
    handlers.getNotifications = () => ({ result: { items: [item({ body: "Recovered body" })] } });
    shell.state = { notifications: counts() };
    await settle();
    expect(wrapper.find(".notification-body").exists()).toBe(false);
    oldRead.resolve({ result: { items: [item({ body: "Stale private body" })] } });
    await settle();
    expect(wrapper.text()).not.toContain("Stale private body");
    expect(wrapper.get(".notification-body").text()).toBe("Recovered body");
    shell.capabilities = { methods: [] };
    await settle();
    expect(wrapper.find(".notification-body").exists()).toBe(false);
    const readCount = requests.length;
    shell.state = { notifications: counts() };
    await settle();
    expect(requests.length).toBe(readCount);
    shell.capabilities = { methods };
    await settle();
    expect(wrapper.get(".notification-body").text()).toBe("Recovered body");
  });

  test("does not issue initial history reads when unavailable or unauthorized", async () => {
    const unavailable = fixture({ notifications: { available: false, reason: "Name conflict" } });
    await settle();
    expect(unavailable.requests).toEqual([]);
    expect(unavailable.wrapper.text()).toContain("Name conflict");
    dispose(unavailable.wrapper);
    const unauthorized = fixture({ methods: ["dismissNotification"] });
    await settle();
    expect(unauthorized.requests).toEqual([]);
    expect(unauthorized.wrapper.find(".notification-item").exists()).toBe(false);
  });

  test("applies native unavailable mutation snapshots and invalidates pending action failures across recovery", async () => {
    const { wrapper, shell, handlers } = fixture();
    await settle();
    handlers.dismissNotification = () => ({
      result: { available: false, reason: "Native service was replaced" },
    });
    await button(wrapper, "Dismiss").trigger("click");
    await settle();
    expect(shell.state?.notifications).toEqual({ available: false, reason: "Native service was replaced" });
    expect(wrapper.text()).toContain("Native service was replaced");
    expect(wrapper.find(".notification-item").exists()).toBe(false);
    shell.state = { notifications: counts() };
    await settle();
    const staleFailure = deferred<Reply>();
    handlers.invokeNotificationAction = () => staleFailure.promise;
    await button(wrapper, "Review").trigger("click");
    shell.state = { notifications: { available: false, reason: "Temporarily unavailable" } };
    shell.state = { notifications: counts() };
    await settle();
    staleFailure.resolve({ error: { code: "OLD_FAILURE", message: "From former service" } });
    await settle();
    expect(wrapper.find('[role="alert"]').exists()).toBe(false);
    expect(wrapper.get(".notification-item").text()).toContain("Build complete");
    expect(shell.state?.notifications).toEqual(counts());
  });

  test("does not restore native count snapshots after capability loss or disposal, or queue reads after unmount", async () => {
    const { wrapper, shell, handlers, requests } = fixture();
    await settle();
    const action = deferred<Reply>();
    handlers.markAllNotificationsRead = () => action.promise;
    await button(wrapper, "Mark all read").trigger("click");
    shell.capabilities = { methods: [] };
    await settle();
    action.resolve({ result: counts(0) });
    await settle();
    expect(shell.state?.notifications).toEqual(counts());
    shell.capabilities = { methods };
    await settle();
    const lateAction = deferred<Reply>();
    handlers.markAllNotificationsRead = () => lateAction.promise;
    await button(wrapper, "Mark all read").trigger("click");
    const pendingRead = deferred<Reply>();
    handlers.getNotifications = () => pendingRead.promise;
    shell.state = { notifications: counts() };
    shell.state = { notifications: counts() };
    await settle();
    const beforeDispose = requests.length;
    dispose(wrapper);
    lateAction.resolve({ result: counts(0) });
    pendingRead.resolve({ result: { items: [item({ body: "Disposed body" })] } });
    await settle();
    expect(shell.state?.notifications).toEqual(counts());
    expect(requests.length).toBe(beforeDispose);
    expect(document.body.textContent).not.toContain("Disposed body");
  });
});

describe("NotificationBarWidget cached summary", () => {
  test("opens only the existing Quick Panel and never requests history or polls state", async () => {
    const { wrapper, shell, handlers, requests } = fixture({ bar: true, methods: ["setQuickPanelOpen"], notifications: counts(7) });
    handlers.setQuickPanelOpen = () => ({ result: { open: true } });
    await settle();
    expect(wrapper.text()).toContain("7 unread");
    expect(requests).toEqual([]);
    await wrapper.get("button").trigger("click");
    await settle();
    expect(requests).toEqual([{ id: expect.any(String), method: "setQuickPanelOpen", params: { open: true } }]);
    shell.state = { notifications: { available: false, reason: "Native conflict" } };
    await settle();
    expect(wrapper.text()).toContain("unavailable");
    expect(wrapper.get("button").attributes("title")).toBe("Native conflict");
    shell.capabilities = { methods: [] };
    await settle();
    expect(wrapper.get("button").attributes("disabled")).toBeDefined();
    await wrapper.get("button").trigger("click");
    expect(requests).toHaveLength(1);
  });
});
