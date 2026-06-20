use std::{process::Command, rc::Rc};

use gtk4::gio::prelude::ListModelExt;

use crate::messages::BRIDGE_VERSION;

pub trait Provider {
    fn name(&self) -> &'static str;
    fn snapshot(&self) -> serde_json::Value;
}

#[derive(Clone)]
pub struct ProviderRegistry {
    providers: Rc<Vec<Box<dyn Provider>>>,
}

impl ProviderRegistry {
    pub fn new(monitors: &gtk4::gio::ListModel) -> Self {
        Self::from_providers(vec![
            Box::new(ClockProvider),
            Box::new(HostProvider {
                monitors: monitors.clone(),
            }),
            Box::new(NiriProvider::new()),
        ])
    }

    pub fn snapshot(&self) -> serde_json::Value {
        let mut state = serde_json::Map::with_capacity(self.providers.len());
        for provider in self.providers.iter() {
            state.insert(provider.name().to_owned(), provider.snapshot());
        }
        serde_json::Value::Object(state)
    }

    fn from_providers(providers: Vec<Box<dyn Provider>>) -> Self {
        Self {
            providers: Rc::new(providers),
        }
    }
}

struct ClockProvider;

impl Provider for ClockProvider {
    fn name(&self) -> &'static str {
        "clock"
    }

    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({ "time": current_time() })
    }
}

struct HostProvider {
    monitors: gtk4::gio::ListModel,
}

impl Provider for HostProvider {
    fn name(&self) -> &'static str {
        "host"
    }

    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "backend": "wayland-layer-shell",
            "monitorCount": self.monitors.n_items(),
            "bridgeVersion": BRIDGE_VERSION,
        })
    }
}

struct NiriProvider {
    detected: bool,
}

#[derive(serde::Deserialize)]
struct NiriFocusedOutput {
    name: String,
}

#[derive(serde::Deserialize)]
struct NiriWorkspace {
    id: u64,
    idx: u8,
    name: Option<String>,
    output: Option<String>,
    is_active: bool,
    is_focused: bool,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
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

#[derive(serde::Deserialize)]
struct NiriFocusedWindow {
    title: Option<String>,
    app_id: Option<String>,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FocusedWindowSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_id: Option<String>,
}

impl From<NiriWorkspace> for WorkspaceSnapshot {
    fn from(workspace: NiriWorkspace) -> Self {
        Self {
            id: workspace.id,
            index: workspace.idx,
            name: workspace.name,
            output: workspace.output,
            is_active: workspace.is_active,
            is_focused: workspace.is_focused,
        }
    }
}

impl From<NiriFocusedWindow> for FocusedWindowSnapshot {
    fn from(window: NiriFocusedWindow) -> Self {
        Self {
            title: window.title,
            app_id: window.app_id,
        }
    }
}

impl NiriProvider {
    fn new() -> Self {
        Self {
            detected: std::env::var_os("NIRI_SOCKET").is_some(),
        }
    }
}

impl Provider for NiriProvider {
    fn name(&self) -> &'static str {
        "niri"
    }

    fn snapshot(&self) -> serde_json::Value {
        if !self.detected {
            return serde_json::json!({
                "available": false,
                "reason": "niri IPC unavailable",
            });
        }

        serde_json::json!({
            "available": true,
            "focusedOutput": niri_part("focused-output", focused_output_snapshot_from_json),
            "workspaces": niri_part("workspaces", workspaces_snapshot_from_json),
            "focusedWindow": niri_part("focused-window", focused_window_snapshot_from_json),
        })
    }
}

fn current_time() -> String {
    glib::DateTime::now_local()
        .and_then(|date_time| date_time.format("%H:%M:%S"))
        .map(|time| time.to_string())
        .unwrap_or_else(|_| "--:--:--".to_owned())
}

fn niri_part(request: &'static str, parser: fn(&[u8]) -> serde_json::Value) -> serde_json::Value {
    match niri_msg_json(request) {
        Ok(stdout) => parser(&stdout),
        Err(reason) => unavailable_part(reason),
    }
}

