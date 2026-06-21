pub const BRIDGE_VERSION: u32 = 2;
const METHOD_GET_CAPABILITIES: &str = "getCapabilities";
const METHOD_GET_HOST_INFO: &str = "getHostInfo";
const METHOD_GET_STATE: &str = "getState";
const METHOD_NIRI_FOCUS_WORKSPACE: &str = "niriFocusWorkspace";
const METHODS: &[&str] = &[
    METHOD_GET_HOST_INFO,
    METHOD_GET_CAPABILITIES,
    METHOD_GET_STATE,
    METHOD_NIRI_FOCUS_WORKSPACE,
];

pub fn capabilities() -> serde_json::Value {
    serde_json::json!({ "methods": METHODS })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PanelContext {
    pub index: u32,
    pub output: Option<String>,
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

pub fn handle_native_request<S, A>(
    raw: &str,
    state_snapshot: S,
    focus_workspace: A,
    panel_context: &PanelContext,
) -> String
where
    S: FnOnce() -> serde_json::Value,
    A: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>,
{
    match serde_json::from_str::<NativeRequest>(raw) {
        Ok(request) => handle_request(&request, state_snapshot, focus_workspace, panel_context),
        Err(_) => bad_request_response(raw),
    }
}

fn handle_request<S, A>(
    request: &NativeRequest,
    state_snapshot: S,
    focus_workspace: A,
    panel_context: &PanelContext,
) -> String
where
    S: FnOnce() -> serde_json::Value,
    A: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>,
{
    let response = match request.method.as_str() {
        METHOD_GET_HOST_INFO => ok_response(
            request.id.as_str(),
            serde_json::json!({
                "shell": "html-desktop-shell",
                "backend": "wayland-layer-shell",
                "bridgeVersion": BRIDGE_VERSION,
                "panel": {
                    "index": panel_context.index,
                    "output": panel_context.output.as_deref(),
                },
            }),
        ),
        METHOD_GET_CAPABILITIES => ok_response(request.id.as_str(), capabilities()),
        METHOD_GET_STATE => ok_response(request.id.as_str(), state_snapshot()),
        METHOD_NIRI_FOCUS_WORKSPACE => {
            match workspace_id_param(&request.params).and_then(focus_workspace) {
                Ok(result) => ok_response(request.id.as_str(), result),
                Err(error) => error_response(request.id.as_str(), error.code, error.message),
            }
        }
        method => error_response(
            request.id.as_str(),
            "unknown_method",
            format!("unknown native method: {method}"),
        ),
    };

    serialize_response(&response)
}

fn workspace_id_param(params: &serde_json::Value) -> Result<u64, NativeMethodError> {
    let Some(workspace_id) = params
        .get("workspaceId")
        .and_then(serde_json::Value::as_u64)
    else {
        return Err(NativeMethodError::new(
            "invalid_params",
            "params.workspaceId must be a positive integer",
        ));
    };

    if workspace_id == 0 {
        return Err(NativeMethodError::new(
            "invalid_params",
            "params.workspaceId must be a positive integer",
        ));
    }

    Ok(workspace_id)
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

    fn test_panel_context() -> PanelContext {
        PanelContext {
            index: 7,
            output: Some("eDP-1".to_owned()),
        }
    }

    fn handle(raw: &str) -> serde_json::Value {
        handle_with_action(raw, |workspace_id| {
            Ok(serde_json::json!({ "workspaceId": workspace_id }))
        })
    }

    fn handle_with_action<F>(raw: &str, focus_workspace: F) -> serde_json::Value
    where
        F: FnOnce(u64) -> Result<serde_json::Value, NativeMethodError>,
    {
        response_value(&handle_native_request(
            raw,
            test_state,
            focus_workspace,
            &test_panel_context(),
        ))
    }

    #[test]
    fn parses_valid_get_host_info_request() {
        let request: NativeRequest =
            serde_json::from_str(r#"{"id":"1","method":"getHostInfo","params":{"ignored":true}}"#)
                .expect("request should parse");

        assert_eq!(request.id, "1");
        assert_eq!(request.method, METHOD_GET_HOST_INFO);
        assert!(request.params.is_object());
    }

    #[test]
    fn get_host_info_returns_versioned_backend() {
        let response = handle(r#"{"id":"1","method":"getHostInfo"}"#);

        assert_eq!(response["id"], "1");
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["shell"], "html-desktop-shell");
        assert_eq!(response["result"]["backend"], "wayland-layer-shell");
        assert_eq!(response["result"]["bridgeVersion"], BRIDGE_VERSION);
        assert_eq!(response["result"]["panel"]["index"], 7);
        assert_eq!(response["result"]["panel"]["output"], "eDP-1");
        assert!(response.get("error").is_none());
    }

    #[test]
    fn get_capabilities_returns_supported_methods() {
        let response = handle(r#"{"id":"2","method":"getCapabilities"}"#);
        let methods = response["result"]["methods"]
            .as_array()
            .expect("methods should be an array");

        assert_eq!(response["ok"], true);
        assert_eq!(methods.len(), 4);
        assert_eq!(methods[0], METHOD_GET_HOST_INFO);
        assert_eq!(methods[1], METHOD_GET_CAPABILITIES);
        assert_eq!(methods[2], METHOD_GET_STATE);
        assert_eq!(methods[3], METHOD_NIRI_FOCUS_WORKSPACE);
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
        ];

        for method in forbidden {
            assert!(!METHODS.contains(&method));
        }
    }

    #[test]
    fn get_state_returns_provider_snapshot() {
        let response = handle(r#"{"id":"state","method":"getState"}"#);

        assert_eq!(response["id"], "state");
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["clock"]["time"], "12:34:56");
        assert_eq!(response["result"]["host"]["monitorCount"], 2);
        assert_eq!(response["result"]["niri"]["available"], false);
    }

    #[test]
    fn niri_focus_workspace_returns_action_result() {
        let response =
            handle(r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceId":3}}"#);

        assert_eq!(response["id"], "focus");
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["workspaceId"], 3);
    }

    #[test]
    fn niri_focus_workspace_rejects_missing_workspace_id() {
        let response = handle_with_action(
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{}}"#,
            |_| panic!("invalid params must not call the action"),
        );

        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "invalid_params");
        assert_eq!(
            response["error"]["message"],
            "params.workspaceId must be a positive integer"
        );
    }

    #[test]
    fn niri_focus_workspace_rejects_zero_workspace_id() {
        let response = handle_with_action(
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceId":0}}"#,
            |_| panic!("invalid params must not call the action"),
        );

        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "invalid_params");
    }

    #[test]
    fn niri_focus_workspace_rejects_unknown_workspace_id() {
        let response = handle_with_action(
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceId":9}}"#,
            |workspace_id| {
                Err(NativeMethodError::new(
                    "unknown_workspace",
                    format!("workspace id {workspace_id} is not in the latest niri snapshot"),
                ))
            },
        );

        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "unknown_workspace");
        assert_eq!(
            response["error"]["message"],
            "workspace id 9 is not in the latest niri snapshot"
        );
    }

    #[test]
    fn niri_focus_workspace_reports_unavailable_niri() {
        let response = handle_with_action(
            r#"{"id":"focus","method":"niriFocusWorkspace","params":{"workspaceId":1}}"#,
            |_| {
                Err(NativeMethodError::new(
                    "niri_unavailable",
                    "niri IPC unavailable",
                ))
            },
        );

        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "niri_unavailable");
        assert_eq!(response["error"]["message"], "niri IPC unavailable");
    }

    #[test]
    fn unknown_method_returns_error() {
        let response = handle(r#"{"id":"3","method":"launch"}"#);

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
        let response = handle(r#"{"id":"4","method":7}"#);

        assert_eq!(response["id"], "4");
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "bad_request");
        assert_eq!(
            response["error"]["message"],
            "request must be a JSON object with string id and method"
        );
    }

    #[test]
    fn malformed_json_without_id_uses_empty_response_id() {
        let response = handle("not json");

        assert_eq!(response["id"], "");
        assert_eq!(response["ok"], false);
        assert_eq!(response["error"]["code"], "bad_request");
    }
}
