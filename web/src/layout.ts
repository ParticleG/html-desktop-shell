export type WidgetKey =
  | "appName"
  | "workspaces"
  | "focusedWindow"
  | "clock"
  | "battery"
  | "network"
  | "actionStatus"
  | "bridgeStatus";

export interface BarLayout {
  left: WidgetKey[];
  center: WidgetKey[];
  right: WidgetKey[];
}

export const barLayout: BarLayout = {
  left: ["appName", "workspaces", "focusedWindow"],
  center: ["clock"],
  right: ["battery", "network", "actionStatus", "bridgeStatus"],
};
