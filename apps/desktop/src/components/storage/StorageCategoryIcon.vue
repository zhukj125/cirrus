<script setup lang="ts">
import { Cloud, FileText, Mail, MessageSquare, Smartphone } from "@lucide/vue";

defineProps<{ mediaKey: string }>();

const petalColors = ["#ffd60a", "#ff9f0a", "#ff375f", "#bf5af2", "#5e5ce6", "#0a84ff", "#30d158", "#a8e05f"];
</script>

<template>
  <span class="grid size-11 shrink-0 place-items-center overflow-hidden rounded-[12px] shadow-sm" aria-hidden="true">
    <svg v-if="mediaKey === 'photos'" class="size-full" viewBox="0 0 40 40">
      <rect width="40" height="40" fill="#fff" />
      <g transform="translate(20 20)">
        <ellipse v-for="(color, index) in petalColors" :key="color" cx="0" cy="-7" rx="3.2" ry="6.2" :fill="color" :transform="`rotate(${index * 45})`" opacity="0.96" />
        <circle r="3.2" fill="#fff" />
      </g>
    </svg>

    <span v-else-if="mediaKey === 'docs'" class="grid size-full place-items-center bg-gradient-to-br from-[#64d2ff] to-[#0a84ff] text-white">
      <FileText :size="20" stroke-width="1.8" />
    </span>

    <span v-else-if="mediaKey === 'messages'" class="grid size-full place-items-center bg-gradient-to-br from-[#67e888] to-[#00a854] text-white">
      <MessageSquare :size="19" class="fill-white" stroke-width="1.4" />
    </span>

    <span v-else-if="mediaKey === 'backup'" class="grid size-full place-items-center bg-gradient-to-br from-[#bf5af2] to-[#5e5ce6] text-white">
      <Smartphone :size="19" />
    </span>

    <span v-else-if="mediaKey === 'mail'" class="grid size-full place-items-center bg-gradient-to-br from-[#64d2ff] to-[#0a84ff] text-white">
      <Mail :size="20" class="fill-white" stroke-width="1.4" />
    </span>

    <span v-else class="grid size-full place-items-center bg-gradient-to-br from-[#8e8e93] to-[#636366] text-white">
      <Cloud :size="20" />
    </span>
  </span>
</template>
