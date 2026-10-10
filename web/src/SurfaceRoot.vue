<template>
  <div v-if="!surfaceView" id="surface-loading" class="surface-root surface-loading" role="status">
    {{ bridgeError || t("states.loading") }}
  </div>
  <component v-else :is="surfaceView.component" v-bind="surfaceView.props" />
</template>

<script setup lang="ts">
import { storeToRefs } from "pinia";
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";

import { useShellStore } from "./stores/shell";
import BarView from "./views/BarView.vue";
import DiagnosticsDialogView from "./views/DiagnosticsDialogView.vue";
import QuickPanelView from "./views/QuickPanelView.vue";
import TrayMenuView from "./views/TrayMenuView.vue";
import WorkspaceHudView from "./views/WorkspaceHudView.vue";

const { t } = useI18n();
const shell = useShellStore();
const { hostInfo, bridgeError } = storeToRefs(shell);
const surfaceView = computed(() => {
  const surface = hostInfo.value?.surface;
  if (!surface) {
    return null;
  }

  switch (surface.kind) {
    case "bar":
      return { component: BarView, props: {} };
    case "quickPanel":
      return { component: QuickPanelView, props: {} };
    case "workspaceHud":
      return { component: WorkspaceHudView, props: { surface } };
    case "diagnosticsDialog":
      return { component: DiagnosticsDialogView, props: {} };
    case "trayMenu":
      return { component: TrayMenuView, props: { surface } };
    default:
      return null;
  }
});

onMounted(() => {
  void shell.loadHostInfo();
});
</script>
