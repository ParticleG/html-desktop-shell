import type { HostInfo, ShellState } from "@html-desktop-shell/shell-api";

export interface WidgetProps {
  state: ShellState | null;
  hostInfo: HostInfo | null;
  bridgeError: string;
  actionError: string;
  actionErrorDetail: string;
  focusWorkspaceFromButton: (workspaceId: number) => Promise<void>;
}
