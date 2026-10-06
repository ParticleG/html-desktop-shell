<template>
  <main id="diagnostics-dialog" class="surface-root surface-diagnostics" :aria-label="t('surfaces.diagnosticsDialog')">
    <header class="surface-header">
      <h1>{{ t("surfaces.diagnosticsDialog") }}</h1>
      <Button
        v-if="canClose"
        class="surface-action"
        :label="t('actions.closeDiagnostics')"
        size="small"
        text
        @click="shell.setDiagnosticsDialogOpenFromButton(false)"
      />
      <span v-else class="surface-unavailable">{{ t("states.diagnosticsCloseUnavailable") }}</span>
    </header>

    <section class="diagnostics-section" :aria-label="t('diagnostics.host')">
      <h2>{{ t("diagnostics.host") }}</h2>
      <dl>
        <dt>{{ t("diagnostics.backend") }}</dt><dd>{{ hostInfo?.backend }}</dd>
        <dt>{{ t("diagnostics.bridgeVersion") }}</dt><dd>{{ hostInfo?.bridgeVersion }}</dd>
        <dt>{{ t("diagnostics.surface") }}</dt><dd>{{ hostInfo?.surface.kind }}</dd>
        <dt>{{ t("diagnostics.output") }}</dt><dd>{{ hostInfo?.surface.output ?? t("states.unavailable") }}</dd>
      </dl>
    </section>

    <section class="diagnostics-section" :aria-label="t('diagnostics.capabilities')">
      <h2>{{ t("diagnostics.capabilities") }}</h2>
      <ul v-if="capabilities?.methods.length">
        <li v-for="method in capabilities.methods" :key="method">{{ method }}</li>
      </ul>
      <p v-else>{{ t("states.none") }}</p>
    </section>

    <section class="diagnostics-section" :aria-label="t('diagnostics.providers')">
      <h2>{{ t("diagnostics.providers") }}</h2>
      <dl>
        <template v-for="provider in providers" :key="provider.name">
          <dt>{{ provider.name }}</dt><dd>{{ provider.availability }}</dd>
        </template>
      </dl>
      <p v-if="bridgeError" class="surface-error" role="status">{{ bridgeError }}</p>
      <ActionStatusWidget v-bind="widgetProps" />
    </section>
  </main>
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useShellPolling } from "../composables/useShellPolling";
import { useShellStore } from "../stores/shell";
import ActionStatusWidget from "../widgets/ActionStatusWidget.vue";
import type { WidgetProps } from "../widgets/types";

const { t } = useI18n();
const shell = useShellStore();
const { hostInfo, capabilities, state, bridgeError, actionError, actionErrorDetail } = storeToRefs(shell);
const canClose = computed(() => shell.hasCapability("setDiagnosticsDialogOpen"));
const providers = computed(() => [
  {
    name: "niri",
    availability: state.value?.niri?.available === true
      ? t("states.available")
      : state.value?.niri?.reason ?? t("states.unavailable"),
  },
  {
    name: t("diagnostics.battery"),
    availability: state.value?.battery?.available === true
      ? t("states.available")
      : state.value?.battery?.reason ?? t("states.unavailable"),
  },
  {
    name: t("diagnostics.network"),
    availability: state.value?.network?.available === true
      ? t("states.available")
      : state.value?.network?.reason ?? t("states.unavailable"),
  },
]);
const widgetProps = computed<WidgetProps>(() => ({
  state: state.value,
  hostInfo: hostInfo.value,
  bridgeError: bridgeError.value,
  actionError: actionError.value,
  actionErrorDetail: actionErrorDetail.value,
  focusWorkspaceFromButton: shell.focusWorkspaceFromButton,
}));

useShellPolling();
</script>