fn niri_msg_json(request: &'static str) -> Result<Vec<u8>, String> {
    let output = Command::new("niri")
        .args(["msg", "-j", request])
        .output()
        .map_err(|error| format!("failed to run niri msg {request}: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr = stderr.trim();
        return if stderr.is_empty() {
            Err(format!(
                "niri msg {request} failed with status {}",
                output.status
            ))
        } else {
            Err(format!("niri msg {request} failed: {stderr}"))
        };
    }

    Ok(output.stdout)
}

fn focused_output_snapshot_from_json(stdout: &[u8]) -> serde_json::Value {
    match parse_focused_output(stdout) {
        Ok(focused_output) => serde_json::json!({
            "available": true,
            "name": focused_output.name,
        }),
        Err(reason) => unavailable_part(reason),
    }
}

fn parse_focused_output(stdout: &[u8]) -> Result<NiriFocusedOutput, String> {
    serde_json::from_slice::<NiriFocusedOutput>(stdout)
        .map_err(|error| format!("failed to parse niri focused-output JSON: {error}"))
}

fn workspaces_snapshot_from_json(stdout: &[u8]) -> serde_json::Value {
    match parse_workspaces(stdout) {
        Ok(workspaces) => serde_json::json!({
            "available": true,
            "items": workspaces,
        }),
        Err(reason) => unavailable_part(reason),
    }
}

fn parse_workspaces(stdout: &[u8]) -> Result<Vec<WorkspaceSnapshot>, String> {
    let workspaces = serde_json::from_slice::<Vec<NiriWorkspace>>(stdout)
        .map_err(|error| format!("failed to parse niri workspaces JSON: {error}"))?;
    let mut workspaces = workspaces
        .into_iter()
        .map(WorkspaceSnapshot::from)
        .collect::<Vec<_>>();
    workspaces.sort_by(|a, b| {
        a.output
            .cmp(&b.output)
            .then(a.index.cmp(&b.index))
            .then(a.id.cmp(&b.id))
    });
    Ok(workspaces)
}

fn focused_window_snapshot_from_json(stdout: &[u8]) -> serde_json::Value {
    match parse_focused_window(stdout) {
        Ok(window) => serde_json::json!({
            "available": true,
            "window": window,
        }),
        Err(reason) => unavailable_part(reason),
    }
}

fn parse_focused_window(stdout: &[u8]) -> Result<Option<FocusedWindowSnapshot>, String> {
    let window = serde_json::from_slice::<Option<NiriFocusedWindow>>(stdout)
        .map_err(|error| format!("failed to parse niri focused-window JSON: {error}"))?;
    Ok(window.map(FocusedWindowSnapshot::from))
}

fn unavailable_part(reason: String) -> serde_json::Value {
    serde_json::json!({
        "available": false,
        "reason": reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticProvider {
        name: &'static str,
        value: serde_json::Value,
    }

    impl Provider for StaticProvider {
        fn name(&self) -> &'static str {
            self.name
        }

        fn snapshot(&self) -> serde_json::Value {
            self.value.clone()
        }
    }

    #[test]
    fn registry_returns_provider_snapshots_by_name() {
        let registry = ProviderRegistry::from_providers(vec![Box::new(StaticProvider {
            name: "demo",
            value: serde_json::json!({ "ok": true }),
        })]);

        assert_eq!(registry.snapshot()["demo"]["ok"], true);
    }

    #[test]
    fn niri_provider_reports_unavailable_when_not_detected() {
        let provider = NiriProvider { detected: false };
        let snapshot = provider.snapshot();

        assert_eq!(snapshot["available"], false);
        assert_eq!(snapshot["reason"], "niri IPC unavailable");
    }

    #[test]
    fn parses_valid_workspaces_json() {
        let snapshot = workspaces_snapshot_from_json(
            br#"[
                {
                    "id": 42,
                    "idx": 2,
                    "name": null,
                    "output": "eDP-1",
                    "is_active": false,
                    "is_focused": false
                },
                {
                    "id": 41,
                    "idx": 1,
                    "name": "code",
                    "output": "eDP-1",
                    "is_active": true,
                    "is_focused": true
                }
            ]"#,
        );

        assert_eq!(snapshot["available"], true);
        assert_eq!(snapshot["items"][0]["id"], 41);
        assert_eq!(snapshot["items"][0]["index"], 1);
        assert_eq!(snapshot["items"][0]["name"], "code");
        assert_eq!(snapshot["items"][0]["output"], "eDP-1");
        assert_eq!(snapshot["items"][0]["isActive"], true);
        assert_eq!(snapshot["items"][0]["isFocused"], true);
        assert!(snapshot["items"][1].get("name").is_none());
    }

    #[test]
    fn parses_valid_focused_window_json() {
        let snapshot = focused_window_snapshot_from_json(
            br#"{
                "id": 7,
                "title": "cargo test",
                "app_id": "Terminal",
                "workspace_id": 41,
                "is_focused": true
            }"#,
        );

        assert_eq!(snapshot["available"], true);
        assert_eq!(snapshot["window"]["title"], "cargo test");
        assert_eq!(snapshot["window"]["appId"], "Terminal");
    }

    #[test]
    fn parses_empty_focused_window_json() {
        let snapshot = focused_window_snapshot_from_json(b"null");

        assert_eq!(snapshot["available"], true);
        assert!(snapshot["window"].is_null());
    }

    #[test]
    fn malformed_workspaces_json_returns_unavailable_part() {
        let snapshot = workspaces_snapshot_from_json(
            br#"[{
                "idx": 1,
                "output": "eDP-1",
                "is_active": true,
                "is_focused": true
            }]"#,
        );

        assert_eq!(snapshot["available"], false);
        assert!(
            snapshot["reason"]
                .as_str()
                .expect("reason should be a string")
                .contains("failed to parse niri workspaces JSON")
        );
    }

    #[test]
    fn clock_provider_returns_hh_mm_ss_time() {
        let time = current_time();
        let bytes = time.as_bytes();

        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes[2], b':');
        assert_eq!(bytes[5], b':');
    }
}
