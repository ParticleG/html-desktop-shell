<template>
  <main id="quick-panel" class="surface-root surface-quick-panel" :aria-label="t('surfaces.quickPanel')">
    <header class="surface-header">
      <h1>{{ t("surfaces.quickPanel") }}</h1>
      <Button
        v-if="canClose"
        class="surface-action"
        :label="t('actions.closeQuickPanel')"
        size="small"
        text
        @click="shell.setQuickPanelOpenFromButton(false)"
      />
    </header>

    <section class="quick-panel-section" :aria-label="t('widgets.workspaces')">
      <WorkspacesWidget v-bind="widgetProps" />
    </section>
    <section class="quick-panel-section provider-summaries" :aria-label="t('regions.providerSummaries')">
      <FocusedWindowWidget v-bind="widgetProps" />
      <BatteryWidget v-bind="widgetProps" />
      <NetworkWidget v-bind="widgetProps" />
      <BridgeStatusWidget v-bind="widgetProps" />
      <ActionStatusWidget v-bind="widgetProps" />
    </section>

    <Tabs v-model:value="desktopTab" class="desktop-tabs" lazy scrollable>
      <TabList :aria-label="t('regions.desktopControls')">
        <Tab value="audio">{{ t("desktop.audio") }}</Tab>
        <Tab value="tray">{{ t("desktop.tray") }}</Tab>
        <Tab value="notifications">{{ t("desktop.notifications") }}</Tab>
        <Tab value="applications">{{ t("desktop.applications") }}</Tab>
      </TabList>
      <TabPanels>
        <TabPanel value="audio"><AudioControls /></TabPanel>
        <TabPanel value="tray"><TrayControls /></TabPanel>
        <TabPanel value="notifications"><NotificationCenter /></TabPanel>
        <TabPanel value="applications"><ApplicationLauncher /></TabPanel>
      </TabPanels>
    </Tabs>

    <footer v-if="canOpenDiagnostics" class="surface-footer">
      <Button
        class="surface-action"
        :label="t('actions.openDiagnostics')"
        size="small"
        @click="shell.setDiagnosticsDialogOpenFromButton(true)"
      />
    </footer>
  </main>
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import Button from "primevue/button";
import Tab from "primevue/tab";
import TabList from "primevue/tablist";
import TabPanel from "primevue/tabpanel";
import TabPanels from "primevue/tabpanels";
import Tabs from "primevue/tabs";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useShellPolling } from "../composables/useShellPolling";
import { useShellStore } from "../stores/shell";
import ActionStatusWidget from "../widgets/ActionStatusWidget.vue";
import ApplicationLauncher from "../widgets/ApplicationLauncher.vue";
import AudioControls from "../widgets/AudioControls.vue";
import BatteryWidget from "../widgets/BatteryWidget.vue";
import BridgeStatusWidget from "../widgets/BridgeStatusWidget.vue";
import FocusedWindowWidget from "../widgets/FocusedWindowWidget.vue";
import NetworkWidget from "../widgets/NetworkWidget.vue";
import NotificationCenter from "../widgets/NotificationCenter.vue";
import TrayControls from "../widgets/TrayControls.vue";
import WorkspacesWidget from "../widgets/WorkspacesWidget.vue";
import type { WidgetProps } from "../widgets/types";

const { t } = useI18n();
const shell = useShellStore();
const { hostInfo, state, bridgeError, actionError, actionErrorDetail } = storeToRefs(shell);
const desktopTab = ref("audio");
const canClose = computed(() => shell.hasCapability("setQuickPanelOpen"));
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
