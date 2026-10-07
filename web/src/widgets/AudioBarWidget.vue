<script setup lang="ts">
import { Volume2, VolumeX } from "@lucide/vue";
import { setQuickPanelOpen } from "@html-desktop-shell/shell-api";
import Button from "primevue/button";
import { computed, useId } from "vue";
import { useI18n } from "vue-i18n";

import { useNativeAction } from "../composables/useNativeAction";
import { useShellStore } from "../stores/shell";

const { t } = useI18n();
const shell = useShellStore();
const { busy, error, run } = useNativeAction();
const summaryId = useId();
const audio = computed(() => {
  const value = shell.state?.audio;
  return value?.available === true && "volume" in value && "muted" in value ? value : null;
});
const unavailableReason = computed(() => {
  const value = shell.state?.audio;
  return value?.available === false ? value.reason : "";
});
const canOpenPanel = computed(() => shell.hasCapability("setQuickPanelOpen"));

function openPanel(): void {
  if (canOpenPanel.value && !busy.value) {
    void run(() => setQuickPanelOpen(true));
  }
}
</script>

<template>
  <div class="desktop-controls" :aria-label="t('desktop.audio')" :aria-busy="busy">
    <Button
      class="surface-action"
      :aria-label="t('actions.openQuickPanel')"
      :aria-describedby="summaryId"
      :disabled="!canOpenPanel || busy"
      size="small"
      text
      @click="openPanel"
    >
      <span :id="summaryId">
        <template v-if="audio">
          <VolumeX v-if="audio.muted" class="widget-icon" :size="14" aria-hidden="true" />
          <Volume2 v-else class="widget-icon" :size="14" aria-hidden="true" />
          {{ t("desktop.volume") }}: {{ audio.volume }}%
          <span v-if="audio.muted"> · {{ t("desktop.mute") }}</span>
        </template>
        <template v-else>
          {{ t("desktop.audio") }}: {{ t("states.unavailable") }}
          <template v-if="unavailableReason"> · {{ unavailableReason }}</template>
        </template>
      </span>
    </Button>
    <span v-if="error" class="desktop-error" role="alert">{{ error }}</span>
  </div>
</template>
