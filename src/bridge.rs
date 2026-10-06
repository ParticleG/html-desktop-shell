use std::{cell::RefCell, rc::Weak};

use crate::{
    messages,
    providers::ProviderRegistry,
    surface::{SurfaceContext, SurfaceError},
    surface_manager::SurfaceManager,
    web_security::WebViewTrust,
};

use webkit6::prelude::*;

const HANDLER_NAME: &str = "shell";

pub fn attach_bridge(
    web_view: &webkit6::WebView,
    trust: WebViewTrust,
    providers: ProviderRegistry,
    surface_context: SurfaceContext,
    surface_manager: Weak<RefCell<SurfaceManager>>,
) -> Result<(), &'static str> {
    let Some(manager) = web_view.user_content_manager() else {
        return Err("missing WebKit user content manager");
    };
    let weak_web_view = web_view.downgrade();

    manager.connect_script_message_with_reply_received(
        Some(HANDLER_NAME),
        move |_manager, value, reply| {
            let Some(web_view) = weak_web_view.upgrade() else {
                reply.return_error_message("WebView is no longer available");
                return true;
            };
            if !trust.allows_bridge(&web_view) {
                reply.return_error_message("native bridge is unavailable for untrusted content");
                return true;
            }

            let Some(context) = value.context() else {
                reply.return_error_message("missing JavaScriptCore context");
                return true;
            };
            let raw_request = value
                .to_json(0)
                .map(|json| json.to_string())
                .unwrap_or_else(|| "null".to_owned());
            let response_json = messages::handle_native_request(
                raw_request.as_str(),
                &surface_context,
                || providers.snapshot(),
                |workspace_index| providers.focus_workspace(workspace_index),
                |open| {
                    let Some(manager) = surface_manager.upgrade() else {
                        return Err(surface_unavailable(&surface_context));
                    };
                    SurfaceManager::set_quick_panel_open(&manager, &surface_context, open)
                        .map(|open| serde_json::json!({ "open": open }))
                        .map_err(native_surface_error)
                },
                |open| {
                    let Some(manager) = surface_manager.upgrade() else {
                        return Err(surface_unavailable(&surface_context));
                    };
                    SurfaceManager::set_diagnostics_dialog_open(&manager, &surface_context, open)
                        .map(|open| serde_json::json!({ "open": open }))
                        .map_err(native_surface_error)
                },
            );
            let result = javascriptcore6::Value::new_string(&context, Some(response_json.as_str()));
            reply.return_value(&result);
            true
        },
    );

    if !manager.register_script_message_handler_with_reply(HANDLER_NAME, None) {
        return Err("failed to register WebKit script message handler: shell");
    }

    Ok(())
}

fn surface_unavailable(context: &SurfaceContext) -> messages::NativeMethodError {
    messages::NativeMethodError::new(
        "surface_unavailable",
        format!("monitor {} is no longer available", context.monitor_index()),
    )
}

fn native_surface_error(error: SurfaceError) -> messages::NativeMethodError {
    messages::NativeMethodError::new(error.code(), error.message())
}
