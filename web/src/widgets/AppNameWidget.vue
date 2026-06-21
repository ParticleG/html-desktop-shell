<template>
  <div id="app-name" class="app-name">{{ t("app.name") }}</div>
</template>

<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { WidgetProps } from "./types";

defineProps<WidgetProps>();

const { t } = useI18n();
</script>
