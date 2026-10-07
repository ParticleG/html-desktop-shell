use serde::Deserialize;

use crate::{
    providers::desktop::{DesktopRequest, TrayOrientation},
    surface::{SurfaceContext, SurfaceKind},
};

pub const BRIDGE_VERSION: u32 = 5;
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
    "setAudioVolume",
    "setAudioMuted",
    "getTrayMenu",
    "activateTrayItem",
    "secondaryActivateTrayItem",
    "showTrayItemContextMenu",
    "scrollTrayItem",
    "activateTrayMenuItem",
    "getNotifications",
    "dismissNotification",
    "invokeNotificationAction",
    "markAllNotificationsRead",
    "searchApplications",
    "launchApplication",
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

pub(crate) enum NativeDispatch {
    Response(String),
    Desktop { id: String, request: DesktopRequest },
}

#[cfg(test)]
impl NativeDispatch {
    fn into_response(self) -> String {
        match self {
            Self::Response(response) => response,
            Self::Desktop { .. } => panic!("desktop operation must not dispatch in this test"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyParams {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VolumeParams {
    volume: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MutedParams {
    muted: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TrayItemParams {
    item_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TrayMenuReadParams {
    item_id: String,
    menu_item_id: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TrayScrollParams {
    item_id: String,
    delta: i32,
    orientation: TrayOrientation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct TrayMenuParams {
    item_id: String,
    menu_item_id: i32,
    revision: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NotificationParams {
    notification_id: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct NotificationActionParams {
    notification_id: u32,
    action_key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchParams {
    query: String,
    limit: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ApplicationParams {
    application_id: String,
}

pub(crate) fn handle_native_request<S, A, Q, D>(
    raw: &str,
    surface_context: &SurfaceContext,
    state_snapshot: S,
    focus_workspace: A,
    set_quick_panel_open: Q,
    set_diagnostics_dialog_open: D,
) -> NativeDispatch
where
    S: FnOnce() -> serde_json::Value,
    A: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>,
    Q: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
    D: FnOnce(bool) -> Result<serde_json::Value, NativeMethodError>,
{
    // Serde structs also accept sequences; require the JSON object token before parsing.
    if !raw.trim_start().starts_with('{') {
        return NativeDispatch::Response(bad_request_response(raw));
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
        Err(_) => NativeDispatch::Response(bad_request_response(raw)),
    }
}

fn handle_request<S, A, Q, D>(
    request: &NativeRequest,
    surface_context: &SurfaceContext,
    state_snapshot: S,
    focus_workspace: A,
    set_quick_panel_open: Q,
    set_diagnostics_dialog_open: D,
) -> NativeDispatch
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
    } else if ![
        METHOD_GET_HOST_INFO,
        METHOD_GET_CAPABILITIES,
        METHOD_GET_STATE,
        METHOD_NIRI_FOCUS_WORKSPACE,
        METHOD_SET_QUICK_PANEL_OPEN,
        METHOD_SET_DIAGNOSTICS_DIALOG_OPEN,
    ]
    .contains(&method)
    {
        match desktop_request(method, &request.params) {
            Ok(operation) => {
                return NativeDispatch::Desktop {
                    id: request.id.clone(),
                    request: operation,
                };
            }
            Err(error) => error_response(request.id.as_str(), error.code, error.message),
        }
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

    NativeDispatch::Response(serialize_response(&response))
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

fn desktop_request(
    method: &str,
    params: &serde_json::Value,
) -> Result<DesktopRequest, NativeMethodError> {
    let invalid =
        || NativeMethodError::new("invalid_params", format!("invalid parameters for {method}"));
    let valid_id = |id: &str| !id.is_empty() && id.len() <= 256 && !id.contains('\0');
    let operation = match method {
        "setAudioVolume" => {
            let params: VolumeParams = object_params(params, method)?;
            if params.volume > 100 {
                return Err(invalid());
            }
            DesktopRequest::SetAudioVolume {
                volume: params.volume,
            }
        }
        "setAudioMuted" => {
            let params: MutedParams = object_params(params, method)?;
            DesktopRequest::SetAudioMuted {
                muted: params.muted,
            }
        }
        "getTrayMenu" => {
            let params: TrayMenuReadParams = object_params(params, method)?;
            if !valid_id(&params.item_id) {
                return Err(invalid());
            }
            DesktopRequest::GetTrayMenu {
                item_id: params.item_id,
                menu_item_id: params.menu_item_id,
            }
        }
        "activateTrayItem" | "secondaryActivateTrayItem" | "showTrayItemContextMenu" => {
            let params: TrayItemParams = object_params(params, method)?;
            if !valid_id(&params.item_id) {
                return Err(invalid());
            }
            match method {
                "activateTrayItem" => DesktopRequest::ActivateTrayItem {
                    item_id: params.item_id,
                },
                "secondaryActivateTrayItem" => DesktopRequest::SecondaryActivateTrayItem {
                    item_id: params.item_id,
                },
                _ => DesktopRequest::ShowTrayItemContextMenu {
                    item_id: params.item_id,
                },
            }
        }
        "scrollTrayItem" => {
            let params: TrayScrollParams = object_params(params, method)?;
            if !valid_id(&params.item_id) {
                return Err(invalid());
            }
            DesktopRequest::ScrollTrayItem {
                item_id: params.item_id,
                delta: params.delta,
                orientation: params.orientation,
            }
        }
        "activateTrayMenuItem" => {
            let params: TrayMenuParams = object_params(params, method)?;
            if !valid_id(&params.item_id) {
                return Err(invalid());
            }
            DesktopRequest::ActivateTrayMenuItem {
                item_id: params.item_id,
                menu_item_id: params.menu_item_id,
                revision: params.revision,
            }
        }
        "getNotifications" | "markAllNotificationsRead" => {
            let _: EmptyParams = object_params(params, method)?;
            if method == "getNotifications" {
                DesktopRequest::GetNotifications
            } else {
                DesktopRequest::MarkAllNotificationsRead
            }
        }
        "dismissNotification" => {
            let params: NotificationParams = object_params(params, method)?;
            if params.notification_id == 0 {
                return Err(invalid());
            }
            DesktopRequest::DismissNotification {
                notification_id: params.notification_id,
            }
        }
        "invokeNotificationAction" => {
            let params: NotificationActionParams = object_params(params, method)?;
            if params.notification_id == 0
                || params.action_key.is_empty()
                || params.action_key.chars().count() > 256
                || params.action_key.contains('\0')
            {
                return Err(invalid());
            }
            DesktopRequest::InvokeNotificationAction {
                notification_id: params.notification_id,
                action_key: params.action_key,
            }
        }
        "searchApplications" => {
            let params: SearchParams = object_params(params, method)?;
            if params.query.chars().count() > 256
                || params.query.contains('\0')
                || !(1..=50).contains(&params.limit)
            {
                return Err(invalid());
            }
            DesktopRequest::SearchApplications {
                query: params.query,
                limit: params.limit,
            }
        }
        "launchApplication" => {
            let params: ApplicationParams = object_params(params, method)?;
            if !valid_id(&params.application_id) {
                return Err(invalid());
            }
            DesktopRequest::LaunchApplication {
                application_id: params.application_id,
            }
        }
        _ => {
            return Err(NativeMethodError::new(
                "unknown_method",
                format!("unknown native method: {method}"),
            ));
        }
    };
    Ok(operation)
}

fn object_params<'a, T: Deserialize<'a>>(
    params: &'a serde_json::Value,
    method: &str,
) -> Result<T, NativeMethodError> {
    let invalid =
        || NativeMethodError::new("invalid_params", format!("invalid parameters for {method}"));
    if !params.is_object() {
        return Err(invalid());
    }
    T::deserialize(params).map_err(|_| invalid())
}

pub(crate) fn desktop_response(
    id: &str,
    result: Result<serde_json::Value, NativeMethodError>,
) -> String {
    let response = match result {
        Ok(result) => ok_response(id, result),
        Err(error) => error_response(id, error.code, error.message),
    };
    serialize_response(&response)
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
        response_value(
            &handle_native_request(
                raw,
                &context(kind),
                || panic!("invalid request must not read provider state"),
                focus_workspace,
                set_quick_panel_open,
                set_diagnostics_dialog_open,
            )
            .into_response(),
        )
    }

    #[test]
    fn get_host_info_returns_exact_v5_surface_contexts() {
        for kind in SurfaceKind::ALL {
            let response = handle_for(r#"{"id":"1","method":"getHostInfo"}"#, kind);

            assert_eq!(response["ok"], true);
            assert_eq!(response["result"]["shell"], "html-desktop-shell");
            assert_eq!(response["result"]["backend"], "wayland-layer-shell");
            assert_eq!(response["result"]["bridgeVersion"], 5);
            assert_eq!(
                response["result"]["surface"]["kind"],
                context(kind).kind().wire_name()
            );
            assert_eq!(response["result"]["surface"]["monitorIndex"], 7);
            assert_eq!(response["result"]["surface"]["output"], "eDP-1");
        }
        assert_eq!(BRIDGE_VERSION, 5);
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
            let response = response_value(
                &handle_native_request(
                    request,
                    &context(SurfaceKind::Bar),
                    || panic!("array envelope must not read state"),
                    |_| panic!("array envelope must not focus a workspace"),
                    |_| panic!("array envelope must not change Quick Panel"),
                    |_| panic!("array envelope must not change Diagnostics"),
                )
                .into_response(),
            );
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

    #[test]
    fn desktop_methods_reject_nonobject_and_unexpected_authority_before_dispatch() {
        for method in &ALL_METHODS[6..] {
            for params in [
                serde_json::json!([]),
                serde_json::json!([true]),
                serde_json::json!(null),
                serde_json::json!({"busName": "org.freedesktop.DBus", "objectPath": "/"}),
            ] {
                let request =
                    serde_json::json!({"id": "invalid", "method": method, "params": params});
                let response = handle_for(&request.to_string(), SurfaceKind::Bar);
                assert_eq!(response["error"]["code"], "invalid_params", "{method}");
            }
        }
    }

    #[test]
    fn desktop_numeric_and_identity_boundaries_prevent_dispatch() {
        for (method, params) in [
            ("setAudioVolume", serde_json::json!({"volume": 101})),
            ("setAudioVolume", serde_json::json!({"volume": -1})),
            ("setAudioVolume", serde_json::json!({"volume": 0.5})),
            ("setAudioMuted", serde_json::json!({"muted": 1})),
            ("activateTrayItem", serde_json::json!({"itemId": ""})),
            (
                "scrollTrayItem",
                serde_json::json!({"itemId": "1", "delta": 120, "orientation": "diagonal"}),
            ),
            (
                "dismissNotification",
                serde_json::json!({"notificationId": 0}),
            ),
            (
                "invokeNotificationAction",
                serde_json::json!({"notificationId": 1, "actionKey": ""}),
            ),
            (
                "searchApplications",
                serde_json::json!({"query": "", "limit": 0}),
            ),
            (
                "searchApplications",
                serde_json::json!({"query": "", "limit": 51}),
            ),
            (
                "searchApplications",
                serde_json::json!({"query": "x".repeat(257), "limit": 1}),
            ),
            (
                "launchApplication",
                serde_json::json!({"applicationId": "x", "command": "rm"}),
            ),
        ] {
            let request = serde_json::json!({"id": "boundary", "method": method, "params": params});
            let response = handle_for(&request.to_string(), SurfaceKind::QuickPanel);
            assert_eq!(
                response["error"]["code"], "invalid_params",
                "{method}: {params}"
            );
        }
        for volume in [0, 100] {
            let request = serde_json::json!({"id": "volume", "method": "setAudioVolume", "params": {"volume": volume}});
            let dispatch = handle_native_request(
                &request.to_string(),
                &context(SurfaceKind::Bar),
                || panic!("volume request must not read aggregate state"),
                |_| panic!("volume request must not change workspace"),
                |_| panic!("volume request must not open surfaces"),
                |_| panic!("volume request must not open diagnostics"),
            );
            assert!(
                matches!(dispatch, NativeDispatch::Desktop { request: DesktopRequest::SetAudioVolume { volume: value }, .. } if value == volume)
            );
        }
    }

    #[test]
    fn notification_action_bounds_count_unicode_characters_not_bytes() {
        let key = "打开".repeat(100);
        let parsed = desktop_request(
            "invokeNotificationAction",
            &serde_json::json!({"notificationId": 1, "actionKey": key}),
        )
        .unwrap();
        assert!(
            matches!(parsed, DesktopRequest::InvokeNotificationAction { action_key, .. } if action_key == key)
        );
        assert!(
            desktop_request(
                "invokeNotificationAction",
                &serde_json::json!({"notificationId": 1, "actionKey": "开".repeat(257)}),
            )
            .is_err()
        );
    }
}
