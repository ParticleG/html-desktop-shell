use serde::Deserialize;

use crate::surface::{SurfaceContext, SurfaceKind};

pub const BRIDGE_VERSION: u32 = 4;
const METHOD_GET_CAPABILITIES: &str = "getCapabilities";
const METHOD_GET_HOST_INFO: &str = "getHostInfo";
const METHOD_GET_STATE: &str = "getState";
const METHOD_NIRI_FOCUS_WORKSPACE: &str = "niriFocusWorkspace";
const METHOD_SET_QUICK_PANEL_OPEN: &str = "setQuickPanelOpen";
const METHOD_SET_DIAGNOSTICS_DIALOG_OPEN: &str = "setDiagnosticsDialogOpen";
const ALL_METHODS: &[&str] = &[
    METHOD_GET_HOST_INFO,
    METHOD_GET_CAPABILITIES,
    METHOD_GET_STATE,
    METHOD_NIRI_FOCUS_WORKSPACE,
    METHOD_SET_QUICK_PANEL_OPEN,
    METHOD_SET_DIAGNOSTICS_DIALOG_OPEN,
];
const BAR_METHODS: &[&str] = ALL_METHODS;
const QUICK_PANEL_METHODS: &[&str] = ALL_METHODS;
const WORKSPACE_HUD_METHODS: &[&str] = &[METHOD_GET_HOST_INFO, METHOD_GET_CAPABILITIES];
const DIAGNOSTICS_DIALOG_METHODS: &[&str] = &[
    METHOD_GET_HOST_INFO,
    METHOD_GET_CAPABILITIES,
    METHOD_GET_STATE,
    METHOD_SET_DIAGNOSTICS_DIALOG_OPEN,
];

pub fn capabilities() -> serde_json::Value {
    let surfaces = SurfaceKind::ALL
        .into_iter()
        .map(|kind| {
            (
                kind.wire_name().to_owned(),
                serde_json::json!({ "methods": methods_for(kind) }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    serde_json::json!({ "surfaces": surfaces })
}

pub fn capabilities_for(context: &SurfaceContext) -> serde_json::Value {
    serde_json::json!({ "methods": methods_for(context.kind()) })
}

fn methods_for(kind: SurfaceKind) -> &'static [&'static str] {
    match kind {
        SurfaceKind::Bar => BAR_METHODS,
        SurfaceKind::QuickPanel => QUICK_PANEL_METHODS,
        SurfaceKind::WorkspaceHud => WORKSPACE_HUD_METHODS,
        SurfaceKind::DiagnosticsDialog => DIAGNOSTICS_DIALOG_METHODS,
    }
}

#[derive(serde::Deserialize)]
pub struct NativeRequest {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(serde::Serialize)]
pub struct NativeResponse<'a> {
    pub id: &'a str,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<NativeError>,
}

#[derive(serde::Serialize)]
pub struct NativeError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug)]
pub struct NativeMethodError {
    pub code: &'static str,
    pub message: String,
}

