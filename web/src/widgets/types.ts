import type { HostInfo, NiriWorkspace, ShellState } from "@html-desktop-shell/shell-api";

export interface WidgetProps {
  state: ShellState | null;
  hostInfo: HostInfo | null;
  bridgeError: string;
  actionError: string;
  actionErrorDetail: string;
  focusWorkspaceFromButton: (workspaceIndex: number) => Promise<void>;
}

export function workspaceIndexForFocus(workspace: NiriWorkspace): number | null {
  const workspaceIndex = workspace.index;
  return typeof workspaceIndex === "number" && Number.isInteger(workspaceIndex) && workspaceIndex > 0
    ? workspaceIndex
    : null;
}
