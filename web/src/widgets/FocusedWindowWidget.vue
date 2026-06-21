<template>
  <div id="focused-window" class="focused-window widget-with-icon" :title="text">
    <Terminal class="widget-icon" :size="14" aria-hidden="true" />
    <span>{{ text }}</span>
  </div>
</template>

<script setup lang="ts">
import { Terminal } from "@lucide/vue";
import { computed } from "vue";

import { focusedWindowText } from "../view-model";
import type { WidgetProps } from "./types";

const props = defineProps<WidgetProps>();
const text = computed(() => focusedWindowText(props.state?.niri?.focusedWindow));
</script>
