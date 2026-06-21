import { createPinia } from "pinia";
import { createApp } from "vue";

import App from "./App.vue";
import { i18n } from "./i18n";
import { installPrimeVue } from "./plugins/primevue";
import { router } from "./router";
import "./styles.css";

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.use(i18n);
installPrimeVue(app);
app.mount("#app");
