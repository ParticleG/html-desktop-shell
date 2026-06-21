<template>
  <div id="workspace-status" class="workspace-status" aria-label="Niri workspaces">
    <Monitor class="widget-icon" :size="14" aria-hidden="true" />
    <span v-if="!workspacesAvailable">{{ t("states.workspacesUnavailable") }}</span>
    <span v-else-if="workspaces.length === 0">{{ t("states.workspacesNone") }}</span>
    <template v-else>
      <Button
        v-for="workspace in workspaces"
        :key="workspace.id ?? `${workspace.output ?? 'unknown'}:${workspace.index ?? 'unknown'}`"
        class="workspace-item"
        :class="{ 'is-active': workspace.isActive, 'is-focused': workspace.isFocused }"
        :aria-current="workspace.isFocused ? 'true' : undefined"
        :label="workspaceLabel(workspace)"
        :title="workspaceTitle(workspace)"
        rounded
        size="small"
        text
        @keydown="handleWorkspaceKeyDown($event, workspace)"
        @pointerdown="handleWorkspacePointerDown($event, workspace)"
      />
    </template>
  </div>
</template>

<script setup lang="ts">
import { Monitor } from "@lucide/vue";
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { NiriWorkspace } from "@html-desktop-shell/shell-api";

import { visibleWorkspaces, workspaceLabel, workspaceTitle } from "../view-model";
import type { WidgetProps } from "./types";

const props = defineProps<WidgetProps>();
const { t } = useI18n();
const workspaces = computed(() => visibleWorkspaces(props.state, props.hostInfo?.panel.output));
const workspacesAvailable = computed(() => props.state?.niri?.workspaces?.available === true);

function focusWorkspaceFromEvent(workspace: NiriWorkspace): void {
  const workspaceId = workspace.id;
  if (typeof workspaceId !== "number" || !Number.isInteger(workspaceId) || workspaceId <= 0) {
    return;
  }
  void props.focusWorkspaceFromButton(workspaceId);
}

function handleWorkspacePointerDown(event: PointerEvent, workspace: NiriWorkspace): void {
  if (event.button !== 0) {
    return;
  }
  event.preventDefault();
  focusWorkspaceFromEvent(workspace);
}

function handleWorkspaceKeyDown(event: KeyboardEvent, workspace: NiriWorkspace): void {
  if (event.key !== "Enter" && event.key !== " ") {
    return;
  }
  event.preventDefault();
  focusWorkspaceFromEvent(workspace);
}
</script>
