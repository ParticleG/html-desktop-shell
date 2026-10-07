<script setup lang="ts">
import {
  dismissNotification,
  getNotifications,
  invokeNotificationAction,
  markAllNotificationsRead,
  type NotificationItem,
  type NotificationsState,
  type ShellMethod,
} from "@html-desktop-shell/shell-api";
import Button from "primevue/button";
import { computed, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { nativeErrorMessage, useNativeAction } from "../composables/useNativeAction";
import { useShellStore } from "../stores/shell";
import ShellIcon from "./ShellIcon.vue";

const { t } = useI18n();
const shell = useShellStore();
const counts = computed(() => shell.state?.notifications);
const canRead = computed(() => counts.value?.available === true && shell.hasCapability("getNotifications"));
const items = ref<NotificationItem[]>([]);
const loading = ref(false);
const readError = ref("");
const { busy, error, run } = useNativeAction();
let disposed = false;
let generation = 0;
let reading = false;
let refreshRequested = false;

async function refreshHistory(): Promise<void> {
  if (disposed || !canRead.value) {
    return;
  }
  refreshRequested = true;
  if (reading) {
    return;
  }
  reading = true;
  loading.value = true;
  try {
    while (refreshRequested && !disposed && canRead.value) {
      refreshRequested = false;
      const currentGeneration = generation;
      try {
        const result = await getNotifications();
        if (!disposed && canRead.value && generation === currentGeneration) {
          items.value = result.items;
          readError.value = "";
        }
      } catch (cause) {
        if (!disposed && canRead.value && generation === currentGeneration) {
          readError.value = nativeErrorMessage(cause);
        }
      }
    }
  } finally {
    reading = false;
    if (!disposed) {
      loading.value = false;
    }
  }
}

// Every parent-owned snapshot can signal a body replacement, even with unchanged counts.
watch([() => shell.state, canRead], () => {
  if (!canRead.value) {
    generation++;
    refreshRequested = false;
    items.value = [];
    readError.value = "";
    error.value = "";
    return;
  }
  void refreshHistory();
}, { immediate: true, flush: "sync" });

async function performAction(method: ShellMethod, operation: () => Promise<NotificationsState>): Promise<void> {
  if (!canRead.value || !shell.hasCapability(method)) {
    return;
  }
  const currentGeneration = generation;
  const result = await run(async () => {
    try {
      return await operation();
    } catch (cause) {
      if (disposed || !canRead.value || generation !== currentGeneration || !shell.hasCapability(method)) {
        return undefined;
      }
      throw cause;
    }
  });
  if (result === undefined || disposed || !canRead.value || generation !== currentGeneration
    || !shell.hasCapability(method)) {
    return;
  }
  if (shell.state) {
    shell.state.notifications = result;
  }
  if (result.available) {
    await refreshHistory();
  }
}

function invokeAction(item: NotificationItem, actionKey: string): void {
  if (!item.active || !actionKey || !item.actions.some((action) => action.key === actionKey)) {
    return;
  }
  void performAction("invokeNotificationAction", () => invokeNotificationAction(item.id, actionKey));
}

onUnmounted(() => {
  disposed = true;
  generation++;
  refreshRequested = false;
});
</script>

<template>
  <section id="notification-center" class="desktop-section" :aria-label="t('desktop.notifications')">
    <header class="desktop-header">
      <h2>{{ t("desktop.notifications") }}</h2>
      <span v-if="counts?.available" class="desktop-caption">{{ t("desktop.unread", { count: counts.unreadCount }) }}</span>
      <div v-if="canRead" class="desktop-controls">
        <Button
          v-if="shell.hasCapability('markAllNotificationsRead')"
          class="surface-action"
          :label="t('desktop.markAllRead')"
          :disabled="busy || !counts?.available || counts.unreadCount === 0"
          size="small"
          @click="performAction('markAllNotificationsRead', markAllNotificationsRead)"
        />
        <Button
          class="surface-action"
          :label="t('desktop.refresh')"
          :disabled="busy || loading"
          size="small"
          text
          @click="refreshHistory"
        />
      </div>
    </header>
    <p v-if="!canRead" class="desktop-empty" role="status">
      {{ t("states.unavailable") }}<template v-if="counts && !counts.available">: {{ counts.reason }}</template>
    </p>
    <template v-else>
      <p v-if="error" class="desktop-error" role="alert">{{ error }}</p>
      <p v-if="readError" class="desktop-error" role="alert">{{ readError }}</p>
      <p v-if="loading" class="desktop-caption" role="status">{{ t("desktop.loading") }}</p>
      <p v-else-if="!items.length && !readError" class="desktop-empty">{{ t("desktop.emptyNotifications") }}</p>
      <ul class="desktop-list" :aria-busy="loading">
        <li
          v-for="item in items"
          :key="item.id"
          class="desktop-item notification-item"
          :class="{ 'is-unread': !item.read, 'is-critical': item.urgency === 'critical' }"
          :data-notification-id="item.id"
        >
          <ShellIcon :icon="item.icon" :size="24" />
          <div class="desktop-item-main">
            <div class="desktop-caption">{{ item.appName }}</div>
            <strong class="desktop-text">{{ item.summary }}</strong>
            <p class="desktop-text notification-body">{{ item.body }}</p>
            <div class="desktop-caption">
              <time :datetime="new Date(item.createdAt).toISOString()">{{ new Date(item.createdAt).toLocaleString() }}</time>
              · {{ t(`desktop.${item.urgency}`) }}
              · {{ item.read ? t("desktop.read") : t("desktop.unread", { count: 1 }) }}
              · {{ item.active ? t("desktop.active") : t("desktop.closed") }}
            </div>
            <div class="desktop-controls">
              <Button
                v-for="action in item.actions"
                :key="action.key"
                class="surface-action"
                :label="action.label"
                :disabled="busy || !item.active || !action.key || !shell.hasCapability('invokeNotificationAction')"
                size="small"
                @click="invokeAction(item, action.key)"
              />
              <Button
                v-if="shell.hasCapability('dismissNotification')"
                class="surface-action"
                :label="t('desktop.dismiss')"
                :aria-label="`${t('desktop.dismiss')}: ${item.summary}`"
                :disabled="busy"
                size="small"
                text
                @click="performAction('dismissNotification', () => dismissNotification(item.id))"
              />
            </div>
          </div>
        </li>
      </ul>
    </template>
  </section>
</template>
