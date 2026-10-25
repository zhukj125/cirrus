<script setup lang="ts">
import { useLocaleStore } from "@/stores/locale";

const locale = useLocaleStore();
const t = locale.t;
</script>

<template>
  <div class="grid h-full min-h-[280px] place-items-center px-5 py-10">
    <div class="flex flex-col items-center text-center">
      <!-- Adapted from the open-license unDraw "No data" illustration. -->
      <svg class="h-[178px] w-[190px] text-muted-foreground" viewBox="0 0 648 632" fill="none" aria-hidden="true">
        <path
          d="M687.328 276.087H512.818a15.018 15.018 0 0 0-15 15v387.85l-2 .61-42.81 13.11a8.007 8.007 0 0 1-9.99-5.31L315.678 271.397a8.003 8.003 0 0 1 5.31-9.99l65.97-20.2 191.25-58.54 65.97-20.2a7.989 7.989 0 0 1 9.99 5.3l32.55 106.32Z"
          transform="translate(-276.182 -133.913)"
          fill="var(--muted)"
          opacity=".55"
        />
        <path
          d="m725.408 274.087-39.23-128.14a16.994 16.994 0 0 0-21.23-11.28l-92.75 28.39-191.24 58.55-92.75 28.4a17.015 17.015 0 0 0-11.28 21.23l134.08 437.93a17.027 17.027 0 0 0 16.26 12.03 16.789 16.789 0 0 0 4.97-.75l63.58-19.46 2-.62v-2.09l-2 .61-64.17 19.65a15.015 15.015 0 0 1-18.73-9.95l-134.07-437.94a14.979 14.979 0 0 1 9.95-18.73l92.75-28.4 191.24-58.54 92.75-28.4a15.156 15.156 0 0 1 4.41-.66 15.015 15.015 0 0 1 14.32 10.61l39.05 127.56.62 2h2.08Z"
          transform="translate(-276.182 -133.913)"
          fill="currentColor"
          opacity=".82"
        />
        <path
          d="M398.863 261.734a9.016 9.016 0 0 1-8.612-6.367l-12.88-42.072a8.999 8.999 0 0 1 5.971-11.24l175.939-53.864a9.009 9.009 0 0 1 11.241 5.971l12.88 42.072a9.01 9.01 0 0 1-5.971 11.241l-175.939 53.864a8.976 8.976 0 0 1-2.629.395Z"
          transform="translate(-276.182 -133.913)"
          fill="var(--primary)"
        />
        <circle cx="190.154" cy="24.955" r="20" fill="var(--primary)" />
        <circle cx="190.154" cy="24.955" r="12.665" fill="var(--card)" />
        <path d="M878.818 716.087h-338a8.51 8.51 0 0 1-8.5-8.5v-405a8.51 8.51 0 0 1 8.5-8.5h338a8.51 8.51 0 0 1 8.5 8.5v405a8.51 8.51 0 0 1-8.5 8.5Z" transform="translate(-276.182 -133.913)" fill="var(--secondary)" />
        <path
          d="M723.318 274.087h-210.5a17.024 17.024 0 0 0-17 17v407.8l2-.61v-407.19a15.018 15.018 0 0 1 15-15h210.5Zm183.5 0h-394a17.024 17.024 0 0 0-17 17v458a17.024 17.024 0 0 0 17 17h394a17.024 17.024 0 0 0 17-17v-458a17.024 17.024 0 0 0-17-17Zm15 475a15.018 15.018 0 0 1-15 15h-394a15.018 15.018 0 0 1-15-15v-458a15.018 15.018 0 0 1 15-15h394a15.018 15.018 0 0 1 15 15Z"
          transform="translate(-276.182 -133.913)"
          fill="currentColor"
          opacity=".82"
        />
        <path d="M801.818 318.087h-184a9.01 9.01 0 0 1-9-9v-44a9.01 9.01 0 0 1 9-9h184a9.01 9.01 0 0 1 9 9v44a9.01 9.01 0 0 1-9 9Z" transform="translate(-276.182 -133.913)" fill="var(--primary)" />
        <circle cx="433.636" cy="105.174" r="20" fill="var(--primary)" />
        <circle cx="433.636" cy="105.174" r="12.182" fill="var(--card)" />
      </svg>
      <p class="mt-3 text-sm font-medium text-foreground">{{ t("downloads.noDownloads") }}</p>
      <p class="mt-1.5 text-xs text-muted-foreground">{{ t("downloads.historyHere") }}</p>
    </div>
  </div>
</template>
