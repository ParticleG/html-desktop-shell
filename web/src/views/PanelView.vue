<template>
  <main id="shell-bar" aria-label="Desktop shell panel">
    <section id="left-widgets" class="gutter-x-md" aria-label="Left panel widgets">
      <component
        :is="widgetRegistry[widget]"
        v-for="widget in panelLayout.left"
        :key="widget"
        v-bind="widgetProps"
      />
    </section>
    <section id="center-widgets" class="gutter-x-md" aria-label="Center panel widgets">
      <component
        :is="widgetRegistry[widget]"
        v-for="widget in panelLayout.center"
        :key="widget"
        v-bind="widgetProps"
      />
    </section>
    <section id="right-widgets" class="gutter-x-md" aria-label="Right panel widgets">
      <component
        :is="widgetRegistry[widget]"
        v-for="widget in panelLayout.right"
        :key="widget"
        v-bind="widgetProps"
      />
    </section>
  </main>
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import { computed } from "vue";
import type { Component } from "vue";

import { useShellPolling } from "../composables/useShellPolling";
import { panelLayout } from "../layout";
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

const shell = useShellStore();
const { hostInfo, state, bridgeError, actionError, actionErrorDetail } = storeToRefs(shell);
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
