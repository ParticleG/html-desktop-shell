<template>
  <div
    v-if="actionError"
    id="action-status"
    class="action-status widget-with-icon"
    role="status"
    aria-live="polite"
    :title="actionErrorDetail"
  >
    <AlertTriangle class="widget-icon" :size="14" aria-hidden="true" />
    <span>{{ actionError }}</span>
  </div>
</template>

<script setup lang="ts">
import { AlertTriangle } from "@lucide/vue";
import { computed } from "vue";

import type { WidgetProps } from "./types";

const props = defineProps<WidgetProps>();
const actionError = computed(() => props.actionError);
const actionErrorDetail = computed(() => props.actionErrorDetail);
</script>
