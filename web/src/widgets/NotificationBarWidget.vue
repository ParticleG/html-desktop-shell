<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useShellStore } from "../stores/shell";

const { t } = useI18n();
const shell = useShellStore();
const notifications = computed(() => shell.state?.notifications);
const label = computed(() => `${t("desktop.notifications")} · ${notifications.value?.available
  ? t("desktop.unread", { count: notifications.value.unreadCount }) : t("states.unavailable")}`);
const reason = computed(() => notifications.value && !notifications.value.available ? notifications.value.reason : undefined);
</script>

<template>
  <Button
    id="notification-bar"
    class="surface-action"
    :label="label"
    :title="reason"
    :disabled="!shell.hasCapability('setQuickPanelOpen')"
    size="small"
    text
    @click="shell.setQuickPanelOpenFromButton(true)"
  />
</template>
