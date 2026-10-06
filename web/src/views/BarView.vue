<template>
  <main id="shell-bar" class="surface-root surface-bar" :aria-label="t('surfaces.bar')">
    <section id="left-widgets" class="gutter-x-md" :aria-label="t('regions.barLeft')">
      <component
        :is="widgetRegistry[widget]"
        v-for="widget in barLayout.left"
        :key="widget"
        v-bind="widgetProps"
      />
    </section>
    <section id="center-widgets" class="gutter-x-md" :aria-label="t('regions.barCenter')">
      <component
        :is="widgetRegistry[widget]"
        v-for="widget in barLayout.center"
        :key="widget"
        v-bind="widgetProps"
      />
    </section>
    <section id="right-widgets" class="gutter-x-md" :aria-label="t('regions.barRight')">
      <component
        :is="widgetRegistry[widget]"
        v-for="widget in barLayout.right"
        :key="widget"
        v-bind="widgetProps"
      />
      <Button
        v-if="canOpenQuickPanel"
        class="surface-action"
        :label="t('actions.openQuickPanel')"
        size="small"
        text
        @click="shell.setQuickPanelOpenFromButton(true)"
      />
      <Button
        v-if="canOpenDiagnostics"
        class="surface-action"
        :label="t('actions.openDiagnostics')"
        size="small"
        text
        @click="shell.setDiagnosticsDialogOpenFromButton(true)"
      />
    </section>
  </main>
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import Button from "primevue/button";
import { computed } from "vue";
import type { Component } from "vue";
import { useI18n } from "vue-i18n";

import { useShellPolling } from "../composables/useShellPolling";
import { barLayout } from "../layout";
import type { WidgetKey } from "../layout";
import { useShellStore } from "../stores/shell";
import ActionStatusWidget from "../widgets/ActionStatusWidget.vue";
import AppNameWidget from "../widgets/AppNameWidget.vue";
import BatteryWidget from "../widgets/BatteryWidget.vue";
import BridgeStatusWidget from "../widgets/BridgeStatusWidget.vue";
import ClockWidget from "../widgets/ClockWidget.vue";
import FocusedWindowWidget from "../widgets/FocusedWindowWidget.vue";
import NetworkWidget from "../widgets/NetworkWidget.vue";
import WorkspacesWidget from "../widgets/WorkspacesWidget.vue";
import type { WidgetProps } from "../widgets/types";

const widgetRegistry: Record<WidgetKey, Component> = {
  appName: AppNameWidget,
  workspaces: WorkspacesWidget,
  focusedWindow: FocusedWindowWidget,
  clock: ClockWidget,
  battery: BatteryWidget,
  network: NetworkWidget,
  actionStatus: ActionStatusWidget,
  bridgeStatus: BridgeStatusWidget,
};

const { t } = useI18n();
const shell = useShellStore();
const { hostInfo, state, bridgeError, actionError, actionErrorDetail } = storeToRefs(shell);
const canOpenQuickPanel = computed(() => shell.hasCapability("setQuickPanelOpen"));
const canOpenDiagnostics = computed(() => shell.hasCapability("setDiagnosticsDialogOpen"));
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
