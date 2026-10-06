import { createI18n } from "vue-i18n";

const messages = {
  en: {
    app: {
      name: "HTML Shell",
    },
    surfaces: {
      bar: "Desktop shell bar",
      quickPanel: "Quick Panel",
      workspaceHud: "Workspace HUD",
      workspace: "Workspace",
      diagnosticsDialog: "Diagnostics",
    },
    regions: {
      barLeft: "Left bar widgets",
      barCenter: "Center bar widgets",
      barRight: "Right bar widgets",
      providerSummaries: "Provider summaries",
    },
    widgets: {
      workspaces: "Workspaces",
    },
    diagnostics: {
      host: "Host",
      backend: "Backend",
      bridgeVersion: "Bridge version",
      surface: "Surface",
      output: "Output",
      capabilities: "Capabilities",
      providers: "Providers",
      battery: "Battery",
      network: "Network",
    },
    states: {
      workspacesUnavailable: "workspaces: unavailable",
      workspacesNone: "workspaces: none",
      windowUnavailable: "window: unavailable",
      noFocusedWindow: "no focused window",
      loading: "Loading native surface…",
      available: "available",
      unavailable: "unavailable",
      none: "none",
      diagnosticsCloseUnavailable: "diagnostics close unavailable",
    },
    actions: {
      workspaceSwitchFailed: "workspace switch failed",
      openQuickPanel: "Open Quick Panel",
      closeQuickPanel: "Close Quick Panel",
      openDiagnostics: "Open Diagnostics",
      closeDiagnostics: "Close Diagnostics",
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
