<template>
  <Tag id="network-status" class="network-status panel-tag" :title="title">
    <Wifi v-if="hasNetworkUp" class="widget-icon" :size="14" aria-hidden="true" />
    <WifiOff v-else class="widget-icon" :size="14" aria-hidden="true" />
    <span>{{ text }}</span>
  </Tag>
</template>

<script setup lang="ts">
import { Wifi, WifiOff } from "@lucide/vue";
import Tag from "primevue/tag";
import { computed } from "vue";

import { networkText } from "../view-model";
import type { WidgetProps } from "./types";

const props = defineProps<WidgetProps>();
const text = computed(() => networkText(props.state?.network));
const hasNetworkUp = computed(() => {
  const network = props.state?.network;
  return (network?.wired?.up ?? 0) > 0 || (network?.wireless?.up ?? 0) > 0;
});
const title = computed(() => {
  const interfaces = props.state?.network?.interfaces;
  if (!Array.isArray(interfaces) || interfaces.length === 0) {
    return text.value;
  }

  return interfaces
    .map((item) => {
      const name = typeof item.name === "string" ? item.name : "unknown";
      const kind = typeof item.kind === "string" ? item.kind : "unknown";
      const state = typeof item.state === "string" ? item.state : "unknown";
      return `${name}: ${kind} ${state}`;
    })
    .join(" · ");
});
</script>
