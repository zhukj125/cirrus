<script setup lang="ts">
import { useId } from "vue";

const maskId = useId();
</script>

<template>
  <svg viewBox="0 0 32 32" fill="none" aria-hidden="true">
    <defs>
      <mask :id="maskId" maskUnits="userSpaceOnUse" x="0" y="0" width="32" height="32">
        <rect width="32" height="32" fill="black" />
        <g fill="white">
          <circle cx="11.9" cy="18.1" r="4.8" />
          <circle cx="17.6" cy="14.2" r="5.8" />
          <circle cx="23.1" cy="18.2" r="4.1" />
          <rect x="7.1" y="18.1" width="17.8" height="6.3" rx="3.15" />
        </g>
        <g fill="black">
          <circle cx="12.3" cy="18.4" r="3.1" />
          <circle cx="17.5" cy="15.6" r="4" />
          <circle cx="22" cy="18.6" r="2.8" />
          <rect x="9.8" y="18.7" width="13" height="3.3" rx="1.8" />
        </g>
      </mask>
    </defs>
    <rect width="32" height="32" fill="currentColor" :mask="`url(#${maskId})`" />
    <circle cx="22.1" cy="21.1" r="1.65" fill="var(--primary)" />
  </svg>
</template>