impl NativeMethodError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceIndexParams {
    #[serde(rename = "workspaceIndex")]
    workspace_index: u64,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenParams {
    open: bool,
}

pub fn handle_native_request<S, A, Q, D>(
    raw: &str,
    surface_context: &SurfaceContext,
    state_snapshot: S,
    focus_workspace: A,
    set_quick_panel_open: Q,
    set_diagnostics_dialog_open: D,
) -> String
where
    S: FnOnce() -> serde_json::Value,
    A: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>,
    Q: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
    D: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
{
    // Serde structs also accept sequences; require the JSON object token before parsing.
    if !raw.trim_start().starts_with('{') {
        return bad_request_response(raw);
    }

    match serde_json::from_str::<NativeRequest>(raw) {
        Ok(request) => handle_request(
            &request,
            surface_context,
            state_snapshot,
            focus_workspace,
            set_quick_panel_open,
            set_diagnostics_dialog_open,
        ),
        Err(_) => bad_request_response(raw),
    }
}

fn handle_request<S, A, Q, D>(
    request: &NativeRequest,
    surface_context: &SurfaceContext,
    state_snapshot: S,
    focus_workspace: A,
    set_quick_panel_open: Q,
    set_diagnostics_dialog_open: D,
) -> String
where
    S: FnOnce() -> serde_json::Value,
    A: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>,
    Q: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
    D: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
{
    let method = request.method.as_str();
    let response = if !ALL_METHODS.contains(&method) {
        error_response(
            request.id.as_str(),
            "unknown_method",
            format!("unknown native method: {method}"),
        )
    } else if !methods_for(surface_context.kind()).contains(&method) {
        error_response(
            request.id.as_str(),
            "surface_forbidden",
            format!(
                "surface {} is not authorized for native method {method}",
                surface_context.kind().wire_name()
            ),
        )
    } else {
        match method {
            METHOD_GET_HOST_INFO => ok_response(
                request.id.as_str(),
                serde_json::json!({
                    "shell": "html-desktop-shell",
                    "backend": "wayland-layer-shell",
                    "bridgeVersion": BRIDGE_VERSION,
                    "surface": surface_context,
                }),
            ),
            METHOD_GET_CAPABILITIES => {
                ok_response(request.id.as_str(), capabilities_for(surface_context))
            }
            METHOD_GET_STATE => ok_response(request.id.as_str(), state_snapshot()),
            METHOD_NIRI_FOCUS_WORKSPACE => {
                match workspace_index_param(&request.params).and_then(focus_workspace) {
                    Ok(result) => ok_response(request.id.as_str(), result),
                    Err(error) => error_response(request.id.as_str(), error.code, error.message),
                }
            }
            METHOD_SET_QUICK_PANEL_OPEN => {
                match open_param(&request.params).and_then(set_quick_panel_open) {
                    Ok(result) => ok_response(request.id.as_str(), result),
                    Err(error) => error_response(request.id.as_str(), error.code, error.message),
                }
            }
            METHOD_SET_DIAGNOSTICS_DIALOG_OPEN => {
                match diagnostics_open_param(&request.params, surface_context.kind())
                    .and_then(set_diagnostics_dialog_open)
                {
                    Ok(result) => ok_response(request.id.as_str(), result),
                    Err(error) => error_response(request.id.as_str(), error.code, error.message),
                }
            }
            _ => unreachable!("known methods are handled exhaustively"),
        }
    };

    serialize_response(&response)
}

fn workspace_index_param(params: &serde_json::Value) -> Result<u64, NativeMethodError> {
    let invalid_params = || {
        NativeMethodError::new(
            "invalid_params",
            "params must contain only workspaceIndex as a positive integer",
        )
    };
    if !params.is_object() {
        return Err(invalid_params());
    }
    let parsed = WorkspaceIndexParams::deserialize(params).map_err(|_| invalid_params())?;

    if parsed.workspace_index == 0 {
        return Err(NativeMethodError::new(
            "invalid_params",
            "params.workspaceIndex must be a positive integer",
        ));
    }

    Ok(parsed.workspace_index)
}

fn open_param(params: &serde_json::Value) -> Result<bool, NativeMethodError> {
    let invalid_params = || {
        NativeMethodError::new(
            "invalid_params",
            "params must contain only open as a boolean",
        )
    };
    if !params.is_object() {
        return Err(invalid_params());
    }
    OpenParams::deserialize(params)
        .map(|params| params.open)
        .map_err(|_| invalid_params())
}

fn diagnostics_open_param(
    params: &serde_json::Value,
    caller_kind: SurfaceKind,
) -> Result<bool, NativeMethodError> {
    let open = open_param(params)?;
    if caller_kind == SurfaceKind::DiagnosticsDialog && open {
        return Err(NativeMethodError::new(
            "surface_forbidden",
            "diagnosticsDialog may close itself but may not reopen itself",
        ));
    }
    Ok(open)
}

fn ok_response(id: &str, result: serde_json::Value) -> NativeResponse<'_> {
    NativeResponse {
        id,
        ok: true,
        result: Some(result),
        error: None,
    }
}

