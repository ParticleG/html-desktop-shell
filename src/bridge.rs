use crate::{messages, providers::ProviderRegistry};

use webkit6::prelude::*;

const HANDLER_NAME: &str = "shell";

pub fn attach_bridge(
    web_view: &webkit6::WebView,
    providers: ProviderRegistry,
    panel_context: messages::PanelContext,
) -> Result<(), &'static str> {
    let Some(manager) = web_view.user_content_manager() else {
        return Err("missing WebKit user content manager");
    };

    manager.connect_script_message_with_reply_received(
        Some(HANDLER_NAME),
        move |_manager, value, reply| {
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
                || providers.snapshot(),
                |workspace_id| providers.focus_workspace(workspace_id),
                &panel_context,
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
