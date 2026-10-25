import { createApp } from "vue";
import { createPinia } from "pinia";

import App from "./App.vue";
import { isBlockingCompatFailure } from "./lib/ui/webview-compat";
import "./styles/globals.css";

if (!isBlockingCompatFailure()) {
  createApp(App).use(createPinia()).mount("#app");
}
