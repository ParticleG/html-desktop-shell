use std::{cell::RefCell, rc::Weak};

use crate::{
    messages,
    providers::ProviderRegistry,
    surface::{SurfaceContext, SurfaceError, SurfaceKind},
    surface_manager::SurfaceManager,
    tray_overlay::{NativeMenuTrigger, TriggerCapture},
    web_security::WebViewTrust,
};

use webkit6::prelude::*;

const HANDLER_NAME: &str = "shell";

pub fn attach_bridge(
    web_view: &webkit6::WebView,
    window: &gtk4::ApplicationWindow,
    presentation: &gtk4::Widget,
    trust: WebViewTrust,
    providers: ProviderRegistry,
    surface_context: SurfaceContext,
    surface_manager: Weak<RefCell<SurfaceManager>>,
) -> Result<(), &'static str> {
    let Some(manager) = web_view.user_content_manager() else {
        return Err("missing WebKit user content manager");
    };
    let weak_web_view = web_view.downgrade();
    let weak_window = window.downgrade();
    let weak_presentation = presentation.downgrade();
    let trigger = matches!(
        surface_context.kind(),
        SurfaceKind::Bar | SurfaceKind::QuickPanel
    )
    .then(|| TriggerCapture::install(window, web_view, trust.clone(), surface_context.kind()));

    manager.connect_script_message_with_reply_received(
        Some(HANDLER_NAME),
        move |_manager, value, reply| {
            let Some(web_view) = weak_web_view.upgrade() else {
                reply.return_error_message("WebView is no longer available");
                return true;
            };
            let Some(window) = weak_window.upgrade() else {
                reply.return_error_message("native window is no longer available");
                return true;
            };
            let Some(presentation) = weak_presentation.upgrade() else {
                reply.return_error_message("native presentation is no longer available");
                return true;
            };
            if !trust.allows_bridge(&web_view) {
                reply.return_error_message("native bridge is unavailable for untrusted content");
                return true;
            }
            if surface_manager.upgrade().is_none_or(|manager| {
                !manager.borrow().surface_view_is_current(
                    &surface_context,
                    &presentation,
                    &web_view,
                )
            }) {
                reply.return_error_message("native surface has been retired");
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
            let dispatch = messages::handle_native_request(
                raw_request.as_str(),
                &surface_context,
                || providers.snapshot_for(surface_context.kind()),
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
            let reply = reply.clone();
            let weak_web_view = weak_web_view.clone();
            let response_trust = trust.clone();
            let response_identity = (!matches!(&dispatch, messages::NativeDispatch::Response(_)))
                .then(|| {
                    (
                        weak_presentation.clone(),
                        surface_manager.clone(),
                        surface_context.clone(),
                    )
                });
            let response = move |response_json: String| {
                let Some(web_view) = weak_web_view.upgrade() else {
                    return;
                };
                if !response_trust.allows_bridge(&web_view) {
                    reply
                        .return_error_message("native bridge is unavailable for untrusted content");
                    return;
                }
                // Synchronous surface operations may intentionally retire their validated caller.
                // Only asynchronous operations need a second identity check before replying.
                if let Some((presentation, manager, surface)) = response_identity
                    && !presentation.upgrade().zip(manager.upgrade()).is_some_and(
                        |(presentation, manager)| {
                            manager.borrow().surface_view_is_current(
                                &surface,
                                &presentation,
                                &web_view,
                            )
                        },
                    )
                {
                    return;
                }
                let result = javascriptcore6::Value::new_string(&context, Some(&response_json));
                reply.return_value(&result);
            };
            match dispatch {
                messages::NativeDispatch::Response(response_json) => response(response_json),
                messages::NativeDispatch::Desktop { id, request } => {
                    if surface_context.kind() == SurfaceKind::TrayMenu {
                        let session = surface_manager
                            .upgrade()
                            .ok_or_else(|| surface_unavailable(&surface_context))
                            .and_then(|manager| {
                                SurfaceManager::tray_menu_session(
                                    &manager,
                                    &surface_context,
                                    &presentation,
                                    &web_view,
                                )
                            });
                        match session {
                            Ok(session) => {
                                providers.execute_tray_menu(session, request, move |result| {
                                    response(messages::desktop_response(&id, result));
                                })
                            }
                            Err(error) => response(messages::desktop_response(&id, Err(error))),
                        }
                        return true;
                    }
                    let resources = surface_manager
                        .upgrade()
                        .ok_or_else(|| surface_unavailable(&surface_context))
                        .and_then(|manager| {
                            SurfaceManager::desktop_context(&manager, &surface_context, &window)
                                .map_err(native_surface_error)
                        });
                    match resources {
                        Ok((point, launch_context)) => providers.execute_desktop(
                            request,
                            point,
                            launch_context,
                            move |result| response(messages::desktop_response(&id, result)),
                        ),
                        Err(error) => response(messages::desktop_response(&id, Err(error))),
                    }
                }
                messages::NativeDispatch::ShowTrayMenu { id, item_id } => {
                    match surface_manager.upgrade() {
                        Some(manager) => SurfaceManager::show_tray_menu(
                            &manager,
                            &surface_context,
                            NativeMenuTrigger {
                                window: &window,
                                web_view: &web_view,
                                trust: trust.clone(),
                                capture: trigger
                                    .as_ref()
                                    .expect("tray opening is scoped to an interactive surface"),
                            },
                            item_id,
                            move |result| response(messages::desktop_response(&id, result)),
                        ),
                        None => response(messages::desktop_response(
                            &id,
                            Err(surface_unavailable(&surface_context)),
                        )),
                    }
                }
                messages::NativeDispatch::CloseTrayMenu { id } => {
                    let outcome = surface_manager
                        .upgrade()
                        .ok_or_else(|| surface_unavailable(&surface_context))
                        .and_then(|manager| {
                            SurfaceManager::tray_menu_session(
                                &manager,
                                &surface_context,
                                &presentation,
                                &web_view,
                            )
                        });
                    let close = outcome.is_ok();
                    response(messages::desktop_response(
                        &id,
                        outcome.map(|_| serde_json::Value::Null),
                    ));
                    if close {
                        let weak_manager = surface_manager.clone();
                        let weak_presentation = presentation.downgrade();
                        let weak_view = web_view.downgrade();
                        let context = surface_context.clone();
                        // Reply first; teardown must not turn a successful close into a revoked-view error.
                        glib::idle_add_local_once(move || {
                            if let Some((presentation, view)) =
                                weak_presentation.upgrade().zip(weak_view.upgrade())
                            {
                                SurfaceManager::retire_tray_menu_view(
                                    &weak_manager,
                                    &context,
                                    &presentation,
                                    &view,
                                );
                            }
                        });
                    }
                }
            }
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
