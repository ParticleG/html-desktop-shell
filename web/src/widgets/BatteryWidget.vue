<template>
  <Tag v-if="text" id="battery-status" class="battery-status panel-tag" :title="title">
    <BatteryCharging v-if="isCharging" class="widget-icon" :size="14" aria-hidden="true" />
    <Battery v-else class="widget-icon" :size="14" aria-hidden="true" />
    <span>{{ text }}</span>
  </Tag>
</template>

<script setup lang="ts">
import { Battery, BatteryCharging } from "@lucide/vue";
import Tag from "primevue/tag";
import { computed } from "vue";

import { batteryText } from "../view-model";
import type { WidgetProps } from "./types";

const props = defineProps<WidgetProps>();
const text = computed(() => batteryText(props.state?.battery));
const isCharging = computed(() => props.state?.battery?.status === "charging");
const title = computed(() => {
  const batteries = props.state?.battery?.batteries;
  if (!Array.isArray(batteries) || batteries.length === 0) {
    return text.value ?? "";
  }

  return batteries
    .map((item) => {
      const name = typeof item.name === "string" ? item.name : "unknown";
      const percentage = typeof item.percentage === "number" && Number.isInteger(item.percentage)
        ? item.percentage
        : "?";
      const status = typeof item.status === "string" ? item.status : "unknown";
      return `${name}: ${percentage}% ${status}`;
    })
    .join(" · ");
});
</script>