fn error_response<'a>(id: &'a str, code: &'static str, message: String) -> NativeResponse<'a> {
    NativeResponse {
        id,
        ok: false,
        result: None,
        error: Some(NativeError { code, message }),
    }
}

fn bad_request_response(raw: &str) -> String {
    let id = request_id(raw).unwrap_or_default();
    let response = error_response(
        id.as_str(),
        "bad_request",
        "request must be a JSON object with string id and method".to_owned(),
    );
    serialize_response(&response)
}

fn request_id(raw: &str) -> Option<String> {
    let value = serde_json::from_str::<serde_json::Value>(raw).ok()?;
    value.get("id")?.as_str().map(str::to_owned)
}

fn serialize_response(response: &NativeResponse<'_>) -> String {
    serde_json::to_string(response).unwrap_or_else(|_| {
        r#"{"id":"","ok":false,"error":{"code":"internal_error","message":"failed to serialize native response"}}"#
            .to_owned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response_value(raw: &str) -> serde_json::Value {
        serde_json::from_str(raw).expect("native response must be valid JSON")
    }

    fn test_state() -> serde_json::Value {
        serde_json::json!({
            "clock": { "time": "12:34:56" },
            "host": {
                "backend": "wayland-layer-shell",
                "monitorCount": 2,
                "bridgeVersion": BRIDGE_VERSION,
            },
            "niri": {
                "available": false,
                "reason": "niri IPC unavailable",
            },
        })
    }

    fn context(kind: SurfaceKind) -> SurfaceContext {
        match kind {
            SurfaceKind::Bar => SurfaceContext::Bar {
                monitor_index: 7,
                output: Some("eDP-1".to_owned()),
            },
            SurfaceKind::QuickPanel => SurfaceContext::QuickPanel {
                monitor_index: 7,
                output: Some("eDP-1".to_owned()),
            },
            SurfaceKind::WorkspaceHud => SurfaceContext::WorkspaceHud {
                monitor_index: 7,
                output: Some("eDP-1".to_owned()),
                workspace_index: 3,
            },
            SurfaceKind::DiagnosticsDialog => SurfaceContext::DiagnosticsDialog {
                monitor_index: 7,
                output: Some("eDP-1".to_owned()),
            },
        }
    }

    fn handle_for(raw: &str, kind: SurfaceKind) -> serde_json::Value {
        handle_with_actions(
            raw,
            kind,
            |workspace_index| Ok(serde_json::json!({ "workspaceIndex": workspace_index })),
            |open| Ok(serde_json::json!({ "open": open })),
            |open| Ok(serde_json::json!({ "open": open })),
        )
    }

    fn handle_with_actions<A, Q, D>(
        raw: &str,
        kind: SurfaceKind,
        focus_workspace: A,
        set_quick_panel_open: Q,
        set_diagnostics_dialog_open: D,
    ) -> serde_json::Value
    where
        A: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>,
        Q: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
        D: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
    {
        response_value(&handle_native_request(
            raw,
            &context(kind),
            test_state,
            focus_workspace,
            set_quick_panel_open,
            set_diagnostics_dialog_open,
        ))
    }

    #[test]
    fn get_host_info_returns_exact_v4_surface_contexts() {
        for kind in SurfaceKind::ALL {
            let response = handle_for(r#"{"id":"1","method":"getHostInfo"}"#, kind);

            assert_eq!(response["ok"], true);
            assert_eq!(response["result"]["shell"], "html-desktop-shell");
            assert_eq!(response["result"]["backend"], "wayland-layer-shell");
            assert_eq!(response["result"]["bridgeVersion"], 4);
            assert_eq!(
                response["result"]["surface"]["kind"],
                context(kind).kind().wire_name()
            );
            assert_eq!(response["result"]["surface"]["monitorIndex"], 7);
            assert_eq!(response["result"]["surface"]["output"], "eDP-1");
        }
        assert_eq!(BRIDGE_VERSION, 4);
    }

    #[test]
    fn get_capabilities_returns_exact_surface_scoped_matrix() {
        let cases = [
            (SurfaceKind::Bar, BAR_METHODS),
            (SurfaceKind::QuickPanel, QUICK_PANEL_METHODS),
            (SurfaceKind::WorkspaceHud, WORKSPACE_HUD_METHODS),
            (SurfaceKind::DiagnosticsDialog, DIAGNOSTICS_DIALOG_METHODS),
        ];

        for (kind, expected) in cases {
            let response = handle_for(r#"{"id":"2","method":"getCapabilities"}"#, kind);
            assert_eq!(response["result"]["methods"], serde_json::json!(expected));
        }
    }

    #[test]
    fn capabilities_do_not_include_generic_native_methods() {
        let forbidden = [
            "runCommand",
            "readFile",
            "writeFile",
            "dbusCall",
            "httpRequest",
            "eval",
            "niriAction",
            "createWindow",
            "showHud",
        ];

        for method in forbidden {
            assert!(!ALL_METHODS.contains(&method));
        }
    }

    #[test]
    fn every_denied_capability_matrix_cell_returns_surface_forbidden() {
        for kind in SurfaceKind::ALL {
            for method in ALL_METHODS {
                if methods_for(kind).contains(method) {
                    continue;
                }
                let request = serde_json::json!({
                    "id": "denied",
                    "method": method,
                    "params": {
                        "workspaceIndex": 1,
                        "open": true,
                    },
                });
                let response = handle_for(request.to_string().as_str(), kind);
                assert_eq!(response["ok"], false);
                assert_eq!(response["error"]["code"], "surface_forbidden");
            }
        }
    }

    #[test]
    fn get_state_returns_provider_snapshot_for_authorized_surface() {
        let response = handle_for(
            r#"{"id":"state","method":"getState"}"#,
            SurfaceKind::DiagnosticsDialog,
        );

        assert_eq!(response["id"], "state");
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["clock"]["time"], "12:34:56");
        assert_eq!(response["result"]["host"]["monitorCount"], 2);
        assert_eq!(response["result"]["niri"]["available"], false);
    }

    #[test]
    fn niri_focus_workspace_uses_exact_workspace_index_wire_contract() {
        let response = handle_for(
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceIndex":2}}"#,
            SurfaceKind::QuickPanel,
        );

        assert_eq!(
            response,
            serde_json::json!({
                "id": "focus",
                "ok": true,
                "result": { "workspaceIndex": 2 },
            })
        );
    }

    #[test]
    fn privileged_params_reject_missing_wrong_type_and_unknown_fields() {
        let invalid_requests = [
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{}}"#,
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceIndex":2,"output":"DP-1"}}"#,
            r#"{"id":"quick","method":"setQuickPanelOpen","params":{}}"#,
            r#"{"id":"quick","method":"setQuickPanelOpen","params":{"open":"yes"}}"#,
            r#"{"id":"quick","method":"setQuickPanelOpen","params":{"open":true,"monitorIndex":7}}"#,
            r#"{"id":"diagnostics","method":"setDiagnosticsDialogOpen","params":null}"#,
        ];

        for request in invalid_requests {
            let response = handle_for(request, SurfaceKind::Bar);
            assert_eq!(response["ok"], false);
            assert_eq!(response["error"]["code"], "invalid_params");
        }
    }

    #[test]
    fn array_envelopes_are_rejected_before_dispatch() {
        for request in [
            r#"["array","getState",{}]"#,
            r#"["array","niriFocusWorkspace",{"workspaceIndex":2}]"#,
            r#"["array","setQuickPanelOpen",{"open":true}]"#,
            r#"["array","setDiagnosticsDialogOpen",{"open":false}]"#,
        ] {
            let response = response_value(&handle_native_request(
                request,
                &context(SurfaceKind::Bar),
                || panic!("array envelope must not read state"),
                |_| panic!("array envelope must not focus a workspace"),
                |_| panic!("array envelope must not change Quick Panel"),
                |_| panic!("array envelope must not change Diagnostics"),
            ));
            assert_eq!(response["id"], "");
            assert_eq!(response["ok"], false);
            assert_eq!(response["error"]["code"], "bad_request");
        }
    }

    #[test]
    fn array_action_params_are_rejected_without_running_actions() {
        for (kind, request) in [
            (
                SurfaceKind::Bar,
                r#"{"id":"focus","method":"niriFocusWorkspace","params":[2]}"#,
            ),
            (
                SurfaceKind::Bar,
                r#"{"id":"quick","method":"setQuickPanelOpen","params":[true]}"#,
            ),
            (
                SurfaceKind::DiagnosticsDialog,
                r#"{"id":"diagnostics","method":"setDiagnosticsDialogOpen","params":[false]}"#,
            ),
        ] {
            let response = handle_with_actions(
                request,
                kind,
                |_| panic!("array params must not focus a workspace"),
                |_| panic!("array params must not change Quick Panel"),
                |_| panic!("array params must not change Diagnostics"),
            );
            assert_eq!(response["ok"], false);
            assert_eq!(response["error"]["code"], "invalid_params");
        }
    }

    #[test]
    fn niri_focus_workspace_rejects_zero_workspace_index() {
        let response = handle_with_actions(
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceIndex":0}}"#,
            SurfaceKind::Bar,
            |_| panic!("invalid params must not call the action"),
            |_| panic!("unrelated action must not run"),
            |_| panic!("unrelated action must not run"),
        );

        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "invalid_params");
    }

    #[test]
    fn native_action_errors_are_preserved() {
        let response = handle_with_actions(
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceIndex":9}}"#,
            SurfaceKind::Bar,
            |workspace_index| {
                Err(NativeMethodError::new(
                    "unknown_workspace",
                    format!("workspace index {workspace_index} is not in the latest niri snapshot"),
                ))
            },
            |_| panic!("unrelated action must not run"),
            |_| panic!("unrelated action must not run"),
        );

        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "unknown_workspace");
    }

    #[test]
    fn diagnostics_dialog_may_close_but_not_reopen_itself() {
        let response = handle_with_actions(
            r#"{"id":"diagnostics","method":"setDiagnosticsDialogOpen","params":{"open":true}}"#,
            SurfaceKind::DiagnosticsDialog,
            |_| panic!("unrelated action must not run"),
            |_| panic!("unrelated action must not run"),
            |_| panic!("forbidden reopen must not call the action"),
        );

        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "surface_forbidden");
    }

    #[test]
    fn unknown_method_returns_error() {
        let response = handle_for(r#"{"id":"3","method":"launch"}"#, SurfaceKind::Bar);

        assert_eq!(response["id"], "3");
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "unknown_method");
        assert_eq!(
            response["error"]["message"],
            "unknown native method: launch"
        );
        assert!(response.get("result").is_none());
    }

    #[test]
    fn malformed_request_returns_bad_request() {
        let response = handle_for(r#"{"id":"4","method":7}"#, SurfaceKind::Bar);

        assert_eq!(response["id"], "4");
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "bad_request");
    }

    #[test]
    fn malformed_json_without_id_uses_empty_response_id() {
        let response = handle_for("not json", SurfaceKind::Bar);

        assert_eq!(response["id"], "");
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "bad_request");
    }
}
