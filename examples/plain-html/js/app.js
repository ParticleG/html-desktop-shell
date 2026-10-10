import {
  focusWorkspace,
  getCapabilities,
  getHostInfo,
  getState,
  setDiagnosticsDialogOpen,
  setQuickPanelOpen,
} from "./bridge.js";

const app = document.getElementById("app");

async function render() {
  try {
    const [hostInfo, capabilities] = await Promise.all([getHostInfo(), getCapabilities()]);
    if (hostInfo.bridgeVersion !== 6) {
      throw new Error("This frontend requires Bridge v6");
    }
    const methods = new Set(capabilities.methods);
    const state = methods.has("getState") ? await getState() : null;

    app.replaceChildren();
    const heading = document.createElement("h1");
    heading.textContent = "HTML Shell bridge demo";
    app.append(heading);

    const details = document.createElement("dl");
    appendDetail(details, "Backend", hostInfo.backend);
    appendDetail(details, "Bridge version", String(hostInfo.bridgeVersion));
    appendDetail(details, "Surface", hostInfo.surface.kind);
    appendDetail(details, "Monitor", String(hostInfo.surface.monitorIndex));
    appendDetail(details, "Output", hostInfo.surface.output ?? "unknown output");
    if (hostInfo.surface.kind === "workspaceHud") {
      appendDetail(details, "Workspace", String(hostInfo.surface.workspaceIndex));
    }
    appendDetail(details, "Capabilities", capabilities.methods.join(", ") || "none");
    if (state) {
      appendDetail(details, "Clock", state.clock?.time ?? "--:--:--");
      appendDetail(details, "Monitor count", String(state.host?.monitorCount ?? "unknown"));
      appendDetail(details, "niri", state.niri?.available ? "available" : "unavailable");
    }
    app.append(details);

    const actions = document.createElement("section");
    actions.className = "actions";
    actions.setAttribute("aria-label", "Available surface actions");
    const actionStatus = document.createElement("p");
    actionStatus.className = "action-status";
    actionStatus.setAttribute("role", "status");

    if (methods.has("setQuickPanelOpen")) {
      const quickPanelOpen = hostInfo.surface.kind !== "quickPanel";
      appendAction(actions, quickPanelOpen ? "Open Quick Panel" : "Close Quick Panel", async () => {
        await setQuickPanelOpen(quickPanelOpen);
      }, actionStatus);
    }
    if (methods.has("setDiagnosticsDialogOpen")) {
      const diagnosticsOpen = hostInfo.surface.kind !== "diagnosticsDialog";
      appendAction(actions, diagnosticsOpen ? "Open Diagnostics" : "Close Diagnostics", async () => {
        await setDiagnosticsDialogOpen(diagnosticsOpen);
      }, actionStatus);
    }
    if (state && methods.has("niriFocusWorkspace")) {
      const workspaces = Array.isArray(state.niri?.workspaces?.items)
        ? state.niri.workspaces.items.filter((workspace) => {
          return !hostInfo.surface.output || workspace.output === hostInfo.surface.output;
        })
        : [];
      for (const workspace of workspaces) {
        if (!Number.isInteger(workspace.index) || workspace.index <= 0) {
          continue;
        }
        appendAction(actions, `Focus workspace ${workspace.name || workspace.index}`, async () => {
          await focusWorkspace(workspace.index);
        }, actionStatus);
      }
    }

    if (actions.childElementCount > 0) {
      app.append(actions, actionStatus);
    }
  } catch (error) {
    app.textContent = error instanceof Error ? error.message : "native bridge unavailable";
  }
}

function appendDetail(list, term, value) {
  const name = document.createElement("dt");
  name.textContent = term;
  const description = document.createElement("dd");
  description.textContent = value;
  list.append(name, description);
}

function appendAction(container, label, action, status) {
  const button = document.createElement("button");
  button.type = "button";
  button.textContent = label;
  button.addEventListener("click", async () => {
    button.disabled = true;
    status.textContent = "";
    try {
      await action();
    } catch (error) {
      status.textContent = error instanceof Error ? error.message : "native bridge request failed";
    } finally {
      button.disabled = false;
    }
  });
  container.append(button);
}

void render();
