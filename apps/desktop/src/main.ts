import { createApp } from "vue";
import { createPinia } from "pinia";

import App from "./App.vue";
import { hydrateDesktopSettings } from "./lib/common/storage";
import { isBlockingCompatFailure } from "./lib/ui/webview-compat";
import "./styles/globals.css";

async function bootstrap(): Promise<void> {
  if (isBlockingCompatFailure()) return;
  await hydrateDesktopSettings();
  createApp(App).use(createPinia()).mount("#app");
}

void bootstrap();
