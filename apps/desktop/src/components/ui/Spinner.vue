<script setup lang="ts">
import { LoaderCircle } from "@lucide/vue";

withDefaults(defineProps<{ size?: number }>(), { size: 20 });
</script>

<template>
  <LoaderCircle class="animate-spin text-muted-foreground" :style="{ width: `${size}px`, height: `${size}px` }" />
</template>
