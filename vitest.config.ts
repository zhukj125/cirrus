import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";
import path from "path";

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "apps/desktop/src"),
    },
  },
  test: {
    env: {
      VITE_AMAP_KEY: "",
      VITE_AMAP_SECURITY_CODE: "",
    },
    environment: "happy-dom",
    include: ["apps/desktop/src/**/__tests__/**/*.test.ts"],
    restoreMocks: true,
  },
});
