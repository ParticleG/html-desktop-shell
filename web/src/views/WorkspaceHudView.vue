<template>
  <main id="workspace-hud" class="surface-root surface-workspace-hud" :aria-label="t('surfaces.workspaceHud')">
    <span class="workspace-hud-label">{{ t("surfaces.workspace") }}</span>
    <strong class="workspace-hud-index">{{ surface.workspaceIndex }}</strong>
    <span v-if="surface.output" class="workspace-hud-output">{{ surface.output }}</span>
  </main>
</template>

<script setup lang="ts">
import type { WorkspaceHudSurfaceContext } from "@html-desktop-shell/shell-api";
import { useI18n } from "vue-i18n";

defineProps<{ surface: WorkspaceHudSurfaceContext }>();

const { t } = useI18n();
</script>
