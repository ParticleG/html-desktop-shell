<script setup lang="ts">
import type { AudioState } from "@html-desktop-shell/shell-api";
import { setAudioMuted, setAudioVolume } from "@html-desktop-shell/shell-api";
import Button from "primevue/button";
import { computed, ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useNativeAction } from "../composables/useNativeAction";
import { useShellStore } from "../stores/shell";

const { t } = useI18n();
const shell = useShellStore();
const { busy, error, run } = useNativeAction();
const rangeId = useId();
const audio = computed(() => {
  const value = shell.state?.audio;
  return value?.available === true && "volume" in value && "muted" in value ? value : null;
});
const unavailableReason = computed(() => {
  const value = shell.state?.audio;
  return value?.available === false ? value.reason : "";
});
const canSetVolume = computed(() => shell.hasCapability("setAudioVolume"));
const canSetMuted = computed(() => shell.hasCapability("setAudioMuted"));
const draftVolume = ref(0);
const editing = ref(false);
let availabilityGeneration = 0;

watch(
  [() => audio.value !== null, canSetVolume, canSetMuted],
  () => {
    availabilityGeneration += 1;
    if (!audio.value || !canSetVolume.value) {
      editing.value = false;
    }
  },
  { flush: "sync" },
);
watch(
  [() => audio.value?.volume, busy],
  () => {
    if (!editing.value && !busy.value) {
      draftVolume.value = Math.min(100, Math.max(0, Math.round(audio.value?.volume ?? 0)));
    }
  },
  { immediate: true, flush: "sync" },
);

function updateDraft(event: Event): void {
  if (!audio.value || !canSetVolume.value || busy.value) {
    return;
  }
  editing.value = true;
  draftVolume.value = Math.min(100, Math.max(0, Math.round(Number((event.target as HTMLInputElement).value))));
}

function cancelEditing(): void {
  editing.value = false;
  draftVolume.value = Math.min(100, Math.max(0, Math.round(audio.value?.volume ?? 0)));
}

async function commitVolume(event: Event): Promise<void> {
  editing.value = false;
  const value = Number((event.target as HTMLInputElement).value);
  if (!Number.isFinite(value) || !audio.value || !canSetVolume.value || busy.value) {
    cancelEditing();
    return;
  }
  await updateAudio("setAudioVolume", () => setAudioVolume(Math.min(100, Math.max(0, Math.round(value)))));
}

async function toggleMuted(): Promise<void> {
  if (!audio.value || !canSetMuted.value || busy.value) {
    return;
  }
  const muted = !audio.value.muted;
  await updateAudio("setAudioMuted", () => setAudioMuted(muted));
}

async function updateAudio(
  method: "setAudioVolume" | "setAudioMuted",
  operation: () => Promise<AudioState>,
): Promise<void> {
  const generation = availabilityGeneration;
  const snapshot = await run(operation);
  if (snapshot !== undefined && generation === availabilityGeneration && audio.value &&
      shell.hasCapability(method) && shell.state) {
    shell.state = { ...shell.state, audio: snapshot };
  }
  if (!editing.value) {
    cancelEditing();
  }
}
</script>

<template>
  <section class="desktop-section" :aria-label="t('desktop.audio')" :aria-busy="busy">
    <header class="desktop-header">
      <h2>{{ t("desktop.audio") }}</h2>
      <span class="desktop-caption">{{ t("desktop.defaultOutput") }}</span>
    </header>
    <template v-if="audio">
      <div class="desktop-controls">
        <label :for="rangeId">{{ t("desktop.volume") }}: {{ audio.volume }}%</label>
        <input
          :id="rangeId"
          class="desktop-range"
          type="range"
          min="0"
          max="100"
          step="1"
          :value="draftVolume"
          :disabled="!canSetVolume || busy"
          :aria-valuetext="`${editing ? draftVolume : audio.volume}%`"
          @pointerdown="editing = canSetVolume && !busy"
          @pointerup="editing = false"
          @pointercancel="cancelEditing"
          @blur="cancelEditing"
          @input="updateDraft"
          @change="commitVolume"
        />
        <Button
          class="surface-action"
          :label="t(audio.muted ? 'desktop.unmute' : 'desktop.mute')"
          :aria-pressed="audio.muted"
          :disabled="!canSetMuted || busy"
          size="small"
          @click="toggleMuted"
        />
      </div>
      <p v-if="audio.volume > 100" class="desktop-caption">
        {{ t("desktop.amplified", { volume: audio.volume }) }}
      </p>
    </template>
    <p v-else class="desktop-empty">
      {{ t("states.unavailable") }}<template v-if="unavailableReason">: {{ unavailableReason }}</template>
    </p>
    <p v-if="error" class="desktop-error" role="alert">{{ error }}</p>
  </section>
</template>
