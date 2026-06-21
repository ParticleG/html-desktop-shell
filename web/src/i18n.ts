import { createI18n } from "vue-i18n";

const messages = {
  en: {
    app: {
      name: "HTML Shell",
    },
    widgets: {
      workspaces: "Workspaces",
    },
    states: {
      workspacesUnavailable: "workspaces: unavailable",
      workspacesNone: "workspaces: none",
      windowUnavailable: "window: unavailable",
      noFocusedWindow: "no focused window",
    },
    actions: {
      workspaceSwitchFailed: "workspace switch failed",
    },
    bridge: {
      unavailable: "bridge: unavailable",
    },
  },
};

export const i18n = createI18n({
  legacy: false,
  locale: "en",
  fallbackLocale: "en",
  messages,
});
