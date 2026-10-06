use std::{cell::RefCell, collections::BTreeMap, process::Command, rc::Rc};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{Provider, niri_collector::NiriCollector};
use crate::messages::NativeMethodError;

const UNAVAILABLE_REASON: &str = "niri IPC unavailable";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceActivation {
    pub(crate) output: String,
    pub(crate) workspace_index: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct WorkspaceActivationHandlerId(u64);

type WorkspaceActivationHandler = Rc<dyn Fn(WorkspaceActivation)>;
pub(super) type SharedWorkspaceActivationObservers = Rc<RefCell<WorkspaceActivationObservers>>;

#[derive(Default)]
pub(super) struct WorkspaceActivationObservers {
    next_id: u64,
    handlers: BTreeMap<WorkspaceActivationHandlerId, WorkspaceActivationHandler>,
}

impl WorkspaceActivationObservers {
    pub(super) fn connect(
        &mut self,
        handler: impl Fn(WorkspaceActivation) + 'static,
    ) -> WorkspaceActivationHandlerId {
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("workspace activation handler IDs exhausted");
        let id = WorkspaceActivationHandlerId(self.next_id);
        self.handlers.insert(id, Rc::new(handler));
        id
    }

    pub(super) fn disconnect(&mut self, id: WorkspaceActivationHandlerId) {
        self.handlers.remove(&id);
    }

    pub(super) fn callbacks(&self) -> Vec<WorkspaceActivationHandler> {
        self.handlers.values().cloned().collect()
    }
}

pub(super) type SharedNiriPublication = Rc<RefCell<NiriPublication>>;

pub(super) struct NiriPublication {
    pub(super) snapshot: serde_json::Value,
    pub(super) workspace_indices: Vec<u64>,
}

impl NiriPublication {
    pub(super) fn unavailable() -> Self {
        Self {
            snapshot: unavailable_snapshot(),
            workspace_indices: Vec::new(),
        }
    }

    pub(super) fn publish_unavailable(&mut self) {
        self.snapshot = unavailable_snapshot();
        self.workspace_indices.clear();
    }

    pub(super) fn publish(&mut self, update: ReducerPublication) {
        self.snapshot = update.snapshot;
        self.workspace_indices = update.workspace_indices;
    }
}

#[derive(Clone)]
pub(super) struct NiriController {
    inner: Rc<NiriControllerInner>,
}

struct NiriControllerInner {
    publication: SharedNiriPublication,
    observers: SharedWorkspaceActivationObservers,
    collector: NiriCollector,
}

impl Drop for NiriControllerInner {
    fn drop(&mut self) {
        self.collector.shutdown();
    }
}

pub(super) struct NiriProvider {
    niri: NiriController,
}

impl NiriController {
    pub(super) fn new() -> Self {
        let publication = Rc::new(RefCell::new(NiriPublication::unavailable()));
        let observers = Rc::new(RefCell::new(WorkspaceActivationObservers::default()));
        let collector = NiriCollector::start(Rc::clone(&publication), Rc::clone(&observers));
        Self {
            inner: Rc::new(NiriControllerInner {
                publication,
                observers,
                collector,
            }),
        }
    }

    #[cfg(test)]
    pub(super) fn stopped() -> Self {
        let publication = Rc::new(RefCell::new(NiriPublication::unavailable()));
        let observers = Rc::new(RefCell::new(WorkspaceActivationObservers::default()));
        let collector = NiriCollector::stopped(Rc::clone(&publication), Rc::clone(&observers));
        Self {
            inner: Rc::new(NiriControllerInner {
                publication,
                observers,
                collector,
            }),
        }
    }

    pub(super) fn snapshot(&self) -> serde_json::Value {
        self.inner.publication.borrow().snapshot.clone()
    }

    pub(super) fn shutdown(&self) {
        self.inner.collector.shutdown();
    }

    pub(super) fn connect_workspace_activation(
        &self,
        handler: impl Fn(WorkspaceActivation) + 'static,
    ) -> WorkspaceActivationHandlerId {
        self.inner.observers.borrow_mut().connect(handler)
    }

    pub(super) fn disconnect_workspace_activation(&self, id: WorkspaceActivationHandlerId) {
        self.inner.observers.borrow_mut().disconnect(id);
    }

    pub(super) fn focus_workspace(
        &self,
        workspace_index: u64,
    ) -> Result<serde_json::Value, NativeMethodError> {
        self.focus_workspace_with_action(workspace_index, focus_niri_workspace)
    }

    fn focus_workspace_with_action<A>(
        &self,
        workspace_index: u64,
        action: A,
    ) -> Result<serde_json::Value, NativeMethodError>
    where
        A: FnOnce(u64) -> Result<(), String>,
    {
        if workspace_index == 0 {
            return Err(NativeMethodError::new(
                "invalid_params",
                "params.workspaceIndex must be a positive integer",
            ));
        }

        {
            let publication = self.inner.publication.borrow();
            if publication.snapshot["available"] != true {
                return Err(NativeMethodError::new(
                    "niri_unavailable",
                    UNAVAILABLE_REASON,
                ));
            }
            if !publication.workspace_indices.contains(&workspace_index) {
                return Err(NativeMethodError::new(
                    "unknown_workspace",
                    format!("workspace index {workspace_index} is not in the latest niri snapshot"),
                ));
            }
        }

        action(workspace_index)
            .map_err(|reason| NativeMethodError::new("niri_action_failed", reason))?;
        Ok(serde_json::json!({ "workspaceIndex": workspace_index }))
    }

    #[cfg(test)]
    fn publish_for_test(&self, update: ReducerPublication) {
        self.inner.publication.borrow_mut().publish(update);
    }

    #[cfg(test)]
    fn make_unavailable_for_test(&self) {
        self.inner.publication.borrow_mut().publish_unavailable();
    }

    #[cfg(test)]
    pub(super) fn emit_activation_for_test(&self, activation: WorkspaceActivation) {
        let callbacks = self.inner.observers.borrow().callbacks();
        for callback in callbacks {
            callback(activation.clone());
        }
    }
}

impl NiriProvider {
    pub(super) fn new(niri: NiriController) -> Self {
        Self { niri }
    }
}

impl Provider for NiriProvider {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn snapshot(&self) -> serde_json::Value {
        self.niri.snapshot()
    }
}

fn focus_niri_workspace(workspace_index: u64) -> Result<(), String> {
    let workspace_index = workspace_index.to_string();
    let output = Command::new("niri")
        .args(["msg", "action", "focus-workspace", workspace_index.as_str()])
        .output()
        .map_err(|error| format!("failed to run niri focus-workspace: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return if stderr.is_empty() {
            Err(format!(
                "niri focus-workspace failed with status {}",
                output.status
            ))
        } else {
            Err(format!("niri focus-workspace failed: {stderr}"))
        };
    }

    Ok(())
}

fn unavailable_snapshot() -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "reason": UNAVAILABLE_REASON,
    })
}

