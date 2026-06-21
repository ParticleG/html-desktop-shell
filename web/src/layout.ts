export type WidgetKey =
  | "appName"
  | "workspaces"
  | "focusedWindow"
  | "clock"
  | "battery"
  | "network"
  | "actionStatus"
  | "bridgeStatus";

export interface PanelLayout {
  left: WidgetKey[];
  center: WidgetKey[];
  right: WidgetKey[];
}

export const panelLayout: PanelLayout = {
  left: ["appName", "workspaces", "focusedWindow"],
  center: ["clock"],
  right: ["battery", "network", "actionStatus", "bridgeStatus"],
};
