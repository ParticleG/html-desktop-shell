<template>
  <span class="shell-icon" :style="{ width: `${size}px`, height: `${size}px` }" aria-hidden="true">
    <canvas v-if="boundedIcon" ref="canvas" :style="{ visibility: painted ? 'visible' : 'hidden' }" />
    <svg v-if="!painted" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5">
      <rect x="3" y="3" width="18" height="18" rx="4" />
      <path d="M7 9h10M7 13h6" />
    </svg>
  </span>
</template>

<script setup lang="ts">
import { computed, ref, watch } from "vue";

import type { ShellIcon } from "@html-desktop-shell/shell-api";

const props = withDefaults(defineProps<{ icon: ShellIcon | null; size?: number }>(), { size: 20 });
const canvas = ref<HTMLCanvasElement | null>(null);
const painted = ref(false);
const boundedIcon = computed(() => {
  const icon = props.icon;
  return icon && Number.isInteger(icon.width) && Number.isInteger(icon.height)
    && icon.width >= 1 && icon.width <= 512 && icon.height >= 1 && icon.height <= 512
    && typeof icon.rgba === "string" && icon.rgba.length <= 1_398_104 ? icon : null;
});

watch([canvas, boundedIcon], ([element, icon]) => {
  painted.value = false;
  if (!element || !icon) {
    return;
  }
  try {
    const decoded = atob(icon.rgba);
    if (decoded.length !== icon.width * icon.height * 4) {
      return;
    }
    const context = element.getContext("2d");
    if (!context) {
      return;
    }
    element.width = icon.width;
    element.height = icon.height;
    const image = context.createImageData(icon.width, icon.height);
    for (let index = 0; index < decoded.length; index++) {
      image.data[index] = decoded.charCodeAt(index);
    }
    context.putImageData(image, 0, 0);
    painted.value = true;
  } catch {
    // Absent or malformed optional icon data never enables URL-based loading.
  }
}, { flush: "post" });
</script>