#[derive(Debug, Clone, Deserialize)]
struct NiriWorkspace {
    id: u64,
    idx: u8,
    name: Option<String>,
    output: Option<String>,
    is_urgent: bool,
    is_active: bool,
    is_focused: bool,
    active_window_id: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct NiriWindow {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    is_focused: bool,
    is_urgent: bool,
    layout: serde_json::Value,
    focus_timestamp: Option<serde_json::Value>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceSnapshot {
    id: u64,
    index: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    is_active: bool,
    is_focused: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct FocusedWindowSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct FocusedWindowItemSnapshot {
    output: String,
    window: Option<FocusedWindowSnapshot>,
}

#[derive(Debug)]
pub(super) struct ReducerPublication {
    pub(super) snapshot: serde_json::Value,
    pub(super) workspace_indices: Vec<u64>,
    pub(super) focused_activation: Option<WorkspaceActivation>,
}

#[derive(Default)]
pub(super) struct NiriReducer {
    workspaces: BTreeMap<u64, NiriWorkspace>,
    windows: BTreeMap<u64, NiriWindow>,
    received_workspaces: bool,
    received_windows: bool,
}

impl NiriReducer {
    pub(super) fn apply_line(&mut self, line: &[u8]) -> Result<Option<ReducerPublication>, String> {
        let Some(event) = parse_event_line(line)? else {
            return Ok(None);
        };
        self.apply(event)?;
        if !self.received_workspaces || !self.received_windows {
            return Ok(None);
        }
        Ok(Some(self.publication()))
    }

    fn apply(&mut self, event: NiriEvent) -> Result<(), String> {
        match event {
            NiriEvent::WorkspacesChanged { workspaces } => {
                self.workspaces = workspaces
                    .into_iter()
                    .map(|workspace| (workspace.id, workspace))
                    .collect();
                self.received_workspaces = true;
            }
            NiriEvent::WorkspaceUrgencyChanged { id, urgent } => {
                let workspace = self.workspace_mut(id, "WorkspaceUrgencyChanged")?;
                workspace.is_urgent = urgent;
            }
            NiriEvent::WorkspaceActivated { id, focused } => {
                let output = self
                    .workspaces
                    .get(&id)
                    .ok_or_else(|| unknown_target("WorkspaceActivated", "workspace", id))?
                    .output
                    .clone();
                for workspace in self.workspaces.values_mut() {
                    let activated = workspace.id == id;
                    if workspace.output == output {
                        workspace.is_active = activated;
                    }
                    if focused {
                        workspace.is_focused = activated;
                    }
                }
            }
            NiriEvent::WorkspaceActiveWindowChanged {
                workspace_id,
                active_window_id,
            } => {
                let workspace = self.workspace_mut(workspace_id, "WorkspaceActiveWindowChanged")?;
                workspace.active_window_id = active_window_id;
            }
            NiriEvent::WindowsChanged { windows } => {
                self.windows = windows
                    .into_iter()
                    .map(|window| (window.id, window))
                    .collect();
                self.received_windows = true;
            }
            NiriEvent::WindowOpenedOrChanged { window } => {
                if window.is_focused {
                    for existing in self.windows.values_mut() {
                        existing.is_focused = false;
                    }
                }
                self.windows.insert(window.id, window);
            }
            NiriEvent::WindowClosed { id } => {
                self.windows
                    .remove(&id)
                    .ok_or_else(|| unknown_target("WindowClosed", "window", id))?;
            }
            NiriEvent::WindowFocusChanged { id } => {
                if let Some(id) = id
                    && !self.windows.contains_key(&id)
                {
                    return Err(unknown_target("WindowFocusChanged", "window", id));
                }
                for window in self.windows.values_mut() {
                    window.is_focused = Some(window.id) == id;
                }
            }
            NiriEvent::WindowFocusTimestampChanged {
                id,
                focus_timestamp,
            } => {
                let window = self.window_mut(id, "WindowFocusTimestampChanged")?;
                window.focus_timestamp = focus_timestamp;
            }
            NiriEvent::WindowUrgencyChanged { id, urgent } => {
                let window = self.window_mut(id, "WindowUrgencyChanged")?;
                window.is_urgent = urgent;
            }
            NiriEvent::WindowLayoutsChanged { changes } => {
                for (id, _) in &changes {
                    if !self.windows.contains_key(id) {
                        return Err(unknown_target("WindowLayoutsChanged", "window", *id));
                    }
                }
                for (id, layout) in changes {
                    self.windows
                        .get_mut(&id)
                        .expect("window targets were validated")
                        .layout = layout;
                }
            }
        }
        Ok(())
    }

    fn workspace_mut(
        &mut self,
        id: u64,
        event: &'static str,
    ) -> Result<&mut NiriWorkspace, String> {
        self.workspaces
            .get_mut(&id)
            .ok_or_else(|| unknown_target(event, "workspace", id))
    }

    fn window_mut(&mut self, id: u64, event: &'static str) -> Result<&mut NiriWindow, String> {
        self.windows
            .get_mut(&id)
            .ok_or_else(|| unknown_target(event, "window", id))
    }

    fn publication(&self) -> ReducerPublication {
        let mut workspaces = self
            .workspaces
            .values()
            .map(|workspace| WorkspaceSnapshot {
                id: workspace.id,
                index: workspace.idx,
                name: workspace.name.clone(),
                output: workspace.output.clone(),
                is_active: workspace.is_active,
                is_focused: workspace.is_focused,
            })
            .collect::<Vec<_>>();
        workspaces.sort_by(|a, b| {
            a.output
                .cmp(&b.output)
                .then(a.index.cmp(&b.index))
                .then(a.id.cmp(&b.id))
        });
        let workspace_indices = workspaces
            .iter()
            .map(|workspace| u64::from(workspace.index))
            .collect();

        let focused_activation = self
            .workspaces
            .values()
            .find(|workspace| workspace.is_focused)
            .and_then(|workspace| {
                Some(WorkspaceActivation {
                    output: workspace.output.clone()?,
                    workspace_index: u64::from(workspace.idx),
                })
            });

        let focused_output = self
            .workspaces
            .values()
            .find(|workspace| workspace.is_focused)
            .and_then(|workspace| workspace.output.as_ref())
            .map_or_else(
                || {
                    serde_json::json!({
                        "available": false,
                        "reason": "niri focused output unavailable",
                    })
                },
                |name| serde_json::json!({ "available": true, "name": name }),
            );
        let mut focused_windows = self
            .workspaces
            .values()
            .filter(|workspace| workspace.is_active)
            .filter_map(|workspace| {
                Some(FocusedWindowItemSnapshot {
                    output: workspace.output.clone()?,
                    window: workspace
                        .active_window_id
                        .and_then(|id| self.windows.get(&id))
                        .map(|window| FocusedWindowSnapshot {
                            title: window.title.clone(),
                            app_id: window.app_id.clone(),
                        }),
                })
            })
            .collect::<Vec<_>>();
        focused_windows.sort_by(|a, b| a.output.cmp(&b.output));

        ReducerPublication {
            snapshot: serde_json::json!({
                "available": true,
                "focusedOutput": focused_output,
                "workspaces": {
                    "available": true,
                    "items": workspaces,
                },
                "focusedWindows": {
                    "available": true,
                    "items": focused_windows,
                },
            }),
            workspace_indices,
            focused_activation,
        }
    }
}

fn unknown_target(event: &'static str, target: &'static str, id: u64) -> String {
    format!("niri {event} referenced unknown {target} {id}")
}

fn parse_event_line(line: &[u8]) -> Result<Option<NiriEvent>, String> {
    let value: serde_json::Value = serde_json::from_slice(line)
        .map_err(|error| format!("failed to parse niri event JSON: {error}"))?;
    let object = value
        .as_object()
        .ok_or_else(|| "niri event must be a JSON object".to_owned())?;
    if object.len() != 1 {
        return Err("niri event must contain exactly one variant".to_owned());
    }
    let (variant, payload) = object.iter().next().expect("object has one entry");
    let event = match variant.as_str() {
        "WorkspacesChanged" => {
            deserialize_event::<WorkspacesChanged>(variant, payload)?.map(|event| {
                NiriEvent::WorkspacesChanged {
                    workspaces: event.workspaces,
                }
            })
        }
        "WorkspaceUrgencyChanged" => {
            deserialize_event::<WorkspaceUrgencyChanged>(variant, payload)?.map(|event| {
                NiriEvent::WorkspaceUrgencyChanged {
                    id: event.id,
                    urgent: event.urgent,
                }
            })
        }
        "WorkspaceActivated" => {
            deserialize_event::<WorkspaceActivated>(variant, payload)?.map(|event| {
                NiriEvent::WorkspaceActivated {
                    id: event.id,
                    focused: event.focused,
                }
            })
        }
        "WorkspaceActiveWindowChanged" => {
            deserialize_event::<WorkspaceActiveWindowChanged>(variant, payload)?.map(|event| {
                NiriEvent::WorkspaceActiveWindowChanged {
                    workspace_id: event.workspace_id,
                    active_window_id: event.active_window_id,
                }
            })
        }
        "WindowsChanged" => deserialize_event::<WindowsChanged>(variant, payload)?.map(|event| {
            NiriEvent::WindowsChanged {
                windows: event.windows,
            }
        }),
        "WindowOpenedOrChanged" => deserialize_event::<WindowOpenedOrChanged>(variant, payload)?
            .map(|event| NiriEvent::WindowOpenedOrChanged {
                window: event.window,
            }),
        "WindowClosed" => deserialize_event::<WindowClosed>(variant, payload)?
            .map(|event| NiriEvent::WindowClosed { id: event.id }),
        "WindowFocusChanged" => deserialize_event::<WindowFocusChanged>(variant, payload)?
            .map(|event| NiriEvent::WindowFocusChanged { id: event.id }),
        "WindowFocusTimestampChanged" => {
            deserialize_event::<WindowFocusTimestampChanged>(variant, payload)?.map(|event| {
                NiriEvent::WindowFocusTimestampChanged {
                    id: event.id,
                    focus_timestamp: event.focus_timestamp,
                }
            })
        }
        "WindowUrgencyChanged" => {
            deserialize_event::<WindowUrgencyChanged>(variant, payload)?.map(|event| {
                NiriEvent::WindowUrgencyChanged {
                    id: event.id,
                    urgent: event.urgent,
                }
            })
        }
        "WindowLayoutsChanged" => {
            deserialize_event::<WindowLayoutsChanged>(variant, payload)?.map(|event| {
                NiriEvent::WindowLayoutsChanged {
                    changes: event.changes,
                }
            })
        }
        _ => None,
    };
    Ok(event)
}

fn deserialize_event<T>(variant: &str, payload: &serde_json::Value) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    serde_json::from_value(payload.clone())
        .map(Some)
        .map_err(|error| format!("malformed niri {variant} event: {error}"))
}

#[derive(Deserialize)]
struct WorkspacesChanged {
    workspaces: Vec<NiriWorkspace>,
}

#[derive(Deserialize)]
struct WorkspaceUrgencyChanged {
    id: u64,
    urgent: bool,
}

#[derive(Deserialize)]
struct WorkspaceActivated {
    id: u64,
    focused: bool,
}

#[derive(Deserialize)]
struct WorkspaceActiveWindowChanged {
    workspace_id: u64,
    active_window_id: Option<u64>,
}

#[derive(Deserialize)]
struct WindowsChanged {
    windows: Vec<NiriWindow>,
}

#[derive(Deserialize)]
struct WindowOpenedOrChanged {
    window: NiriWindow,
}

#[derive(Deserialize)]
struct WindowClosed {
    id: u64,
}

#[derive(Deserialize)]
struct WindowFocusChanged {
    id: Option<u64>,
}

#[derive(Deserialize)]
struct WindowFocusTimestampChanged {
    id: u64,
    focus_timestamp: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct WindowUrgencyChanged {
    id: u64,
    urgent: bool,
}

#[derive(Deserialize)]
struct WindowLayoutsChanged {
    changes: Vec<(u64, serde_json::Value)>,
}

enum NiriEvent {
    WorkspacesChanged {
        workspaces: Vec<NiriWorkspace>,
    },
    WorkspaceUrgencyChanged {
        id: u64,
        urgent: bool,
    },
    WorkspaceActivated {
        id: u64,
        focused: bool,
    },
    WorkspaceActiveWindowChanged {
        workspace_id: u64,
        active_window_id: Option<u64>,
    },
    WindowsChanged {
        windows: Vec<NiriWindow>,
    },
    WindowOpenedOrChanged {
        window: NiriWindow,
    },
    WindowClosed {
        id: u64,
    },
    WindowFocusChanged {
        id: Option<u64>,
    },
    WindowFocusTimestampChanged {
        id: u64,
        focus_timestamp: Option<serde_json::Value>,
    },
    WindowUrgencyChanged {
        id: u64,
        urgent: bool,
    },
    WindowLayoutsChanged {
        changes: Vec<(u64, serde_json::Value)>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(
        id: u64,
        idx: u8,
        output: Option<&str>,
        active: bool,
        focused: bool,
        active_window_id: Option<u64>,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "idx": idx,
            "name": if id == 41 { Some("code") } else { None },
            "output": output,
            "is_urgent": false,
            "is_active": active,
            "is_focused": focused,
            "active_window_id": active_window_id,
            "future_field": "ignored"
        })
    }

    fn window(id: u64, title: &str, focused: bool) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "title": title,
            "app_id": "Terminal",
            "pid": 123,
            "workspace_id": 41,
            "is_focused": focused,
            "is_floating": false,
            "is_urgent": false,
            "layout": {},
            "focus_timestamp": null,
            "future_field": true
        })
    }

    fn line(value: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&value).expect("test event must serialize")
    }

    fn coherent_reducer() -> NiriReducer {
        let mut reducer = NiriReducer::default();
        reducer
            .apply_line(&line(serde_json::json!({
                "WorkspacesChanged": {
                    "workspaces": [
                        workspace(42, 2, Some("eDP-1"), false, false, None),
                        workspace(30, 3, Some("HDMI-A-1"), true, false, None),
                        workspace(10, 1, Some("DP-1"), true, false, Some(8)),
                        workspace(41, 1, Some("eDP-1"), true, true, Some(7))
                    ]
                }
            })))
            .expect("workspace event must apply");
        reducer
            .apply_line(&line(serde_json::json!({
                "WindowsChanged": {
                    "windows": [
                        window(7, "cargo test", true),
                        window(8, "browser", false)
                    ]
                }
            })))
            .expect("window event must apply");
        reducer
    }

    #[test]
    fn initial_barrier_and_exact_schema_sort() {
        let mut reducer = NiriReducer::default();
        let first = reducer
            .apply_line(&line(serde_json::json!({
                "WorkspacesChanged": {
                    "workspaces": [
                        workspace(42, 2, Some("eDP-1"), false, false, None),
                        workspace(30, 3, Some("HDMI-A-1"), true, false, None),
                        workspace(10, 1, Some("DP-1"), true, false, Some(8)),
                        workspace(41, 1, Some("eDP-1"), true, true, Some(7))
                    ]
                }
            })))
            .expect("workspace event must apply");
        assert!(first.is_none());

        let published = reducer
            .apply_line(&line(serde_json::json!({
                "WindowsChanged": {
                    "windows": [
                        window(7, "cargo test", true),
                        window(8, "browser", false)
                    ]
                }
            })))
            .expect("window event must apply")
            .expect("second initial state part must publish");

        assert_eq!(
            published.snapshot,
            serde_json::json!({
                "available": true,
                "focusedOutput": { "available": true, "name": "eDP-1" },
                "workspaces": {
                    "available": true,
                    "items": [
                        {
                            "id": 10,
                            "index": 1,
                            "output": "DP-1",
                            "isActive": true,
                            "isFocused": false
                        },
                        {
                            "id": 30,
                            "index": 3,
                            "output": "HDMI-A-1",
                            "isActive": true,
                            "isFocused": false
                        },
                        {
                            "id": 41,
                            "index": 1,
                            "name": "code",
                            "output": "eDP-1",
                            "isActive": true,
                            "isFocused": true
                        },
                        {
                            "id": 42,
                            "index": 2,
                            "output": "eDP-1",
                            "isActive": false,
                            "isFocused": false
                        }
                    ]
                },
                "focusedWindows": {
                    "available": true,
                    "items": [
                        {
                            "output": "DP-1",
                            "window": { "title": "browser", "appId": "Terminal" }
                        },
                        {
                            "output": "HDMI-A-1",
                            "window": null
                        },
                        {
                            "output": "eDP-1",
                            "window": { "title": "cargo test", "appId": "Terminal" }
                        }
                    ]
                }
            })
        );
        assert_eq!(published.workspace_indices, vec![1, 3, 1, 2]);
        assert_eq!(
            published.focused_activation,
            Some(WorkspaceActivation {
                output: "eDP-1".to_owned(),
                workspace_index: 1,
            })
        );
    }

    #[test]
    fn full_replacements_remove_missing_state() {
        let mut reducer = coherent_reducer();
        let workspaces = reducer
            .apply_line(&line(serde_json::json!({
                "WorkspacesChanged": { "workspaces": [workspace(55, 3, Some("HDMI-A-1"), true, true, Some(7))] }
            })))
            .expect("replacement must apply")
            .expect("coherent reducer must publish");
        assert_eq!(workspaces.workspace_indices, vec![3]);
        assert_eq!(
            workspaces.snapshot["workspaces"]["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let unavailable_activation = reducer
            .apply_line(&line(serde_json::json!({
                "WorkspacesChanged": { "workspaces": [workspace(56, 4, None, true, true, Some(7))] }
            })))
            .expect("focused workspace without output must apply")
            .expect("coherent reducer must publish");
        assert_eq!(unavailable_activation.focused_activation, None);

        let windows = reducer
            .apply_line(&line(serde_json::json!({
                "WindowsChanged": { "windows": [] }
            })))
            .expect("replacement must apply")
            .expect("coherent reducer must publish");
        assert_eq!(
            windows.snapshot["focusedWindows"]["items"],
            serde_json::json!([])
        );
    }

    #[test]
    fn output_windows_follow_active_workspace_ids_not_global_keyboard_focus() {
        let mut reducer = coherent_reducer();

        let keyboard_focus_changed = reducer
            .apply_line(&line(serde_json::json!({
                "WindowFocusChanged": { "id": 8 }
            })))
            .expect("focus change must apply")
            .expect("coherent reducer must publish");
        assert_eq!(
            keyboard_focus_changed.snapshot["focusedWindows"]["items"],
            serde_json::json!([
                {
                    "output": "DP-1",
                    "window": { "title": "browser", "appId": "Terminal" }
                },
                { "output": "HDMI-A-1", "window": null },
                {
                    "output": "eDP-1",
                    "window": { "title": "cargo test", "appId": "Terminal" }
                }
            ])
        );

        let changed = reducer
            .apply_line(&line(serde_json::json!({
                "WindowOpenedOrChanged": { "window": window(8, "browser changed", true) }
            })))
            .expect("window change must apply")
            .expect("coherent reducer must publish");
        assert_eq!(
            changed.snapshot["focusedWindows"]["items"][0]["window"]["title"],
            "browser changed"
        );
        assert_eq!(
            changed.snapshot["focusedWindows"]["items"][2]["window"]["title"],
            "cargo test"
        );

        reducer
            .apply_line(&line(serde_json::json!({
                "WindowOpenedOrChanged": { "window": window(9, "editor", true) }
            })))
            .expect("window open must apply")
            .expect("coherent reducer must publish");
        let activated = reducer
            .apply_line(&line(serde_json::json!({
                "WorkspaceActiveWindowChanged": {
                    "workspace_id": 30,
                    "active_window_id": 9
                }
            })))
            .expect("active window change must apply")
            .expect("coherent reducer must publish");
        assert_eq!(
            activated.snapshot["focusedWindows"]["items"][1],
            serde_json::json!({
                "output": "HDMI-A-1",
                "window": { "title": "editor", "appId": "Terminal" }
            })
        );

        let closed = reducer
            .apply_line(&line(serde_json::json!({
                "WindowClosed": { "id": 7 }
            })))
            .expect("window close must apply")
            .expect("coherent reducer must publish");
        assert_eq!(
            closed.snapshot["focusedWindows"]["items"][2],
            serde_json::json!({ "output": "eDP-1", "window": null })
        );
        assert!(closed.snapshot.get("focusedWindow").is_none());
    }

    #[test]
    fn unknown_variants_are_ignored() {
        let mut reducer = coherent_reducer();
        assert!(
            reducer
                .apply_line(br#"{"FutureEvent":{"anything":true}}"#)
                .expect("unknown event must be tolerated")
                .is_none()
        );
    }

    #[test]
    fn malformed_missing_and_unknown_targets_fail() {
        let mut reducer = coherent_reducer();
        assert!(reducer.apply_line(b"not json").is_err());
        assert!(
            reducer
                .apply_line(br#"{"WindowClosed":{}}"#)
                .expect_err("missing field must fail")
                .contains("malformed niri WindowClosed event")
        );
        assert!(
            reducer
                .apply_line(br#"{"WindowClosed":{"id":999}}"#)
                .expect_err("unknown target must fail")
                .contains("unknown window 999")
        );
        assert!(
            reducer
                .apply_line(br#"{"WindowFocusChanged":{"id":999}}"#)
                .expect_err("unknown focus target must fail")
                .contains("unknown window 999")
        );
    }

    #[test]
    fn action_uses_published_index_and_unavailable_clears_it() {
        let niri = NiriController::stopped();
        let publication = coherent_reducer().publication();
        niri.publish_for_test(publication);
        let mut invoked_with = None;
        let result = niri
            .focus_workspace_with_action(2, |index| {
                invoked_with = Some(index);
                Ok(())
            })
            .expect("published index must be accepted");
        assert_eq!(invoked_with, Some(2));
        assert_eq!(result, serde_json::json!({ "workspaceIndex": 2 }));

        niri.make_unavailable_for_test();
        let error = niri
            .focus_workspace_with_action(2, |_| panic!("unavailable niri must reject action"))
            .expect_err("unavailable state must clear old indices");
        assert_eq!(error.code, "niri_unavailable");
        assert_eq!(niri.snapshot(), unavailable_snapshot());
    }

    #[test]
    fn action_rejects_workspace_id_and_preserves_error_codes() {
        let niri = NiriController::stopped();
        niri.publish_for_test(coherent_reducer().publication());
        let error = niri
            .focus_workspace_with_action(41, |_| panic!("workspace id must not be accepted"))
            .expect_err("id differs from index");
        assert_eq!(error.code, "unknown_workspace");

        let invalid = niri
            .focus_workspace_with_action(0, |_| panic!("zero must not invoke action"))
            .expect_err("zero is invalid");
        assert_eq!(invalid.code, "invalid_params");

        let failed = niri
            .focus_workspace_with_action(1, |_| Err("boom".to_owned()))
            .expect_err("action failure must be preserved");
        assert_eq!(failed.code, "niri_action_failed");
    }
}
