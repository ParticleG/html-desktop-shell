import Aura from "@primeuix/themes/aura";
import PrimeVue from "primevue/config";
import type { App } from "vue";

export function installPrimeVue(app: App): void {
  app.use(PrimeVue, {
    theme: {
      preset: Aura,
      options: {
        darkModeSelector: "system",
        cssLayer: false,
      },
    },
  });
}
