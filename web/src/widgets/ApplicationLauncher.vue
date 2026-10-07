<template>
  <section class="desktop-section" :aria-label="t('desktop.applications')">
    <header class="desktop-header">
      <h2>{{ t("desktop.applications") }}</h2>
    </header>
    <label :for="inputId" class="desktop-caption">{{ t("desktop.searchApplications") }}</label>
    <!-- WebKit's native search decoration loads a data: SVG outside the trusted local origin. -->
    <InputText
      :id="inputId"
      v-model="query"
      class="desktop-search"
      type="text"
      role="searchbox"
      autocomplete="off"
      :placeholder="t('desktop.searchPlaceholder')"
      :disabled="!ready"
      :aria-controls="resultsId"
      :aria-describedby="selectedApplication ? `${resultsId}-${selectedIndex}` : undefined"
      @keydown="onSearchKeydown"
    />
    <p v-if="!ready" class="desktop-empty" role="status">
      {{ t("states.unavailable") }}<template v-if="unavailableReason">: {{ unavailableReason }}</template>
    </p>
    <template v-else>
      <p v-if="searching" class="desktop-caption" role="status">{{ t("desktop.loading") }}</p>
      <p v-if="busy" class="desktop-caption" role="status">{{ t("desktop.loading") }}</p>
      <p v-if="!searching && !busy && !items.length && !searchError" class="desktop-empty" role="status">
        {{ t("desktop.emptyApplications") }}
      </p>
      <ul :id="resultsId" class="desktop-list" :aria-busy="searching || busy">
        <li v-for="(application, index) in items" :key="application.id" class="desktop-item">
          <Button
            :id="`${resultsId}-${index}`"
            class="surface-action desktop-item-main"
            :class="{ 'is-selected': selectedIndex === index }"
            :aria-label="t('desktop.launch', { name: application.name })"
            :aria-current="selectedIndex === index ? 'true' : undefined"
            :disabled="busy || searching"
            @focus="selectedIndex = index"
            @click="launch(application.id)"
          >
            <ShellIcon :icon="application.icon" />
            <span class="desktop-text">
              <span>{{ application.name }}</span>
              <span v-if="application.description" class="desktop-caption">{{ application.description }}</span>
            </span>
          </Button>
        </li>
      </ul>
      <p v-if="searchError || error" class="desktop-error" role="alert">{{ error || searchError }}</p>
    </template>
  </section>
</template>

<script setup lang="ts">
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import { computed, onUnmounted, ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";

import { launchApplication, searchApplications } from "@html-desktop-shell/shell-api";
import type { DesktopApplication } from "@html-desktop-shell/shell-api";

import { nativeErrorMessage, useNativeAction } from "../composables/useNativeAction";
import { useShellStore } from "../stores/shell";
import ShellIcon from "./ShellIcon.vue";

const { t } = useI18n();
const shell = useShellStore();
const { busy, error, run } = useNativeAction();
const inputId = `application-search-${useId()}`;
const resultsId = `${inputId}-results`;
const query = ref("");
const items = ref<DesktopApplication[]>([]);
const searching = ref(false);
const searchError = ref("");
const selectedIndex = ref(0);
const ready = computed(() => shell.state?.applications?.available === true
  && shell.hasCapability("searchApplications") && shell.hasCapability("launchApplication"));
const unavailableReason = computed(() => {
  const applications = shell.state?.applications;
  return applications?.available === false ? applications.reason : "";
});
const selectedApplication = computed(() => items.value[selectedIndex.value]);
let timer: ReturnType<typeof setTimeout> | undefined;
let generation = 0;
let displayedQuery: string | null = null;
let disposed = false;
let activeSearches = 0;
let refreshQueued = false;
let authorizationEpoch = 0;

watch([query, ready, () => shell.state?.applications], ([nextQuery, available], previous) => {
  if (previous[0] === undefined || available !== previous[1]) {
    authorizationEpoch++;
    error.value = "";
  }
  if (previous[0] !== undefined && nextQuery === previous[0] && available === previous[1]) {
    if (available) {
      refreshQueued = true;
      drainRefresh();
    }
    return;
  }
  const requestGeneration = ++generation;
  refreshQueued = false;
  if (timer !== undefined) {
    clearTimeout(timer);
    timer = undefined;
  }
  items.value = [];
  displayedQuery = null;
  selectedIndex.value = 0;
  searchError.value = "";
  searching.value = available;
  if (!available) {
    return;
  }
  if (previous.length > 0 && nextQuery !== previous[0]) {
    timer = setTimeout(() => {
      timer = undefined;
      void search(nextQuery, requestGeneration);
    }, 180);
  } else {
    void search(nextQuery, requestGeneration);
  }
}, { immediate: true, flush: "sync" });

async function search(searchQuery: string, requestGeneration: number): Promise<void> {
  activeSearches++;
  refreshQueued = false;
  try {
    const result = await searchApplications(searchQuery, 25);
    if (!disposed && ready.value && generation === requestGeneration) {
      const selectedId = selectedApplication.value?.id;
      items.value = result.items;
      const selectedPosition = result.items.findIndex((application) => application.id === selectedId);
      selectedIndex.value = selectedPosition < 0 ? 0 : selectedPosition;
      displayedQuery = searchQuery;
      searchError.value = "";
    }
  } catch (cause) {
    if (!disposed && ready.value && generation === requestGeneration) {
      searchError.value = nativeErrorMessage(cause);
    }
  } finally {
    activeSearches--;
    if (!disposed && generation === requestGeneration) {
      searching.value = false;
    }
    drainRefresh();
  }
}

function drainRefresh(): void {
  if (!disposed && ready.value && refreshQueued && activeSearches === 0 && timer === undefined) {
    void search(query.value, generation);
  }
}

function onSearchKeydown(event: KeyboardEvent): void {
  if (event.isComposing || !ready.value || searching.value || busy.value || !items.value.length
    || displayedQuery !== query.value) {
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const direction = event.key === "ArrowDown" ? 1 : -1;
    selectedIndex.value = (selectedIndex.value + direction + items.value.length) % items.value.length;
  } else if (event.key === "Enter") {
    event.preventDefault();
    const application = selectedApplication.value;
    if (application) {
      void launch(application.id);
    }
  }
}

async function launch(applicationId: string): Promise<void> {
  if (disposed || !ready.value || searching.value || busy.value || displayedQuery !== query.value
    || !items.value.some((application) => application.id === applicationId)) {
    return;
  }
  const launchEpoch = authorizationEpoch;
  await run(async () => {
    try {
      const result = await launchApplication(applicationId);
      return !disposed && ready.value && authorizationEpoch === launchEpoch ? result : undefined;
    } catch (cause) {
      if (!disposed && ready.value && authorizationEpoch === launchEpoch) {
        throw cause;
      }
      return undefined;
    }
  });
}

onUnmounted(() => {
  disposed = true;
  generation++;
  if (timer !== undefined) {
    clearTimeout(timer);
  }
});
</script>
