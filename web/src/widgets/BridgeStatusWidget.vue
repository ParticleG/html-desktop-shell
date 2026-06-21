<template>
  <div id="bridge-status" class="bridge-status">{{ text }}</div>
</template>

<script setup lang="ts">
import { computed } from "vue";

import { bridgeStatusText } from "../view-model";
import type { WidgetProps } from "./types";

const props = defineProps<WidgetProps>();
const text = computed(() => props.bridgeError || bridgeStatusText(props.state));
</script>
