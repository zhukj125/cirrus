import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

const host = process.env.TAURI_DEV_HOST;
const isTauri = !!host || !!process.env.TAURI_ENV_ARCH;

export default defineConfig(async () => ({
  root: import.meta.dirname,
  envDir: path.resolve(import.meta.dirname, "../.."),
  plugins: [vue(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./src"),
    },
  },
  clearScreen: false,
  build: {
    outDir: "../../dist",
    emptyOutDir: true,
    target: "safari15",
    cssTarget: "safari15",
    chunkSizeWarningLimit: 800,
    sourcemap: false,
  },
  server: {
    port: isTauri ? 1420 : 5173,
    strictPort: isTauri,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**", "**/crates/**"],
    },
  },
}));
