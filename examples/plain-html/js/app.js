import {
  activateTrayMenuItem,
  closeTrayMenu,
  focusWorkspace,
  getCapabilities,
  getHostInfo,
  getState,
  getTrayMenu,
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
    document.body.classList.toggle("tray-menu-page", hostInfo.surface.kind === "trayMenu");
    if (hostInfo.surface.kind === "trayMenu") {
      const trayMenu = createTrayMenu(hostInfo.surface, methods);
      app.append(trayMenu.element);
      trayMenu.load();
      return;
    }
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
      const output = hostInfo.surface.output;
      const workspaces = output && Array.isArray(state.niri?.workspaces?.items)
        ? state.niri.workspaces.items.filter((workspace) => workspace.output === output)
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

function nativeErrorText(error) {
  if (error?.code) {
    return `${error.code}: ${error.message}`;
  }
  return error instanceof Error ? error.message : "native bridge request failed";
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
      status.textContent = nativeErrorText(error);
    } finally {
      button.disabled = false;
    }
  });
  container.append(button);
}

function createTrayMenu(surface, methods) {
  const itemId = surface.trayItemId;
  const canRead = methods.has("getTrayMenu");
  const canActivate = methods.has("activateTrayMenuItem");
  const canClose = methods.has("closeTrayMenu");
  const expanded = new Set();
  let current = null;
  let pending = false;
  let focusedId = null;

  const element = document.createElement("section");
  element.id = "tray-menu-content";
  element.className = "tray-menu-content";
  element.tabIndex = -1;
  element.setAttribute("role", "region");
  element.setAttribute("aria-label", `Tray menu for ${itemId}`);
  const controls = document.createElement("div");
  controls.className = "tray-menu-controls";
  const loading = document.createElement("p");
  loading.className = "tray-menu-empty";
  loading.setAttribute("role", "status");
  loading.textContent = canRead ? "Loading native menu…" : "Tray menu unavailable for this surface";
  const status = document.createElement("p");
  status.className = "action-status";
  status.setAttribute("role", "alert");
  const tree = document.createElement("div");
  tree.className = "tray-menu-tree";
  element.append(controls, loading, status, tree);

  function isSubmenu(node) {
    return node.hasSubmenu || node.children.length > 0;
  }

  function findNode(nodes, id) {
    for (const node of nodes) {
      if (node.id === id) {
        return node;
      }
      const child = findNode(node.children, id);
      if (child) {
        return child;
      }
    }
    return null;
  }

  function appendNodes(list, nodes) {
    for (const node of nodes) {
      if (!node.visible) {
        continue;
      }
      const item = document.createElement("li");
      item.className = "tray-menu-node";
      item.setAttribute("role", "none");
      if (node.type === "separator") {
        const separator = document.createElement("div");
        separator.setAttribute("role", "separator");
        item.append(separator);
      } else {
        const submenu = isSubmenu(node);
        const button = document.createElement("button");
        button.type = "button";
        button.dataset.menuId = String(node.id);
        button.setAttribute("role", node.toggleType === "checkmark" ? "menuitemcheckbox" : node.toggleType === "radio" ? "menuitemradio" : "menuitem");
        if (node.toggleType === "checkmark") {
          button.setAttribute("aria-checked", node.toggleState === 1 ? "true" : node.toggleState === 0 ? "false" : "mixed");
        }
        if (node.toggleType === "radio") {
          button.setAttribute("aria-checked", node.toggleState === 1 ? "true" : "false");
        }
        if (submenu) {
          button.setAttribute("aria-haspopup", "menu");
          button.setAttribute("aria-expanded", String(expanded.has(node.id)));
        }
        button.disabled = pending || !node.enabled || (submenu ? !canRead : !canActivate);
        if (node.toggleType) {
          const mark = document.createElement("span");
          mark.className = "tray-toggle";
          mark.setAttribute("aria-hidden", "true");
          mark.textContent = node.toggleState === 1 ? node.toggleType === "radio" ? "●" : "✓" : node.toggleState < 0 ? "−" : "";
          button.append(mark);
        }
        const label = document.createElement("span");
        label.textContent = node.label || String(node.id);
        button.append(label);
        if (submenu) {
          const caret = document.createElement("span");
          caret.setAttribute("aria-hidden", "true");
          caret.textContent = expanded.has(node.id) ? "▾" : "▸";
          button.append(caret);
        }
        item.append(button);
        if (submenu && expanded.has(node.id) && node.children.length > 0) {
          const nested = document.createElement("ul");
          nested.className = "tray-menu";
          nested.setAttribute("role", "menu");
          nested.dataset.submenuId = String(node.id);
          nested.setAttribute("aria-label", node.label || String(node.id));
          appendNodes(nested, node.children);
          item.append(nested);
        }
      }
      list.append(item);
    }
  }

  function renderTree() {
    tree.replaceChildren();
    if (current?.root.visible) {
      const list = document.createElement("ul");
      list.className = "tray-menu";
      list.setAttribute("role", "menu");
      list.setAttribute("aria-label", `Tray menu for ${itemId}`);
      appendNodes(list, current.root.children);
      tree.append(list);
    }
    const focused = focusedId === null ? null : tree.querySelector(`[data-menu-id="${focusedId}"]`);
    (focused ?? tree.querySelector("button[data-menu-id]:not(:disabled)") ?? element).focus();
  }

  async function run(action) {
    status.textContent = "";
    try {
      await action();
      return true;
    } catch (error) {
      status.textContent = nativeErrorText(error);
      return false;
    }
  }

  async function load(menuItemId = 0) {
    if (!canRead || pending) {
      return;
    }
    pending = true;
    renderTree();
    await run(async () => {
      const result = await getTrayMenu(itemId, menuItemId);
      if (result.itemId !== itemId) {
        throw new Error("Native tray menu returned a different item");
      }
      current = result;
      if (menuItemId !== 0) {
        expanded.add(menuItemId);
      }
      for (const id of [...expanded]) {
        const node = findNode([result.root], id);
        if (!node || !isSubmenu(node) || node.children.length === 0) {
          expanded.delete(id);
        }
      }
    });
    pending = false;
    loading.textContent = "";
    renderTree();
  }

  async function closeMenu() {
    if (pending || !canClose) {
      return;
    }
    pending = true;
    renderTree();
    await run(() => closeTrayMenu());
    pending = false;
    renderTree();
  }

  async function activate(node) {
    const menu = current;
    if (!menu || pending || !canActivate || !node.enabled) {
      return;
    }
    pending = true;
    renderTree();
    const activated = await run(() => activateTrayMenuItem(itemId, node.id, menu.revision));
    pending = false;
    if (activated) {
      await closeMenu();
      return;
    }
    renderTree();
  }

  function focusSubmenu(nodeId) {
    const first = tree.querySelector(`[data-submenu-id="${nodeId}"] button[data-menu-id]:not(:disabled)`);
    if (first) {
      focusedId = Number(first.dataset.menuId);
      first.focus();
    }
  }

  function nodeForButton(button) {
    return button && current ? findNode([current.root], Number(button.dataset.menuId)) : null;
  }

  element.addEventListener("focusin", (event) => {
    if (event.target instanceof HTMLElement && event.target.dataset.menuId) {
      focusedId = Number(event.target.dataset.menuId);
    }
  });

  element.addEventListener("click", (event) => {
    const button = event.target instanceof Element ? event.target.closest("button[data-menu-id]") : null;
    const node = nodeForButton(button);
    if (!node) {
      return;
    }
    focusedId = node.id;
    if (!isSubmenu(node)) {
      void activate(node);
      return;
    }
    if (expanded.has(node.id)) {
      expanded.delete(node.id);
      renderTree();
      return;
    }
    void load(node.id).then(() => focusSubmenu(node.id));
  });

  element.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      void closeMenu();
      return;
    }
    const buttons = [...tree.querySelectorAll("button[data-menu-id]:not(:disabled)")];
    if (buttons.length === 0) {
      return;
    }
    const active = document.activeElement;
    const index = buttons.indexOf(active);
    if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
      event.preventDefault();
      const next = event.key === "Home" ? 0
        : event.key === "End" ? buttons.length - 1
        : event.key === "ArrowDown" ? (index + 1) % buttons.length
        : index <= 0 ? buttons.length - 1 : index - 1;
      buttons[next].focus();
      return;
    }
    const node = active instanceof HTMLElement && active.dataset.menuId
      ? nodeForButton(active)
      : null;
    if (event.key === "ArrowRight" && node && isSubmenu(node) && !pending) {
      event.preventDefault();
      if (expanded.has(node.id)) {
        focusSubmenu(node.id);
      } else {
        void load(node.id).then(() => focusSubmenu(node.id));
      }
      return;
    }
    if (event.key === "ArrowLeft") {
      const nested = active instanceof HTMLElement ? active.closest("ul[data-submenu-id]") : null;
      if (!nested) {
        return;
      }
      event.preventDefault();
      expanded.delete(Number(nested.dataset.submenuId));
      const parent = nested.closest("li")?.querySelector("button[data-menu-id]");
      focusedId = parent ? Number(parent.dataset.menuId) : focusedId;
      renderTree();
    }
  });

  appendAction(controls, "Refresh", () => load(), status);
  appendAction(controls, "Close menu", () => closeMenu(), status);

  return { element, load };
}

void render();
