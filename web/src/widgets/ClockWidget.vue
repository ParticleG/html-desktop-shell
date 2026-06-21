<template>
  <div id="clock" class="clock widget-with-icon">
    <Clock class="widget-icon" :size="14" aria-hidden="true" />
    <span>{{ time }}</span>
  </div>
</template>

<script setup lang="ts">
import { Clock } from "@lucide/vue";
import { computed } from "vue";

import type { WidgetProps } from "./types";

const props = defineProps<WidgetProps>();
const time = computed(() => props.state?.clock?.time || "--:--:--");
</script>
